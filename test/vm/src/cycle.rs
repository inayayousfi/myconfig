//! The test cycle of one guest system: a fresh install, a second install that must change
//! nothing, and a removal that must bring back the state from before the install. On Windows
//! the cycle also checks the Arch WSL distribution that the Windows installer creates.
//!
//! The cycle runs on a throwaway layer over the clean image. A failed cycle keeps that layer,
//! with its state lists, comparisons, command logs and last screen, as the failed disk of its
//! system until the next cycle of that system.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::answers::Answers;
use crate::binary::{Binary, Launch};
use crate::collect::{self, CollectError};
use crate::compare::{self, Allowed, AllowedError, Comparison, Difference};
use crate::disk::{self, DiskError};
use crate::drive::{Desktop, DriveError, Driver, Terminal};
use crate::os::Os;
use crate::release;
use crate::snapshot::Snapshot;
use crate::ssh::Guest;
use crate::start::{self, Disk, Options, ReadyError, StartError};
use crate::updates;
use crate::vm::Vm;

const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(3 * 60);
/// The differences printed in the terminal. The report file holds all of them.
const SHOWN_DIFFERENCES: usize = 40;

/// A failure of `cycle`. Failures before the guest starts leave no failed disk; `Failed` and
/// `NotKept` name where the failed layer is.
#[derive(Debug, thiserror::Error)]
pub enum CycleError {
    #[error(
        "no installer cycle runs on {0}; its image builds the USB stick, and the InayOS installer is tested with: cargo xtask vm usb build, then cargo xtask vm scenario <name>"
    )]
    NoCycle(Os),
    #[error("allowed-changes/{name}.txt is not valid: {error}")]
    Allowed {
        name: &'static str,
        error: AllowedError,
    },
    #[error(transparent)]
    Start(#[from] StartError),
    #[error(transparent)]
    Disk(#[from] DiskError),
    #[error(
        "the {os} cycle failed: {error}\nthe disk, state lists, comparisons, logs and last screen are kept in {kept}"
    )]
    Failed {
        os: Os,
        error: Box<StepError>,
        kept: PathBuf,
    },
    #[error(
        "the {os} cycle failed: {error}\nkeeping the failed disk failed too, so it is still in {work}: {keep}"
    )]
    NotKept {
        os: Os,
        error: Box<StepError>,
        work: PathBuf,
        keep: io::Error,
    },
}

/// A failure of one step of a running cycle.
#[derive(Debug, thiserror::Error)]
pub enum StepError {
    #[error(transparent)]
    Ready(#[from] ReadyError),
    #[error(transparent)]
    Collect(#[from] CollectError),
    #[error("{step}: {error}")]
    Drive {
        step: String,
        #[source]
        error: DriveError,
    },
    #[error(
        "{count} differences after {after} that the allowed-changes file does not allow; see {report}"
    )]
    Changed {
        after: &'static str,
        count: usize,
        report: PathBuf,
    },
    #[error("could not write {0}: {1}")]
    Io(PathBuf, io::Error),
}

/// Runs the cycle of `os` on a new throwaway layer over its clean image. With `watch`, a viewer
/// window shows the guest display.
pub fn cycle(os: Os, watch: bool, repository: &Path) -> Result<(), CycleError> {
    let parse =
        |name, text| Allowed::parse(text).map_err(|error| CycleError::Allowed { name, error });
    let Some(allowed) = compare::allowed_changes(os) else {
        return Err(CycleError::NoCycle(os));
    };
    let allowed = parse(os.name(), allowed)?;
    let arch_wsl_allowed = match os {
        Os::Windows => Some(parse("arch-wsl", compare::ARCH_WSL_ALLOWED_CHANGES)?),
        Os::Ubuntu | Os::Arch => None,
    };
    let mut started = start::start(
        Options {
            os,
            disk: Disk::Throwaway,
            displays: 1,
            watch,
        },
        repository,
    )?;
    println!("vm: {}", release::check(os, &started.paths));
    let failed = started.paths.failed(os);
    disk::discard(&failed)?;

    let work = started.layer.clone();
    let folder = work.dir().to_owned();
    let result = Steps {
        os,
        guest: &started.guest,
        binaries: &started.binaries,
        allowed: &allowed,
        arch_wsl_allowed: arch_wsl_allowed.as_ref(),
        folder: &folder,
    }
    .run(&mut started.vm);
    let vm = started.vm;

    match result {
        Ok(()) => {
            vm.shut_down(SHUTDOWN_TIMEOUT);
            disk::discard(&work)?;
            println!("vm: the {os} cycle passed");
            Ok(())
        }
        Err(error) => {
            let screen = folder.join("screen.png");
            if let Err(screen_error) = vm.qmp().and_then(|mut qmp| qmp.screendump(&screen)) {
                eprintln!("vm: could not save the last screen: {screen_error}");
            }
            vm.shut_down(SHUTDOWN_TIMEOUT);
            match fs::rename(work.dir(), failed.dir()) {
                Ok(()) => Err(CycleError::Failed {
                    os,
                    error: Box::new(error),
                    kept: failed.dir().to_owned(),
                }),
                Err(keep) => Err(CycleError::NotKept {
                    os,
                    error: Box::new(error),
                    work: folder,
                    keep,
                }),
            }
        }
    }
}

/// The steps of a cycle inside the running guest. Every file they write goes into `folder`.
struct Steps<'a> {
    os: Os,
    guest: &'a Guest,
    binaries: &'a [Binary],
    allowed: &'a Allowed,
    /// The allowed changes of the Arch WSL distribution, for a Windows guest.
    arch_wsl_allowed: Option<&'a Allowed>,
    folder: &'a Path,
}

impl Steps<'_> {
    fn run(&self, vm: &mut Vm) -> Result<(), StepError> {
        start::install_binaries(self.os, vm, self.guest, self.binaries)?;
        match updates::pending(self.os, self.guest) {
            Ok(Some(count)) => println!("vm: the guest has {count} pending package updates"),
            Ok(None) => {}
            Err(error) => {
                println!("vm: warning: could not count the pending package updates: {error}")
            }
        }

        let answers = Answers::new();
        let mut driver: Box<dyn Driver> = match self.os {
            Os::Ubuntu | Os::Arch => Box::new(Terminal::new(self.guest, &answers)),
            Os::Windows => Box::new(Desktop::new(
                self.guest,
                &answers,
                self.folder.join("scratch"),
            )),
        };
        let driver = driver.as_mut();

        // The installer that the guest runs itself. The Arch WSL installer runs inside the
        // distribution that the arch-wsl module of the Windows installer creates, and that
        // module installs it.
        let installer = self
            .binaries
            .iter()
            .find(|binary| binary.launch != Launch::Wsl)
            .expect("every guest has an installer of its own");
        let arch_wsl = self
            .binaries
            .iter()
            .find(|binary| binary.launch == Launch::Wsl)
            .zip(self.arch_wsl_allowed);
        if arch_wsl.is_some() {
            collect::prepare_arch_wsl(self.guest, &self.folder.join("scratch"))?;
            collect::forget_arch_wsl_created(self.guest)?;
        }

        let before = self.lists("before")?;
        self.drive(driver, vm, installer, "install", "first install")?;
        self.drive(driver, vm, installer, "verify", "verify")?;
        let first = self.lists("first-install")?;
        // Only the first install creates the distribution, and the hook saved its lists
        // before the Arch WSL installer ran in it. A later install saves them again over
        // the installed distribution, so they are read now.
        let arch_wsl_first = match arch_wsl {
            Some(_) => {
                let first = self.arch_wsl_lists("arch-wsl-first-install")?;
                println!("vm: reading the arch-wsl-created state lists");
                let created = collect::arch_wsl_created_lists(
                    self.guest,
                    &self.folder.join("lists-arch-wsl-created.txt"),
                )?;
                Some((first, created))
            }
            None => None,
        };
        self.drive(driver, vm, installer, "install", "second install")?;
        let second = self.lists("second-install")?;
        self.check(
            &first,
            &second,
            self.allowed,
            Comparison::Install,
            "the second install",
            "second-install-changes",
        )?;

        if let (Some((arch_wsl, allowed)), Some((first, created))) = (arch_wsl, arch_wsl_first) {
            let second = self.arch_wsl_lists("arch-wsl-second-install")?;
            self.check(
                &first,
                &second,
                allowed,
                Comparison::Install,
                "the second install in Arch WSL",
                "arch-wsl-second-install-changes",
            )?;
            // The Windows remove unregisters the distribution, so the Arch WSL remove goes first.
            self.remove(driver, vm, arch_wsl)?;
            let after = self.arch_wsl_lists("arch-wsl-after")?;
            self.check(
                &created,
                &after,
                allowed,
                Comparison::Removal,
                "the Arch WSL removal",
                "arch-wsl-removal-leftovers",
            )?;
        }

        self.remove(driver, vm, installer)?;
        let after = self.lists("after")?;
        self.check(
            &before,
            &after,
            self.allowed,
            Comparison::Removal,
            "removal",
            "removal-leftovers",
        )
    }

    /// Removes every module of `binary` with one `remove` command, which orders the removal.
    fn remove(
        &self,
        driver: &mut dyn Driver,
        vm: &mut Vm,
        binary: &Binary,
    ) -> Result<(), StepError> {
        let list = self.drive(driver, vm, binary, "list", "list")?;
        let modules: Vec<&str> = list
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .collect();
        self.drive(
            driver,
            vm,
            binary,
            &format!("remove {}", modules.join(" ")),
            "remove",
        )?;
        Ok(())
    }

    /// Runs one installer command and saves its log under a name made of `step` and the package.
    fn drive(
        &self,
        driver: &mut dyn Driver,
        vm: &mut Vm,
        binary: &Binary,
        arguments: &str,
        step: &str,
    ) -> Result<String, StepError> {
        let label = format!("{} {step}", binary.package);
        println!("vm: {label}");
        let log = self
            .folder
            .join("logs")
            .join(format!("{}.log", label.replace(' ', "-")));
        let folder = self.folder.join("logs");
        fs::create_dir_all(&folder).map_err(|error| StepError::Io(folder, error))?;
        driver
            .run(vm, &binary.command(arguments), &log)
            .map_err(|error| StepError::Drive { step: label, error })
    }

    fn arch_wsl_lists(&self, name: &str) -> Result<Snapshot, StepError> {
        println!("vm: saving the {name} state lists");
        let file = self.folder.join(format!("lists-{name}.txt"));
        Ok(collect::collect_arch_wsl(self.guest, &file)?)
    }

    fn lists(&self, name: &str) -> Result<Snapshot, StepError> {
        println!("vm: saving the {name} state lists");
        let file = self.folder.join(format!("lists-{name}.txt"));
        Ok(collect::collect(
            self.os,
            self.guest,
            &self.folder.join("scratch"),
            &file,
        )?)
    }

    /// Compares two state lists, writes the differences that `allowed` does not allow to
    /// `report`, shows them, and fails when there is any.
    fn check(
        &self,
        before: &Snapshot,
        after: &Snapshot,
        allowed: &Allowed,
        comparison: Comparison,
        what: &'static str,
        report: &str,
    ) -> Result<(), StepError> {
        let differences = compare::compare(before, after, allowed, comparison);
        let report = self.folder.join(format!("{report}.txt"));
        let text: String = differences
            .iter()
            .map(|difference| format!("{difference}\n"))
            .collect();
        fs::write(&report, &text).map_err(|error| StepError::Io(report.clone(), error))?;
        if differences.is_empty() {
            println!("vm: no difference after {what}");
            return Ok(());
        }
        show(&differences);
        Err(StepError::Changed {
            after: what,
            count: differences.len(),
            report,
        })
    }
}

fn show(differences: &[Difference]) {
    for difference in differences.iter().take(SHOWN_DIFFERENCES) {
        println!("  {difference}");
    }
    if differences.len() > SHOWN_DIFFERENCES {
        println!("  and {} more", differences.len() - SHOWN_DIFFERENCES);
    }
}
