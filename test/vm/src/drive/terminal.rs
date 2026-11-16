//! Drives a Linux guest through a pseudo-terminal that SSH opens in the guest. The installer
//! sees a terminal, so it asks its questions and sudo asks for the password, and everything
//! typed goes through the SSH connection.

use std::fs::File;
use std::io::{self, Read, Write};
use std::path::Path;
use std::process::{Child, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::thread;
use std::time::{Duration, Instant};

use super::{DriveError, Driver, MAX_PASSWORD_PROMPTS, TIMEOUT, answer_text, check_exit};
use crate::answers::{Answers, Response};
use crate::os::GUEST_PASSWORD;
use crate::ssh::Guest;
use crate::vm::Vm;

const POLL: Duration = Duration::from_secs(1);
/// The output kept for prompt matching. A question with its settings fits easily.
const PENDING_LIMIT: usize = 16 * 1024;

pub struct Terminal<'a> {
    guest: &'a Guest,
    answers: &'a Answers,
}

impl<'a> Terminal<'a> {
    pub fn new(guest: &'a Guest, answers: &'a Answers) -> Terminal<'a> {
        Terminal { guest, answers }
    }
}

impl Driver for Terminal<'_> {
    fn run(&mut self, vm: &mut Vm, command: &str, log: &Path) -> Result<String, DriveError> {
        let mut file = File::create(log).map_err(|error| DriveError::Io(log.to_owned(), error))?;
        let mut child = self
            .guest
            .command(command, true)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .map_err(DriveError::Spawn)?;
        let result = converse(self.answers, &mut child, vm, command, log, &mut file);
        if result.is_err() {
            // Ending ssh ends the command in the guest, because it loses its terminal.
            let _ = child.kill();
        }
        let output = result?;
        let status = child
            .wait()
            .map_err(|error| DriveError::Io(log.to_owned(), error))?;
        check_exit(command, status.code(), log)?;
        Ok(output)
    }
}

/// Reads the output until ssh closes it, typing into the terminal whenever the installer waits.
fn converse(
    answers: &Answers,
    child: &mut Child,
    vm: &mut Vm,
    command: &str,
    log: &Path,
    file: &mut File,
) -> Result<String, DriveError> {
    let mut input = child.stdin.take().expect("stdin was piped");
    let mut stdout = child.stdout.take().expect("stdout was piped");
    let (sender, chunks) = mpsc::channel();
    thread::spawn(move || {
        let mut buffer = [0; 4096];
        while let Ok(count) = stdout.read(&mut buffer) {
            if count == 0 || sender.send(buffer[..count].to_vec()).is_err() {
                break;
            }
        }
    });
    let conversation = Conversation {
        answers,
        command,
        log,
    };
    conversation.run(&chunks, &mut input, file, &mut io::stdout(), || {
        vm.check_running().map_err(DriveError::VmStopped)
    })
}

/// The exchange with one installer command: what it prints arrives in chunks, and what the
/// installer waits for is typed back.
struct Conversation<'a> {
    answers: &'a Answers,
    command: &'a str,
    log: &'a Path,
}

impl Conversation<'_> {
    /// Reads `chunks` until their sender is gone, copies them to `file` and `echo`, and types
    /// into `input` whenever the installer waits. `check_running` runs whenever no output came
    /// for a while. Returns the whole output without carriage returns.
    fn run(
        &self,
        chunks: &Receiver<Vec<u8>>,
        input: &mut impl Write,
        file: &mut impl Write,
        echo: &mut impl Write,
        mut check_running: impl FnMut() -> Result<(), DriveError>,
    ) -> Result<String, DriveError> {
        let log = self.log;
        let io_error = |error| DriveError::Io(log.to_owned(), error);
        let deadline = Instant::now() + TIMEOUT;
        let mut output = Vec::new();
        let mut pending = String::new();
        let mut passwords = 0;
        loop {
            let chunk = match chunks.recv_timeout(POLL) {
                Ok(chunk) => chunk,
                Err(RecvTimeoutError::Disconnected) => break,
                Err(RecvTimeoutError::Timeout) => {
                    check_running()?;
                    if Instant::now() >= deadline {
                        return Err(DriveError::Timeout {
                            command: self.command.to_owned(),
                            log: log.to_owned(),
                        });
                    }
                    continue;
                }
            };
            file.write_all(&chunk).map_err(io_error)?;
            // The person running the cycle sees the installer output as it comes.
            let _ = echo.write_all(&chunk);
            let _ = echo.flush();
            output.extend_from_slice(&chunk);
            // The guest terminal ends lines with a carriage return and a line feed.
            pending.push_str(&String::from_utf8_lossy(&chunk).replace('\r', ""));

            let typed = match self.answers.respond(&pending) {
                Response::Wait => {
                    keep_end(&mut pending, PENDING_LIMIT);
                    continue;
                }
                Response::Password => {
                    passwords += 1;
                    if passwords > MAX_PASSWORD_PROMPTS {
                        return Err(DriveError::PasswordRefused {
                            log: log.to_owned(),
                        });
                    }
                    format!("{GUEST_PASSWORD}\n")
                }
                Response::Answer { question, yes } => {
                    println!(
                        "\nvm: answering {} to: {question}",
                        if yes { "yes" } else { "no" }
                    );
                    answer_text(yes).to_owned()
                }
                Response::Stop(question) => {
                    return Err(DriveError::UnknownQuestion {
                        question,
                        log: log.to_owned(),
                    });
                }
            };
            input.write_all(typed.as_bytes()).map_err(io_error)?;
            input.flush().map_err(io_error)?;
            pending.clear();
        }
        Ok(String::from_utf8_lossy(&output).replace('\r', ""))
    }
}

/// Drops the start of `text` so that at most about `limit` bytes are left.
fn keep_end(text: &mut String, limit: usize) {
    if text.len() <= limit {
        return;
    }
    let mut start = text.len() - limit;
    while !text.is_char_boundary(start) {
        start += 1;
    }
    text.drain(..start);
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    /// What a conversation typed and returned after the installer printed `chunks` and ended.
    fn talk(chunks: &[&str]) -> (Result<String, DriveError>, String) {
        let answers = Answers::new();
        let log = PathBuf::from("install.log");
        let conversation = Conversation {
            answers: &answers,
            command: "~/.local/bin/myconfig install",
            log: &log,
        };
        let (sender, receiver) = mpsc::channel();
        for chunk in chunks {
            sender.send(chunk.as_bytes().to_vec()).unwrap();
        }
        drop(sender);
        let mut typed = Vec::new();
        let result = conversation.run(
            &receiver,
            &mut typed,
            &mut Vec::new(),
            &mut io::sink(),
            || Ok(()),
        );
        (result, String::from_utf8(typed).unwrap())
    }

    #[test]
    fn the_password_and_known_answers_are_typed_and_the_output_is_returned() {
        // A prompt split across two chunks is answered once it is complete.
        let (result, typed) = talk(&[
            "==> Installing base\r\n[sudo] pass",
            "word for tester: ",
            "\r\n==> Installing ssh\r\nThe OpenSSH server is off. Turn it on now? [y/N] ",
            "y\r\ndone\r\n",
        ]);
        assert_eq!(typed, format!("{GUEST_PASSWORD}\ny\n"));
        assert_eq!(
            result.unwrap(),
            "==> Installing base\n[sudo] password for tester: \n==> Installing ssh\n\
             The OpenSSH server is off. Turn it on now? [y/N] y\ndone\n"
        );
    }

    #[test]
    fn an_unknown_question_stops_without_typing() {
        let (result, typed) =
            talk(&["==> Installing docker\r\nAdd tester to the docker group? [y/N] "]);
        assert_eq!(typed, "");
        assert!(matches!(result, Err(DriveError::UnknownQuestion { .. })));
    }

    #[test]
    fn a_refused_password_stops_after_the_last_allowed_prompt() {
        let prompt = "[sudo] password for tester: ";
        let prompts: Vec<&str> = (0..=MAX_PASSWORD_PROMPTS).map(|_| prompt).collect();
        let (result, typed) = talk(&prompts);
        assert_eq!(
            typed,
            format!("{GUEST_PASSWORD}\n").repeat(MAX_PASSWORD_PROMPTS as usize)
        );
        assert!(matches!(result, Err(DriveError::PasswordRefused { .. })));
    }
}
