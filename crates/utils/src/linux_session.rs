use std::{error::Error, sync::mpsc, thread, time::Duration};

use xshell::{Shell, cmd};

use crate::{PackageSystem, find_program};

pub struct LinuxSession {
    stop: mpsc::Sender<()>,
    worker: Option<thread::JoinHandle<()>>,
}

impl LinuxSession {
    pub fn prepare(sh: &Shell, system: PackageSystem) -> Result<Self, Box<dyn Error>> {
        if system == PackageSystem::Winget {
            return Err("a Linux package session cannot use Winget".into());
        }
        find_program(sh, "sudo")?;
        match system {
            PackageSystem::Arch => {
                find_program(sh, "pacman")?;
            }
            PackageSystem::Apt => {
                find_program(sh, "apt-get")?;
            }
            PackageSystem::Winget => unreachable!(),
        }
        cmd!(sh, "sudo -v").run()?;
        let (stop, receiver) = mpsc::channel();
        let worker = thread::spawn(move || {
            let Ok(sh) = Shell::new() else { return };
            while receiver.recv_timeout(Duration::from_secs(60)).is_err() {
                if cmd!(sh, "sudo -n true").quiet().run().is_err() {
                    break;
                }
            }
        });
        let session = Self {
            stop,
            worker: Some(worker),
        };
        match system {
            PackageSystem::Arch => cmd!(sh, "sudo pacman -Syu --noconfirm").run()?,
            PackageSystem::Apt => cmd!(sh, "sudo apt-get update").run()?,
            PackageSystem::Winget => unreachable!(),
        }
        Ok(session)
    }
}

impl Drop for LinuxSession {
    fn drop(&mut self) {
        let _ = self.stop.send(());
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}
