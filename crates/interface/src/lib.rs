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
use inayos_modules::{
    Context, Interaction, Module, ModuleResult, PackageSystem, leftovers, runner, state::StateStore,
};
use xshell::Shell;

/// Exit code when a question had no answer, so a remove kept something or an install
/// skipped a module, and nothing failed.
pub const UNANSWERED: u8 = 3;

/// The text that ends every question of the plain command line, after which it reads the
/// answer.
pub const QUESTION_END: &str = " [y/N] ";

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
    /// Removes the named modules. `myconfig-leftovers` names what myconfig, the earlier
    /// name of InayOS, installed under its own names.
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
    #[command(subcommand, name = "shared-desktop")]
    SharedDesktop(SharedDesktopStep),
}

#[derive(Subcommand)]
enum SharedDesktopStep {
    /// Moves the shared desktop items, writing each move to RECORD. Replaces an item
    /// already on DESKTOP only when the JSON list in REPLACE names its shared item.
    Move {
        record: PathBuf,
        desktop: PathBuf,
        replace: PathBuf,
    },
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
        Some(Command::Internal(step)) => run_internal(step).map(|()| false),
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
    let state = RefCell::new(StateStore::open_user(&home)?);
    let ctx = Context::new(package_system, &shell, Path::new(&home), io, &state);
    action(&ctx)
}

fn run_internal(step: Internal) -> ModuleResult {
    use inayos_modules::internal;
    match step {
        Internal::SharedDesktop(SharedDesktopStep::Move {
            record,
            desktop,
            replace,
        }) => internal::elevated_move(&record, &desktop, &replace),
        Internal::SharedDesktop(SharedDesktopStep::MoveBack { change }) => {
            internal::elevated_move_back(&change)
        }
    }
}

/// The modules that `remove` names, in profile order, and whether it names the leftovers
/// group of the earlier name.
fn select_removal<'a>(
    profile: &Profile<'a>,
    names: &[String],
) -> ModuleResult<(Vec<&'a dyn Module>, bool)> {
    let leftovers = names.iter().any(|name| name == leftovers::GROUP);
    let modules: Vec<String> = names
        .iter()
        .filter(|name| *name != leftovers::GROUP)
        .cloned()
        .collect();
    if leftovers && modules.is_empty() {
        return Ok((Vec::new(), true));
    }
    Ok((select(profile, &modules)?, leftovers))
}

/// Runs one command line action and returns whether a question went unanswered.
fn run_plain(profile: &Profile<'_>, command: Command) -> ModuleResult<bool> {
    // Unknown module names are reported before sudo asks for a password.
    let (modules, leftovers) = match &command {
        Command::Install { modules } | Command::Verify { modules } => {
            (select(profile, modules)?, false)
        }
        Command::Remove { modules, .. } => select_removal(profile, modules)?,
        Command::List | Command::Internal(_) => return Ok(false),
    };
    let io = plain::Plain;
    let _session = sudo::Session::start_interactive(profile.package_system)?;
    with_context(profile.package_system, &io, |ctx| match command {
        Command::Install { .. } => match runner::install(ctx, profile.modules, &modules) {
            Ok(()) => {
                println!("{} profile completed successfully", profile.title);
                Ok(false)
            }
            Err(error) if matches!(&error, runner::InstallError::Incomplete(report) if report.only_unanswered()) =>
            {
                eprintln!("{error}");
                Ok(true)
            }
            Err(error) => Err(error.into()),
        },
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
            let report = if leftovers {
                runner::remove_with_leftovers(ctx, profile.modules, &modules, force)?
            } else {
                runner::remove(ctx, profile.modules, &modules, force)?
            };
            Ok(report.unanswered)
        }
        Command::List | Command::Internal(_) => Ok(false),
    })
}
