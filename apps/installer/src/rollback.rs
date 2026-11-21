//! `inayos-installer rollback`: finds the InayOS system, opens it, lists its snapshots, and
//! puts it back to the chosen one. The options answer every question, so a script can run
//! it without a terminal.
use std::{
    io::{self, BufRead, IsTerminal, Write},
    path::PathBuf,
    process::{Command, ExitCode},
};

use clap::Args;
use inayos_install_core::{
    Host, RealHost,
    rollback::{self, InstalledSystem, Opened, RollbackError, Unlock},
};

use crate::console::Console;

#[derive(Args)]
pub struct Options {
    /// The disk of the InayOS system, such as /dev/nvme0n1. Needed when several disks hold
    /// one.
    #[arg(long)]
    disk: Option<String>,
    /// The number of the snapshot to go back to, as the list shows it.
    #[arg(long)]
    snapshot: Option<u32>,
    /// A file that holds the recovery key of an encrypted system. Without it, the TPM opens
    /// the system, and when it refuses, the command asks for the key.
    #[arg(long)]
    recovery_key_file: Option<PathBuf>,
    /// Rolls back without asking for a confirmation.
    #[arg(long)]
    yes: bool,
    /// The log of the rollback.
    #[arg(long, default_value = "/var/log/inayos-rollback.log")]
    log: PathBuf,
}

pub fn run(options: Options) -> ExitCode {
    let console = match Console::open(&options.log) {
        Ok(console) => console,
        Err(error) => {
            eprintln!(
                "error: could not open the log {}: {error}",
                options.log.display()
            );
            return ExitCode::FAILURE;
        }
    };
    match roll_back(&options, &console) {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            console.error(&message);
            ExitCode::FAILURE
        }
    }
}

fn roll_back(options: &Options, console: &Console) -> Result<(), String> {
    let host = RealHost;
    if host.effective_uid().map_err(|error| error.to_string())? != 0 {
        return Err("the rollback must run as root: sudo inayos-installer rollback".to_owned());
    }
    let system = choose_system(
        rollback::find(&host, console).map_err(|error| error.to_string())?,
        options.disk.as_deref(),
    )?;
    let key = match &options.recovery_key_file {
        Some(file) => Some(
            std::fs::read_to_string(file)
                .map_err(|error| format!("could not read {}: {error}", file.display()))?,
        ),
        None => None,
    };
    let opened = open(&host, console, &system, key.as_deref())?;
    let result = choose_and_roll_back(&opened, options);
    for error in opened.close() {
        console.error(&format!("cleanup failed: {error}"));
    }
    let kept = result?;
    println!(
        "The system is back to the snapshot. The system from before stays on the disk as the \
         subvolume {kept}. Restart the computer without the USB stick."
    );
    Ok(())
}

/// The system on `disk`, or the only one found.
fn choose_system(
    found: Vec<InstalledSystem>,
    disk: Option<&str>,
) -> Result<InstalledSystem, String> {
    match disk {
        Some(disk) => found
            .into_iter()
            .find(|system| system.disk == disk)
            .ok_or_else(|| format!("{disk} holds no InayOS system")),
        None => {
            let mut found = found;
            match found.len() {
                0 => Err("no disk holds an InayOS system".to_owned()),
                1 => Ok(found.remove(0)),
                _ => Err(format!(
                    "several disks hold an InayOS system: choose one with --disk ({})",
                    found
                        .iter()
                        .map(|system| system.disk.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                )),
            }
        }
    }
}

/// Opens the system with the recovery key when there is one, else with the TPM, and asks
/// for the key on the terminal when the TPM refuses.
fn open<'a>(
    host: &'a RealHost,
    console: &'a Console,
    system: &InstalledSystem,
    key: Option<&str>,
) -> Result<Opened<'a>, String> {
    let unlock = key.map_or(Unlock::Tpm, Unlock::RecoveryKey);
    match rollback::open(host, console, system, unlock) {
        Ok(opened) => Ok(opened),
        Err(RollbackError::TpmRefused { partition }) if key.is_none() => {
            if !io::stdin().is_terminal() {
                return Err(format!(
                    "the TPM did not open {partition}: pass the recovery key with --recovery-key-file"
                ));
            }
            let key = ask_recovery_key()?;
            rollback::open(host, console, system, Unlock::RecoveryKey(&key))
                .map_err(|error| error.to_string())
        }
        Err(error) => Err(error.to_string()),
    }
}

/// The recovery key, typed without showing on the screen.
fn ask_recovery_key() -> Result<String, String> {
    let output = Command::new("systemd-ask-password")
        .args(["--timeout=0", "Recovery key of the InayOS disk:"])
        .output()
        .map_err(|error| format!("could not ask for the recovery key: {error}"))?;
    if !output.status.success() {
        return Err("no recovery key was given".to_owned());
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

fn choose_and_roll_back(opened: &Opened, options: &Options) -> Result<String, String> {
    let snapshots = opened.snapshots().map_err(|error| error.to_string())?;
    if snapshots.is_empty() {
        return Err("the system has no snapshot".to_owned());
    }
    let number = match options.snapshot {
        Some(number) => number,
        None => {
            println!("Snapshots of the system, oldest first (dates in UTC):");
            for snapshot in &snapshots {
                println!(
                    "  {:>5}  {:<6}  {}  {}",
                    snapshot.number, snapshot.kind, snapshot.date, snapshot.description
                );
            }
            println!(
                "A pre snapshot holds the system from before that pacman command; a post \
                 snapshot, from after it."
            );
            let answer = ask("Number of the snapshot to go back to: ")?;
            answer
                .trim()
                .parse()
                .map_err(|_| format!("{:?} is not a snapshot number", answer.trim()))?
        }
    };
    if !options.yes {
        let answer = ask(&format!(
            "Replace the system with snapshot {number}? Its packages and system files come \
             back; /home stays as it is. Type yes to go on: "
        ))?;
        if answer.trim() != "yes" {
            return Err("the rollback was cancelled; the disk is unchanged".to_owned());
        }
    }
    opened
        .roll_back(number)
        .map(|done| done.kept)
        .map_err(|error| error.to_string())
}

fn ask(question: &str) -> Result<String, String> {
    if !io::stdin().is_terminal() {
        return Err(format!(
            "no terminal answers \"{}\": pass --snapshot and --yes",
            question.trim()
        ));
    }
    print!("{question}");
    let _ = io::stdout().flush();
    let mut answer = String::new();
    io::stdin()
        .lock()
        .read_line(&mut answer)
        .map_err(|error| error.to_string())?;
    Ok(answer)
}
