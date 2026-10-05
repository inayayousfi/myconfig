//! The command line and the interactive screen shared by every installer app.
mod plain;
mod screen;
mod sudo;

use std::{
    cell::RefCell,
    path::{Path, PathBuf},
    process::ExitCode,
};

use clap::{Parser, Subcommand};
use myconfig_modules::{
    Context, Interaction, Module, ModuleResult, PackageSystem, runner,
    state::{StateStore, state_directory},
};
use xshell::Shell;

/// Exit code when a remove question had no answer and something was kept.
const UNANSWERED: u8 = 3;

#[derive(Parser)]
#[command(about = "Installs, verifies and removes this machine's configuration")]
struct Arguments {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Installs all modules, or only the named ones, then verifies each.
    Install { modules: Vec<String> },
    /// Verifies all modules, or only the named ones.
    Verify { modules: Vec<String> },
    /// Removes the named modules.
    Remove {
        #[arg(required = true)]
        modules: Vec<String>,
        /// Also removes packages and settings that other modules or packages still need.
        #[arg(long)]
        force: bool,
    },
    /// Prints the module names in profile order.
    List,
    /// Steps the installer starts by itself, such as an elevated child or a login service.
    #[command(hide = true, subcommand)]
    Internal(Internal),
}

#[derive(Subcommand)]
enum Internal {
    #[command(subcommand, name = "kde-plasma")]
    KdePlasma(KdePlasmaStep),
    #[command(subcommand, name = "shared-desktop")]
    SharedDesktop(SharedDesktopStep),
}

#[derive(Subcommand)]
enum KdePlasmaStep {
    /// Rebuilds and reloads Glass for the running KWin.
    RepairGlass,
}

#[derive(Subcommand)]
enum SharedDesktopStep {
    /// Moves the shared desktop items, writing each move to RECORD.
    Move { record: PathBuf, desktop: PathBuf },
    /// Undoes the move described in CHANGE.
    MoveBack { change: PathBuf },
}

/// What one installer app is: its title, its package manager and its profile.
pub struct Profile<'a> {
    pub title: &'a str,
    pub package_system: PackageSystem,
    pub modules: &'a [&'a dyn Module],
}

pub fn main(profile: Profile<'_>) -> ExitCode {
    let arguments = Arguments::parse();
    let result = match arguments.command {
        None => screen::run(&profile),
        Some(Command::List) => {
            for module in profile.modules {
                println!("{}", module.name());
            }
            Ok(false)
        }
        Some(Command::Internal(step)) => run_internal(&profile, step).map(|()| false),
        Some(command) => run_plain(&profile, command),
    };
    report(result)
}

fn report(result: ModuleResult<bool>) -> ExitCode {
    match result {
        Ok(false) => ExitCode::SUCCESS,
        Ok(true) => ExitCode::from(UNANSWERED),
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::FAILURE
        }
    }
}

/// The modules named on the command line, in profile order; all of them when none is named.
fn select<'a>(profile: &Profile<'a>, names: &[String]) -> ModuleResult<Vec<&'a dyn Module>> {
    for name in names {
        if !profile.modules.iter().any(|module| module.name() == name) {
            let known: Vec<_> = profile.modules.iter().map(|module| module.name()).collect();
            return Err(format!(
                "unknown module {name}; this profile has: {}",
                known.join(", ")
            )
            .into());
        }
    }
    Ok(profile
        .modules
        .iter()
        .filter(|module| names.is_empty() || names.iter().any(|name| name == module.name()))
        .copied()
        .collect())
}

fn home() -> ModuleResult<PathBuf> {
    let variable = if cfg!(windows) { "USERPROFILE" } else { "HOME" };
    Ok(PathBuf::from(
        std::env::var_os(variable).ok_or_else(|| format!("{variable} is unset"))?,
    ))
}

/// Points user services at the login session's runtime folder and message bus when the
/// run starts outside a desktop session, so `systemctl --user` reaches the user manager.
#[cfg(unix)]
fn set_session_bus(shell: &Shell) {
    use std::os::unix::fs::MetadataExt;
    let runtime = match shell
        .var_os("XDG_RUNTIME_DIR")
        .filter(|value| !value.is_empty())
    {
        Some(runtime) => PathBuf::from(runtime),
        None => match std::fs::metadata("/proc/self") {
            Ok(metadata) => PathBuf::from(format!("/run/user/{}", metadata.uid())),
            Err(_) => return,
        },
    };
    shell.set_var("XDG_RUNTIME_DIR", &runtime);
    if shell
        .var_os("DBUS_SESSION_BUS_ADDRESS")
        .is_none_or(|value| value.is_empty())
    {
        shell.set_var(
            "DBUS_SESSION_BUS_ADDRESS",
            format!("unix:path={}/bus", runtime.display()),
        );
    }
}

#[cfg(not(unix))]
fn set_session_bus(_shell: &Shell) {}

/// Opens the recorded state and builds the context for one action.
fn with_context<T>(
    package_system: PackageSystem,
    io: &dyn Interaction,
    action: impl FnOnce(&Context) -> ModuleResult<T>,
) -> ModuleResult<T> {
    let shell = Shell::new()?;
    set_session_bus(&shell);
    let home = home()?;
    let state = RefCell::new(StateStore::open(&state_directory(&home))?);
    let ctx = Context::new(package_system, &shell, Path::new(&home), io, &state);
    action(&ctx)
}

fn run_internal(profile: &Profile<'_>, step: Internal) -> ModuleResult {
    use myconfig_modules::internal;
    match step {
        // The login service has no terminal; Glass asks for permission through pkexec.
        Internal::KdePlasma(KdePlasmaStep::RepairGlass) => with_context(
            profile.package_system,
            &plain::Plain,
            internal::repair_glass,
        ),
        Internal::SharedDesktop(SharedDesktopStep::Move { record, desktop }) => {
            internal::elevated_move(&record, &desktop)
        }
        Internal::SharedDesktop(SharedDesktopStep::MoveBack { change }) => {
            internal::elevated_move_back(&change)
        }
    }
}

/// Runs one command line action and returns whether a question went unanswered.
fn run_plain(profile: &Profile<'_>, command: Command) -> ModuleResult<bool> {
    // Unknown module names are reported before sudo asks for a password.
    let modules = match &command {
        Command::Install { modules }
        | Command::Verify { modules }
        | Command::Remove { modules, .. } => select(profile, modules)?,
        Command::List | Command::Internal(_) => return Ok(false),
    };
    let io = plain::Plain;
    let _session = sudo::Session::start_interactive(profile.package_system)?;
    with_context(profile.package_system, &io, |ctx| match command {
        Command::Install { .. } => {
            runner::install(ctx, &modules)?;
            println!("{} profile completed successfully", profile.title);
            Ok(false)
        }
        Command::Verify { .. } => {
            let failures: Vec<_> = runner::verify(ctx, &modules)
                .into_iter()
                .filter_map(|(name, result)| result.err().map(|error| format!("{name}: {error}")))
                .collect();
            if failures.is_empty() {
                println!("Every module verified successfully");
                Ok(false)
            } else {
                Err(failures.join("\n").into())
            }
        }
        Command::Remove { force, .. } => {
            let report = runner::remove(ctx, profile.modules, &modules, force)?;
            Ok(report.unanswered)
        }
        Command::List | Command::Internal(_) => Ok(false),
    })
}
