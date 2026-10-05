//! Keeps one sudo permission alive for the whole run on Linux.
use std::{
    io::Write,
    process::{Command, Stdio},
    sync::mpsc,
    thread,
    time::Duration,
};

use myconfig_modules::{ModuleResult, PackageSystem};

/// Refreshes the sudo timestamp every 60 seconds until dropped.
pub struct Session {
    stop: Option<mpsc::Sender<()>>,
    worker: Option<thread::JoinHandle<()>>,
}

impl Session {
    fn needed(system: PackageSystem) -> bool {
        system != PackageSystem::Winget
    }

    fn keep_alive() -> Self {
        let (stop, received) = mpsc::channel();
        let worker = thread::spawn(move || {
            // Dropping the session disconnects the channel, which ends the loop.
            while matches!(
                received.recv_timeout(Duration::from_secs(60)),
                Err(mpsc::RecvTimeoutError::Timeout)
            ) {
                let refreshed = Command::new("sudo")
                    .args(["-n", "true"])
                    .stdin(Stdio::null())
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .status();
                if !refreshed.is_ok_and(|status| status.success()) {
                    break;
                }
            }
        });
        Self {
            stop: Some(stop),
            worker: Some(worker),
        }
    }

    fn none() -> Self {
        Self {
            stop: None,
            worker: None,
        }
    }

    /// Asks for the password in the terminal, as `sudo -v` does by itself.
    pub fn start_interactive(system: PackageSystem) -> ModuleResult<Self> {
        if !Self::needed(system) {
            return Ok(Self::none());
        }
        if !Command::new("sudo").arg("-v").status()?.success() {
            return Err("sudo did not grant permission".into());
        }
        Ok(Self::keep_alive())
    }

    /// Whether sudo already has a valid permission, so no password is needed.
    pub fn cached(system: PackageSystem) -> bool {
        !Self::needed(system)
            || Command::new("sudo")
                .args(["-n", "true"])
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .is_ok_and(|status| status.success())
    }

    /// Starts the permission with a password typed into the screen.
    pub fn start_with_password(system: PackageSystem, password: &str) -> ModuleResult<Self> {
        if !Self::needed(system) {
            return Ok(Self::none());
        }
        let mut child = Command::new("sudo")
            .args(["-S", "-v", "-p", ""])
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()?;
        if let Some(mut input) = child.stdin.take() {
            input.write_all(format!("{password}\n").as_bytes())?;
        }
        if !child.wait()?.success() {
            return Err("sudo rejected the password".into());
        }
        Ok(Self::keep_alive())
    }

    /// Keeps an already valid permission alive.
    pub fn start_cached(system: PackageSystem) -> Self {
        if Self::needed(system) {
            Self::keep_alive()
        } else {
            Self::none()
        }
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        drop(self.stop.take());
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}
