//! The `gui-install` scenario: it installs through the screens of the InayOS installer, driven
//! by QMP key presses only, as a person at the keyboard would.
//!
//! The installer writes the name of its current screen to [`SCREEN_FILE`]. After each input,
//! the xtask reads that file over SSH as root until it names the expected screen, then saves a
//! screenshot of it and checks that the screenshot is not a single color.
//!
//! The plan follows the installer's keyboard contract: Enter goes to the next screen or runs
//! the screen's main action, Tab moves to the next field, and a screen that opens gives its
//! first field the focus. The defaults of the screens stay: the us layout, the first disk with
//! its first available disk choice, the suggested username, the computer name `inayos`, the
//! modules of the profile, and the built-in answers to the module questions.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Child, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use super::{LIVE_LOG, Scenario, StepError};
use crate::os::{GUEST_PASSWORD, GUEST_USER};
use crate::ppm;
use crate::ssh::{Guest, SshError};

/// The installer writes its current screen here on the live system.
pub const SCREEN_FILE: &str = "/run/inayos-installer/screen";
/// The full name typed on the user screen. The installer suggests the username from it, which
/// is the test account's.
const FULL_NAME: &str = "Tester";
/// The word that the summary screen needs before it enables Install.
const CONFIRMATION: &str = "erase";
/// Where the live system keeps the installer, and the command that the Sway session of the
/// stick runs on the installer workspace, as `test/usb/profile` sets it.
const LIVE_INSTALLER: &str = "/usr/local/bin/inayos-installer";
const SWAY_START: &str = "workspace 1:Installer; exec /usr/local/bin/inayos-start-installer";
/// The ext4 label of the home partition that the wipe-disk layout creates.
const HOME_LABEL: &str = "inayos-home";

const FIRST_SCREEN_TIMEOUT: Duration = Duration::from_secs(5 * 60);
const SCREEN_TIMEOUT: Duration = Duration::from_secs(2 * 60);
const INSTALL_TIMEOUT: Duration = Duration::from_secs(90 * 60);
/// A screen draws its first frame a moment after it writes its name.
const SCREEN_SETTLE: Duration = Duration::from_secs(3);
/// The pause between two inputs, so a moved focus has landed before the next key.
const INPUT_PAUSE: Duration = Duration::from_millis(500);
const SCREEN_POLL: Duration = Duration::from_secs(1);

/// One input of the plan.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Input {
    /// A key press, named by its QEMU key code.
    Key(&'static str),
    /// Text typed one key at a time on the us layout.
    Text(String),
}

/// A screen of the plan and the inputs given on it. The inputs of one screen lead to the next
/// screen of the plan.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Screen {
    pub name: &'static str,
    pub inputs: Vec<Input>,
}

/// The screens that an install on a machine with a wired network goes through, in order. The
/// Wi-Fi screen is skipped because the network is already connected. The last screen is the
/// progress screen, which the install leaves by itself for `done` or `failed`.
pub fn plan(password: &str) -> Vec<Screen> {
    let next = || vec![Input::Key("ret")];
    vec![
        Screen {
            name: "keyboard",
            inputs: next(),
        },
        Screen {
            name: "disk",
            inputs: next(),
        },
        Screen {
            name: "security",
            inputs: next(),
        },
        Screen {
            name: "user",
            inputs: vec![
                Input::Text(FULL_NAME.to_owned()),
                // The username field keeps the suggestion.
                Input::Key("tab"),
                Input::Key("tab"),
                Input::Text(password.to_owned()),
                Input::Key("tab"),
                Input::Text(password.to_owned()),
                Input::Key("ret"),
            ],
        },
        Screen {
            name: "modules",
            inputs: next(),
        },
        Screen {
            name: "summary",
            inputs: vec![Input::Text(CONFIRMATION.to_owned()), Input::Key("ret")],
        },
        Screen {
            name: "progress",
            inputs: Vec::new(),
        },
    ]
}

/// A `tail -F` of the installer's log on the live system, copied to a file of the layer as the
/// log grows. It stops when dropped.
struct LogStream {
    child: Child,
}

impl LogStream {
    fn start(live: &Guest, local: &Path) -> Result<LogStream, StepError> {
        if let Some(parent) = local.parent() {
            fs::create_dir_all(parent).map_err(|error| StepError::Io(parent.to_owned(), error))?;
        }
        let file =
            fs::File::create(local).map_err(|error| StepError::Io(local.to_owned(), error))?;
        let child = live
            .command(&format!("tail -n +1 -F {LIVE_LOG} 2>/dev/null"), false)
            .stdin(Stdio::null())
            .stdout(file)
            .stderr(Stdio::null())
            .spawn()
            .map_err(|error| StepError::Ssh(SshError::Spawn("ssh", error)))?;
        Ok(LogStream { child })
    }
}

impl Drop for LogStream {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Scenario {
    /// Boots the stick, replaces its installer with the one built from the working tree,
    /// installs through the screens, and powers the live system off.
    pub(super) fn install_through_gui(&mut self) -> Result<(), StepError> {
        let live = self.boot_live()?;
        println!("vm: waiting for the installer of the stick on its keyboard screen");
        if let Err(error) = self.wait_for_screen(&live, "keyboard", FIRST_SCREEN_TIMEOUT) {
            return Err(explain_first_wait(&live, error));
        }
        self.restart_fresh_installer(&live)?;
        self.wait_for_screen(&live, "keyboard", SCREEN_TIMEOUT)?;

        let ended = self.go_through_screens(&live);
        // The log helps most after a failure, so it is copied whatever the outcome.
        if let Err(error) = live.copy_from(LIVE_LOG, &self.log("installer-gui-file")) {
            eprintln!("vm: could not copy {LIVE_LOG}: {error}");
        }
        let last = plan(GUEST_PASSWORD).len();
        if ended? == "failed" {
            if let Err(error) = self.screen_shot(last, "failed") {
                eprintln!("vm: {error}");
            }
            return Err(StepError::Check(format!(
                "the installer showed its failed screen; its log is in {}",
                self.log("installer-gui-file").display()
            )));
        }
        self.screen_shot(last, "done")?;
        self.add_key_to_new_home(&live)?;
        self.power_off(&live, "systemctl poweroff")
    }

    /// Starts the installer built from the working tree in the stick's Sway session, goes from
    /// the keyboard screen to the disk screen, and saves it as `shot` with the single-color
    /// check. The wired network of the guest skips the Wi-Fi screen. The installer stays open.
    pub(super) fn show_disk_screen(&mut self, live: &Guest, shot: &str) -> Result<(), StepError> {
        self.wait_for_stick_installer(live)?;
        self.restart_fresh_installer(live)?;
        self.wait_for_screen(live, "keyboard", SCREEN_TIMEOUT)?;
        thread::sleep(SCREEN_SETTLE);
        self.vm().qmp()?.send_keys(&["ret"])?;
        self.wait_for_screen(live, "disk", SCREEN_TIMEOUT)?;
        self.checked_screenshot(shot, "disk")
    }

    /// Waits until the installer that the stick's Sway session starts shows its keyboard
    /// screen. Its disk inventory has then ended, including the read-only mount of a Windows
    /// EFI partition, so a command-line install that starts afterwards finds nothing mounted.
    pub(super) fn wait_for_stick_installer(&mut self, live: &Guest) -> Result<(), StepError> {
        println!("vm: waiting for the installer of the stick on its keyboard screen");
        self.wait_for_screen(live, "keyboard", FIRST_SCREEN_TIMEOUT)
            .map_err(|error| explain_first_wait(live, error))
    }

    /// Gives each screen of the plan its inputs, then waits until the install ends, and
    /// returns the screen it ended on. The installer's log streams to the layer meanwhile.
    fn go_through_screens(&mut self, live: &Guest) -> Result<&'static str, StepError> {
        for (index, screen) in plan(GUEST_PASSWORD).iter().enumerate() {
            if index > 0 {
                self.wait_for_screen(live, screen.name, SCREEN_TIMEOUT)?;
            }
            println!("vm: on the {} screen", screen.name);
            self.screen_shot(index, screen.name)?;
            let mut qmp = self.vm().qmp()?;
            for input in &screen.inputs {
                match input {
                    Input::Key(key) => qmp.send_keys(&[key])?,
                    Input::Text(text) => qmp.type_text(text)?,
                }
                thread::sleep(INPUT_PAUSE);
            }
        }
        let log = self.log("installer-gui");
        println!("vm: the install runs; its log streams to {}", log.display());
        let _stream = LogStream::start(live, &log)?;
        self.wait_for_end(live)
    }

    /// Stops the installer that the Sway session started from the stick, puts the binaries
    /// built from the working tree in place of the stick's copies, and starts the fresh installer the
    /// way the session does.
    fn restart_fresh_installer(&mut self, live: &Guest) -> Result<(), StepError> {
        println!("vm: replacing the stick's installer with the one built from the working tree");
        // The fresh installer takes the old one's place on the installer workspace.
        let _ = live.run(&format!("pkill -x -f {LIVE_INSTALLER}"));
        self.wait_until(
            "the stick's installer stopped",
            SCREEN_TIMEOUT,
            || match live.run(&format!("pgrep -x -f {LIVE_INSTALLER}")) {
                Ok(pids) => Ok(Err(format!("still running: {}", pids.trim()))),
                Err(SshError::Failed { .. }) => Ok(Ok(())),
                Err(error) => Err(error.into()),
            },
        )?;
        live.run(&format!("rm -f {SCREEN_FILE}"))?;
        self.copy_binaries(live)?;
        let start = format!(
            "socket=$(ls /run/user/$(id -u live)/sway-ipc.*.sock | head -n 1) && \
             sudo -u live SWAYSOCK=\"$socket\" swaymsg '{SWAY_START}'"
        );
        live.run(&start)?;
        Ok(())
    }

    /// Reads the screen file until it names `expected`. The failed screen ends the wait at once.
    fn wait_for_screen(
        &mut self,
        live: &Guest,
        expected: &'static str,
        timeout: Duration,
    ) -> Result<(), StepError> {
        let deadline = Instant::now() + timeout;
        let mut last = None;
        loop {
            self.vm().check_running()?;
            let screen = read_screen(live)?;
            if screen.as_deref() == Some(expected) {
                return Ok(());
            }
            if screen.as_deref() == Some("failed") {
                return Err(StepError::Check(format!(
                    "the installer showed its failed screen while the {expected} screen was expected"
                )));
            }
            last = screen.or(last);
            if Instant::now() >= deadline {
                return Err(StepError::NotReached {
                    what: format!("the installer did not show its {expected} screen"),
                    timeout,
                    last: last.map(|screen| format!("screen {screen}")),
                });
            }
            thread::sleep(SCREEN_POLL);
        }
    }

    /// Waits until the install ends on the done or the failed screen, and returns which.
    fn wait_for_end(&mut self, live: &Guest) -> Result<&'static str, StepError> {
        let deadline = Instant::now() + INSTALL_TIMEOUT;
        loop {
            self.vm().check_running()?;
            match read_screen(live)?.as_deref() {
                Some("done") => return Ok("done"),
                Some("failed") => return Ok("failed"),
                _ => {}
            }
            if Instant::now() >= deadline {
                return Err(StepError::NotReached {
                    what: "the install did not end on the done or failed screen".to_owned(),
                    timeout: INSTALL_TIMEOUT,
                    last: None,
                });
            }
            thread::sleep(super::POLL);
        }
    }

    /// Saves the screen as `gui-<index>-<name>.png` in the layer and checks that it is not a
    /// single color.
    fn screen_shot(&mut self, index: usize, name: &str) -> Result<(), StepError> {
        self.checked_screenshot(&format!("gui-{index:02}-{name}.png"), name)
    }

    /// Saves the screen as `shot` in the layer, once the screen named `name` has drawn, and
    /// checks that it is not a single color.
    fn checked_screenshot(&mut self, shot: &str, name: &str) -> Result<(), StepError> {
        thread::sleep(SCREEN_SETTLE);
        self.screenshot(shot)?;
        let check: PathBuf = self.layer.dir().join("screen-check.ppm");
        self.vm().qmp()?.screendump_ppm(&check)?;
        let bytes = fs::read(&check).map_err(|error| StepError::Io(check.clone(), error))?;
        fs::remove_file(&check).map_err(|error| StepError::Io(check.clone(), error))?;
        match ppm::is_single_color(&bytes) {
            Ok(false) => Ok(()),
            Ok(true) => Err(StepError::Check(format!(
                "the {name} screen is a single color, so nothing drew on it; see {shot}"
            ))),
            Err(error) => Err(StepError::Check(format!(
                "could not read the screenshot of the {name} screen: {error}"
            ))),
        }
    }

    /// Adds the xtask's key to the new user's `authorized_keys` on the home partition, so the
    /// checks can log in over SSH. The screens take no SSH key, so this is the one change
    /// that does not go through them.
    fn add_key_to_new_home(&mut self, live: &Guest) -> Result<(), StepError> {
        println!("vm: adding the xtask's key to the new home folder");
        let home = format!("/run/inayos-home/{GUEST_USER}");
        live.run(&format!(
            "set -e; mkdir -p /run/inayos-home; mount /dev/disk/by-label/{HOME_LABEL} /run/inayos-home; \
             owner=$(stat -c %u:%g {home}); install -d -m 700 -o \"${{owner%:*}}\" -g \"${{owner#*:}}\" {home}/.ssh; \
             printf '%s\\n' '{key}' >> {home}/.ssh/authorized_keys; \
             chown \"$owner\" {home}/.ssh/authorized_keys; chmod 600 {home}/.ssh/authorized_keys; \
             umount /run/inayos-home",
            key = self.public_key
        ))?;
        Ok(())
    }
}

/// Adds to a missed first keyboard screen whether any installer runs, which tells an image
/// whose Sway session does not start the installer apart from an installer that writes no
/// screen file.
fn explain_first_wait(live: &Guest, error: StepError) -> StepError {
    let StepError::NotReached {
        what,
        timeout,
        last,
    } = error
    else {
        return error;
    };
    let cause = match live.run(&format!("pgrep -x -f {LIVE_INSTALLER}")) {
        Ok(_) => "the installer runs but wrote no keyboard screen",
        Err(SshError::Failed { .. }) => {
            "no installer runs, so the Sway session of the stick did not start it; an image built \
             before the current test/usb/profile does not, and `cargo xtask vm usb build` makes \
             a fresh one"
        }
        Err(_) => "whether an installer runs could not be read",
    };
    StepError::NotReached {
        what: format!("{what} ({cause})"),
        timeout,
        last,
    }
}

/// The screen the installer shows, or none before it wrote the file.
fn read_screen(live: &Guest) -> Result<Option<String>, StepError> {
    match live.run(&format!("cat {SCREEN_FILE}")) {
        Ok(text) => Ok(Some(text.trim().to_owned()).filter(|screen| !screen.is_empty())),
        Err(SshError::Failed { .. }) => Ok(None),
        Err(error) => Err(error.into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_plan_visits_every_screen_of_an_install_on_a_wired_network_in_order() {
        let names: Vec<&str> = plan("pw").iter().map(|screen| screen.name).collect();
        assert_eq!(
            names,
            [
                "keyboard", "disk", "security", "user", "modules", "summary", "progress"
            ]
        );
    }

    #[test]
    fn the_user_screen_gets_the_password_twice_and_the_summary_the_confirmation_before_enter() {
        let screens = plan("secret");
        let user = &screens[3].inputs;
        let typed: Vec<&Input> = user
            .iter()
            .filter(|input| matches!(input, Input::Text(_)))
            .collect();
        assert_eq!(
            typed,
            [
                &Input::Text(FULL_NAME.to_owned()),
                &Input::Text("secret".to_owned()),
                &Input::Text("secret".to_owned()),
            ]
        );
        assert_eq!(user.last(), Some(&Input::Key("ret")));
        assert_eq!(
            screens[5].inputs,
            [Input::Text("erase".to_owned()), Input::Key("ret")]
        );
    }

    #[test]
    fn every_typed_text_has_keys_on_the_us_layout() {
        for screen in plan(GUEST_PASSWORD) {
            for input in screen.inputs {
                if let Input::Text(text) = input {
                    assert!(
                        text.chars()
                            .all(|character| crate::qmp::key_for(character).is_some()),
                        "{text}"
                    );
                }
            }
        }
    }
}
