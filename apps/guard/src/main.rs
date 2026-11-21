//! `inayos-guard`, the InayOS boot guard, run as root by its pacman hook and its boot
//! service. `crates/guard` holds its logic.
#[cfg(unix)]
mod run {
    use std::process::ExitCode;

    use inayos_guard::{
        RebindOutcome, UpdateOutcome, check_boot, check_update, prepare_firmware_update, rebind,
        status,
        status::describe,
        system::{self, RealSystem},
    };

    const USAGE: &str = "usage: inayos-guard <command>

Commands:
  check-update  check the boot files rEFInd starts and write its Previous system entry
  check-boot    check the system that just started, and bind the TPM to PCR 7 again
                after a temporary TPM slot
  prepare-firmware-update
                add a temporary TPM slot to each encrypted volume before a firmware
                or Secure Boot database update
  rebind        ask for the recovery key once and bind the TPM to PCR 7 again on each
                encrypted volume that the TPM no longer opens
  status        print the result of the last checks";

    pub fn main() -> ExitCode {
        let args: Vec<String> = std::env::args().skip(1).collect();
        let command = match args.as_slice() {
            [command] => command.as_str(),
            _ => {
                eprintln!("{USAGE}");
                return ExitCode::from(2);
            }
        };
        let system = RealSystem;
        // The commands that change the TPM slots, the record or rEFInd's entries run one
        // at a time, so none acts on slots or a record another one is changing.
        let _lock = match command {
            "check-update" | "check-boot" | "prepare-firmware-update" | "rebind" => {
                match system::lock() {
                    Ok(file) => Some(file),
                    Err(error) => {
                        eprintln!(
                            "inayos-guard: could not lock {}: {error}",
                            system::LOCK_FILE
                        );
                        return ExitCode::from(3);
                    }
                }
            }
            _ => None,
        };
        match command {
            // A failed check exits with success: pacman runs this after the transaction,
            // and the failures go to stderr, which pacman shows.
            "check-update" => match check_update(&system) {
                Ok(UpdateOutcome::FromSnapshot) => {
                    eprintln!(
                        "inayos-guard: this system started from a snapshot, so the boot files are not checked and rEFInd's default stays as it is"
                    );
                    ExitCode::SUCCESS
                }
                Ok(UpdateOutcome::Checked(report)) => {
                    for note in &report.notes {
                        eprintln!("inayos-guard: {note}");
                    }
                    for failure in &report.failures {
                        eprintln!("inayos-guard: {failure}");
                    }
                    if !report.failures.is_empty() {
                        eprintln!(
                            "inayos-guard: rEFInd now starts {} by default",
                            report.default_selection
                        );
                    }
                    ExitCode::SUCCESS
                }
                Err(error) => fail(&error),
            },
            "check-boot" => match check_boot(&system) {
                Ok(result) => {
                    for change in &result.changes {
                        eprintln!("inayos-guard: {change}");
                    }
                    for failure in &result.failures {
                        eprintln!("inayos-guard: {failure}");
                    }
                    if result.passed {
                        ExitCode::SUCCESS
                    } else {
                        ExitCode::FAILURE
                    }
                }
                Err(error) => fail(&error),
            },
            "prepare-firmware-update" => match prepare_firmware_update(&system) {
                Ok(None) => {
                    eprintln!(
                        "inayos-guard: this system is not encrypted, so a firmware update needs no temporary TPM slot"
                    );
                    ExitCode::SUCCESS
                }
                Ok(Some(result)) => {
                    for line in result.changes.iter().chain(&result.failures) {
                        eprintln!("inayos-guard: {line}");
                    }
                    if result.passed {
                        ExitCode::SUCCESS
                    } else {
                        ExitCode::FAILURE
                    }
                }
                Err(error) => fail(&error),
            },
            "rebind" => match rebind(&system) {
                Ok(RebindOutcome::NotEncrypted) => {
                    eprintln!("inayos-guard: this system is not encrypted");
                    ExitCode::SUCCESS
                }
                Ok(RebindOutcome::AlreadyBound) => {
                    eprintln!(
                        "inayos-guard: the TPM opens every encrypted volume on this boot, so nothing needs binding"
                    );
                    ExitCode::SUCCESS
                }
                Ok(RebindOutcome::Refused(reason)) => {
                    eprintln!("inayos-guard: nothing was changed: {reason}");
                    ExitCode::FAILURE
                }
                Ok(RebindOutcome::Rebound(result)) => {
                    for line in result.changes.iter().chain(&result.failures) {
                        eprintln!("inayos-guard: {line}");
                    }
                    if result.passed {
                        ExitCode::SUCCESS
                    } else {
                        ExitCode::FAILURE
                    }
                }
                Err(error) => fail(&error),
            },
            "status" => match status(&system) {
                Ok(Some(status)) => {
                    print!("{}", describe(&status));
                    if status.passed {
                        ExitCode::SUCCESS
                    } else {
                        ExitCode::FAILURE
                    }
                }
                Ok(None) => {
                    println!("No check has run yet.");
                    ExitCode::FAILURE
                }
                Err(error) => fail(&error),
            },
            "-h" | "--help" | "help" => {
                println!("{USAGE}");
                ExitCode::SUCCESS
            }
            other => {
                eprintln!("inayos-guard: unknown command {other}\n\n{USAGE}");
                ExitCode::from(2)
            }
        }
    }

    fn fail(error: &inayos_guard::Error) -> ExitCode {
        eprintln!("inayos-guard: {error}");
        ExitCode::from(3)
    }
}

#[cfg(unix)]
fn main() -> std::process::ExitCode {
    run::main()
}

/// The guard runs on Linux only. Other targets build it so the workspace check covers
/// them.
#[cfg(not(unix))]
fn main() -> std::process::ExitCode {
    eprintln!("inayos-guard runs only on Linux");
    std::process::ExitCode::FAILURE
}
