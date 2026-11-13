//! The test cycle of one guest system: a fresh install, a second install that must change
//! nothing, and a removal that must bring back the state from before the install.
//!
//! The cycle runs on a throwaway layer over the clean image. A failed cycle keeps that layer,
//! with its state lists, comparisons, command logs and last screen, as the failed disk of its
//! system until the next cycle of that system.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::answers::Answers;
use crate::binary::Binary;
use crate::collect::{self, CollectError};
use crate::compare::{self, Allowed, AllowedError, Difference};
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
    #[error("the allowed-changes file of {os} is not valid: {error}")]
    Allowed { os: Os, error: AllowedError },
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
    let allowed = Allowed::parse(compare::allowed_changes(os))
        .map_err(|error| CycleError::Allowed { os, error })?;
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
            Os::Cachyos | Os::Ubuntu => Box::new(Terminal::new(self.guest, &answers)),
            Os::Windows => Box::new(Desktop::new(
                self.guest,
                &answers,
                self.folder.join("scratch"),
            )),
        };
        let driver = driver.as_mut();

        let before = self.lists("before")?;
        for binary in self.binaries {
            self.drive(driver, vm, binary, "install", "first install")?;
        }
        for binary in self.binaries {
            self.drive(driver, vm, binary, "verify", "verify")?;
        }
        let first = self.lists("first-install")?;
        for binary in self.binaries {
            self.drive(driver, vm, binary, "install", "second install")?;
        }
        let second = self.lists("second-install")?;
        self.check(
            &first,
            &second,
            "the second install",
            "second-install-changes",
        )?;

        // The later installer depends on what the earlier one set up, so it goes first.
        for binary in self.binaries.iter().rev() {
            let list = self.drive(driver, vm, binary, "list", "list")?;
            let modules: Vec<&str> = list
                .lines()
                .map(str::trim)
                .filter(|line| !line.is_empty())
                .collect();
            for module in modules.iter().rev() {
                self.drive(
                    driver,
                    vm,
                    binary,
                    &format!("remove {module}"),
                    &format!("remove {module}"),
                )?;
            }
        }
        let after = self.lists("after")?;
        self.check(&before, &after, "removal", "removal-leftovers")
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

    /// Compares two state lists, writes the differences that are not allowed to `report`,
    /// shows them, and fails when there is any.
    fn check(
        &self,
        before: &Snapshot,
        after: &Snapshot,
        what: &'static str,
        report: &str,
    ) -> Result<(), StepError> {
        let differences = compare::compare(before, after, self.allowed);
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
