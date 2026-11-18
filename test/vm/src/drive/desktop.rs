//! Drives the Windows guest through its logged-in desktop, where the installer runs
//! unelevated as it does for a person. A scheduled task starts each command in a console
//! window of the desktop session, with its output copied to a log file that the xtask reads
//! over SSH. Keys typed through QMP answer the console and the elevation prompts.
//!
//! No part of this driver has run on a real guest. The two parts most likely to fail are that
//! the console window has the keyboard focus when an answer is typed, and that Alt+Y presses
//! Yes on the elevation prompt.

use std::fs;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, Instant};

use super::{DriveError, Driver, MAX_PASSWORD_PROMPTS, TIMEOUT, answer_text, check_exit};
use crate::answers::{Answers, Response};
use crate::os::{GUEST_PASSWORD, WINDOWS_TOOL_DIR};
use crate::ssh::Guest;
use crate::vm::Vm;

const POLL: Duration = Duration::from_secs(3);
/// The elevation prompt takes a moment to close after Yes. It is not pressed again sooner.
const CONSENT_REPEAT: Duration = Duration::from_secs(10);
const TASK: &str = "inayos-vm";

pub struct Desktop<'a> {
    guest: &'a Guest,
    answers: &'a Answers,
    /// A host folder for the files copied into the guest.
    scratch: PathBuf,
}

impl<'a> Desktop<'a> {
    pub fn new(guest: &'a Guest, answers: &'a Answers, scratch: PathBuf) -> Desktop<'a> {
        Desktop {
            guest,
            answers,
            scratch,
        }
    }

    fn start(&self, command: &str) -> Result<(), DriveError> {
        let output = format!(r"{WINDOWS_TOOL_DIR}\output.log");
        let exit = format!(r"{WINDOWS_TOOL_DIR}\exit.txt");
        let script = format!(
            "@echo off\r\n\
             cd /d \"%USERPROFILE%\"\r\n\
             {command} > \"{output}\" 2>&1\r\n\
             (echo %ERRORLEVEL%)> \"{exit}\"\r\n"
        );
        fs::create_dir_all(&self.scratch)
            .map_err(|error| DriveError::Io(self.scratch.clone(), error))?;
        let local = self.scratch.join("run.cmd");
        fs::write(&local, script).map_err(|error| DriveError::Io(local.clone(), error))?;

        self.guest.run(&format!(
            "(if not exist {WINDOWS_TOOL_DIR} mkdir {WINDOWS_TOOL_DIR}) & \
             (if exist \"{output}\" del \"{output}\") & \
             (if exist \"{exit}\" del \"{exit}\")"
        ))?;
        let run = format!(r"{WINDOWS_TOOL_DIR}\run.cmd");
        self.guest.copy_to(&local, &run.replace('\\', "/"))?;
        // An interactive task of the logged-in user runs in that user's desktop session,
        // unelevated, and needs no stored password.
        self.guest.run(&format!(
            "powershell.exe -NoLogo -NoProfile -NonInteractive -Command \"\
             $action = New-ScheduledTaskAction -Execute 'cmd.exe' -Argument '/c {run}'; \
             $principal = New-ScheduledTaskPrincipal -UserId $env:USERNAME -LogonType Interactive -RunLevel Limited; \
             Register-ScheduledTask -TaskName {TASK} -Action $action -Principal $principal -Force | Out-Null; \
             Start-ScheduledTask -TaskName {TASK}\""
        ))?;
        Ok(())
    }

    fn output(&self) -> Result<String, DriveError> {
        let text = self.guest.run(&format!(
            r#"if exist "{WINDOWS_TOOL_DIR}\output.log" type "{WINDOWS_TOOL_DIR}\output.log""#
        ))?;
        Ok(text.replace('\r', ""))
    }

    fn exit_code(&self) -> Result<Option<i32>, DriveError> {
        let text = self.guest.run(&format!(
            r#"if exist "{WINDOWS_TOOL_DIR}\exit.txt" type "{WINDOWS_TOOL_DIR}\exit.txt""#
        ))?;
        Ok(text.trim().parse().ok())
    }

    fn consent_open(&self) -> Result<bool, DriveError> {
        let text = self
            .guest
            .run(r#"tasklist /FI "IMAGENAME eq consent.exe" /NH"#)?;
        Ok(text.to_ascii_lowercase().contains("consent.exe"))
    }

    fn remove_task(&self) {
        // A task left behind is replaced by the next command, so a failure here changes nothing.
        let _ = self.guest.run(&format!(
            "powershell.exe -NoLogo -NoProfile -NonInteractive -Command \
             \"Unregister-ScheduledTask -TaskName {TASK} -Confirm:$false\""
        ));
    }
}

impl Driver for Desktop<'_> {
    fn run(&mut self, vm: &mut Vm, command: &str, log: &Path) -> Result<String, DriveError> {
        self.start(command)?;
        let mut output = String::new();
        let result = self.follow(vm, command, log, &mut output);
        fs::write(log, &output).map_err(|error| DriveError::Io(log.to_owned(), error))?;
        self.remove_task();
        result.map(|()| output)
    }
}

impl Desktop<'_> {
    /// Watches the started command until it ends, keeping its output so far in `output`.
    fn follow(
        &self,
        vm: &mut Vm,
        command: &str,
        log: &Path,
        output: &mut String,
    ) -> Result<(), DriveError> {
        let deadline = Instant::now() + TIMEOUT;
        let mut answered = 0;
        let mut passwords = 0;
        let mut consent_pressed: Option<Instant> = None;
        loop {
            thread::sleep(POLL);
            vm.check_running().map_err(DriveError::VmStopped)?;

            if self.consent_open()?
                && consent_pressed.is_none_or(|pressed| pressed.elapsed() >= CONSENT_REPEAT)
            {
                println!("vm: pressing Yes on the elevation prompt");
                vm.qmp()?.send_keys(&["alt", "y"])?;
                consent_pressed = Some(Instant::now());
            }

            // The exit code is read first, so the output read after it is complete.
            let exit = self.exit_code()?;
            *output = self.output()?;
            if let Some(code) = exit {
                return check_exit(command, Some(code), log);
            }

            // Only the output since the last answer can hold a waiting prompt. The answers
            // typed into the console do not reach the log file.
            let typed = match self.answers.respond(output.get(answered..).unwrap_or("")) {
                Response::Wait => None,
                Response::Password => {
                    passwords += 1;
                    if passwords > MAX_PASSWORD_PROMPTS {
                        return Err(DriveError::PasswordRefused {
                            log: log.to_owned(),
                        });
                    }
                    Some(format!("{GUEST_PASSWORD}\n"))
                }
                Response::Answer { question, yes } => {
                    println!(
                        "vm: answering {} to: {question}",
                        if yes { "yes" } else { "no" }
                    );
                    Some(answer_text(yes).to_owned())
                }
                Response::Stop(question) => {
                    return Err(DriveError::UnknownQuestion {
                        question,
                        log: log.to_owned(),
                    });
                }
            };
            if let Some(typed) = typed {
                vm.qmp()?.type_text(&typed)?;
                answered = output.len();
            }
            if Instant::now() >= deadline {
                return Err(DriveError::Timeout {
                    command: command.to_owned(),
                    log: log.to_owned(),
                });
            }
        }
    }
}
