//! What a module receives: commands, questions, and recorded changes.
use std::{
    cell::{Cell, RefCell},
    fs,
    io::{BufRead, BufReader, Read, Write},
    path::{Path, PathBuf},
    process::{Command, ExitStatus, Stdio},
    sync::mpsc,
};

use inayos_utils::{PackageSpec, PackageSystem, find_program, resolve_package};
use package_catalog::Package;
use serde::{Deserialize, Serialize};
use xshell::{Cmd, Shell, cmd};

use crate::{
    ModuleResult, Setting,
    package_manager::PackageManager,
    state::{Change, Previous, StateStore},
};

/// A yes or no question that nobody could answer, such as in a script with no terminal.
#[derive(Debug)]
pub struct Unanswered {
    pub question: String,
}

impl std::fmt::Display for Unanswered {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "no answer to: {}", self.question)
    }
}

impl std::error::Error for Unanswered {}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Step {
    Install,
    Verify,
    Remove,
}

#[derive(Debug)]
pub enum Event {
    Started {
        module: String,
        step: Step,
    },
    Finished {
        module: String,
        step: Step,
        error: Option<String>,
    },
    /// The runner did not install the module, because a module it needs is not in place
    /// or because nobody could answer its question.
    Skipped {
        module: String,
        reason: String,
    },
    /// The module verified because the person declined its install, so nothing of it is
    /// installed.
    Declined {
        module: String,
    },
}

/// How a run talks to the person: the screen or the plain command line.
pub trait Interaction {
    /// One line of command output.
    fn output(&self, line: &str);
    /// A message the person should read, such as a step that needs a new login.
    fn note(&self, message: &str);
    fn confirm(&self, question: &str) -> Result<bool, Unanswered>;
    /// Whether `with_terminal` has a terminal to give.
    fn has_terminal(&self) -> bool;
    /// Gives the whole terminal to a program, then takes it back.
    fn with_terminal(&self, command: &mut Command) -> std::io::Result<ExitStatus>;
    fn event(&self, event: Event);
}

/// Whether a service starts at boot or login, and whether it runs now.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ServiceState {
    pub enabled: bool,
    pub active: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ServiceScope {
    System,
    User,
}

/// Where a run's changes take effect. The interface chooses it; modules never test it,
/// and the `Context` methods that touch the running system answer for it.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum RunMode {
    /// On the running system: services start now and the desktop session sees changes.
    #[default]
    Live,
    /// Inside a never-booted system, run by the InayOS installer as the new user. Every
    /// command gets `SYSTEMD_OFFLINE=1`, services are only enabled, and steps that act on
    /// the running system wait until the installed system starts.
    Installer,
}

/// Whether a step that needs the running system happened now, or waits until the installed
/// system starts because the run is in installer mode.
#[must_use]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Applied {
    Now,
    Deferred,
}

enum Mode {
    /// Output goes to the person.
    Stream,
    /// Standard output is returned; standard error is kept for the error message.
    Capture,
    /// Output is discarded.
    Quiet,
}

struct Finished {
    status: ExitStatus,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
}

/// `command` with `SYSTEMD_OFFLINE=1`, which tells systemctl and Axidev that the system
/// has not booted. sudo resets the environment, so a `sudo` command also asks it to keep
/// the variable. sudo refuses that request unless the sudoers rule allows every command
/// or carries the `SETENV` tag.
fn offline(command: Command) -> Command {
    let mut command = if Path::new(command.get_program()).file_name() == Some("sudo".as_ref()) {
        let mut kept = Command::new(command.get_program());
        kept.arg("--preserve-env=SYSTEMD_OFFLINE")
            .args(command.get_args());
        for (key, value) in command.get_envs() {
            match value {
                Some(value) => kept.env(key, value),
                None => kept.env_remove(key),
            };
        }
        if let Some(directory) = command.get_current_dir() {
            kept.current_dir(directory);
        }
        kept
    } else {
        command
    };
    command.env("SYSTEMD_OFFLINE", "1");
    command
}

pub struct Context<'a> {
    pub package_system: PackageSystem,
    pub shell: &'a Shell,
    pub home: &'a Path,
    io: &'a dyn Interaction,
    state: &'a RefCell<StateStore>,
    module: Cell<&'static str>,
    run_mode: RunMode,
}

impl<'a> Context<'a> {
    pub fn new(
        package_system: PackageSystem,
        shell: &'a Shell,
        home: &'a Path,
        io: &'a dyn Interaction,
        state: &'a RefCell<StateStore>,
    ) -> Self {
        Self {
            package_system,
            shell,
            home,
            io,
            state,
            module: Cell::new(""),
            run_mode: RunMode::Live,
        }
    }

    /// The same context for a run in `mode`.
    #[must_use]
    pub fn with_run_mode(mut self, mode: RunMode) -> Self {
        self.run_mode = mode;
        self
    }

    fn installer_mode(&self) -> bool {
        self.run_mode == RunMode::Installer
    }

    /// Shows a step that installer mode leaves for the installed system's first start.
    fn deferred(&self, step: &str) {
        self.io.output(&format!(
            "Deferred until the installed system starts: {step}"
        ));
    }

    pub(crate) fn set_module(&self, module: &'static str) {
        self.module.set(module);
    }

    /// The changes of the current module that its last removal did not undo, oldest first.
    pub(crate) fn pending(&self) -> Vec<crate::state::Pending> {
        self.state.borrow().pending(self.module.get())
    }

    pub(crate) fn interaction(&self) -> &dyn Interaction {
        self.io
    }

    pub(crate) fn state(&self) -> &RefCell<StateStore> {
        self.state
    }

    // Commands.

    fn execute(&self, cmd: Cmd<'_>, input: Option<&[u8]>, mode: Mode) -> ModuleResult<Finished> {
        let display = cmd.to_string();
        self.execute_command(Command::from(cmd), &display, input, mode)
    }

    fn execute_command(
        &self,
        mut command: Command,
        display: &str,
        input: Option<&[u8]>,
        mode: Mode,
    ) -> ModuleResult<Finished> {
        if matches!(mode, Mode::Stream) {
            self.io.output(&format!("$ {display}"));
        }
        if self.installer_mode() {
            command = offline(command);
        }
        command.stdin(if input.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        });
        let piped = |quiet: bool| if quiet { Stdio::null() } else { Stdio::piped() };
        let quiet = matches!(mode, Mode::Quiet);
        command.stdout(piped(quiet)).stderr(piped(quiet));
        let mut child = command
            .spawn()
            .map_err(|error| format!("could not start `{display}`: {error}"))?;
        let (lines, received) = mpsc::channel::<String>();
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let stream = matches!(mode, Mode::Stream);
        std::thread::scope(|scope| -> ModuleResult {
            let writer = match (input, child.stdin.take()) {
                (Some(input), Some(mut pipe)) => Some(scope.spawn(move || {
                    pipe.write_all(input)?;
                    pipe.flush()
                })),
                _ => None,
            };
            let out_reader = child.stdout.take().map(|pipe| {
                let lines = lines.clone();
                scope.spawn(move || forward(pipe, stream.then_some(lines)))
            });
            let err_reader = child.stderr.take().map(|pipe| {
                let lines = lines.clone();
                scope.spawn(move || forward(pipe, stream.then_some(lines)))
            });
            drop(lines);
            for line in received {
                self.io.output(&line);
            }
            if let Some(reader) = out_reader {
                stdout = reader.join().map_err(|_| "output reader panicked")??;
            }
            if let Some(reader) = err_reader {
                stderr = reader.join().map_err(|_| "output reader panicked")??;
            }
            if let Some(writer) = writer {
                writer.join().map_err(|_| "input writer panicked")??;
            }
            Ok(())
        })?;
        let status = child.wait()?;
        Ok(Finished {
            status,
            stdout,
            stderr,
        })
    }

    fn checked(&self, cmd: Cmd<'_>, input: Option<&[u8]>, mode: Mode) -> ModuleResult<Vec<u8>> {
        let display = cmd.to_string();
        self.checked_command(Command::from(cmd), &display, input, mode)
    }

    fn checked_command(
        &self,
        command: Command,
        display: &str,
        input: Option<&[u8]>,
        mode: Mode,
    ) -> ModuleResult<Vec<u8>> {
        let finished = self.execute_command(command, display, input, mode)?;
        if !finished.status.success() {
            let detail = String::from_utf8_lossy(&finished.stderr);
            let detail = detail.trim();
            return Err(if detail.is_empty() {
                format!("`{display}` failed: {}", finished.status)
            } else {
                format!("`{display}` failed: {}: {detail}", finished.status)
            }
            .into());
        }
        Ok(finished.stdout)
    }

    /// Runs a command and shows its output.
    pub fn run(&self, cmd: Cmd<'_>) -> ModuleResult {
        self.checked(cmd, None, Mode::Stream).map(drop)
    }

    pub fn run_with_input(&self, cmd: Cmd<'_>, input: &[u8]) -> ModuleResult {
        self.checked(cmd, Some(input), Mode::Stream).map(drop)
    }

    /// Runs a command and returns its standard output without the final line break.
    pub fn read(&self, cmd: Cmd<'_>) -> ModuleResult<String> {
        Ok(text(self.checked(cmd, None, Mode::Capture)?))
    }

    pub fn read_with_input(&self, cmd: Cmd<'_>, input: &[u8]) -> ModuleResult<String> {
        Ok(text(self.checked(cmd, Some(input), Mode::Capture)?))
    }

    pub fn read_bytes(&self, cmd: Cmd<'_>) -> ModuleResult<Vec<u8>> {
        self.checked(cmd, None, Mode::Capture)
    }

    /// Runs a command and returns its standard output whether or not it succeeded.
    pub fn read_bytes_unchecked(&self, cmd: Cmd<'_>) -> ModuleResult<Vec<u8>> {
        Ok(self.execute(cmd, None, Mode::Capture)?.stdout)
    }

    /// Runs a command and returns whether it succeeded, with its standard output.
    pub fn read_unchecked(&self, cmd: Cmd<'_>) -> ModuleResult<(bool, String)> {
        let finished = self.execute(cmd, None, Mode::Capture)?;
        Ok((finished.status.success(), text(finished.stdout)))
    }

    /// Runs a command, shows its output, and returns its exit code instead of failing.
    pub fn run_status(&self, cmd: Cmd<'_>) -> ModuleResult<Option<i32>> {
        Ok(self.execute(cmd, None, Mode::Stream)?.status.code())
    }

    /// Runs a command without showing its output and returns whether it succeeded.
    pub fn succeeds(&self, cmd: Cmd<'_>) -> ModuleResult<bool> {
        Ok(self.execute(cmd, None, Mode::Quiet)?.status.success())
    }

    /// Runs a command that saves through the user's D-Bus session, such as `gsettings set`,
    /// which saves nothing and still succeeds without one. Installer mode has no session
    /// bus, so the command runs under its own `dbus-run-session`, with the user's
    /// configuration folder.
    pub fn run_with_session_bus(&self, cmd: Cmd<'_>) -> ModuleResult {
        let display = cmd.to_string();
        let command = Command::from(cmd);
        if !self.installer_mode() {
            return self
                .checked_command(command, &display, None, Mode::Stream)
                .map(drop);
        }
        let mut wrapped = Command::new("dbus-run-session");
        wrapped
            .arg("--")
            .arg(command.get_program())
            .args(command.get_args());
        for (key, value) in command.get_envs() {
            match value {
                Some(value) => wrapped.env(key, value),
                None => wrapped.env_remove(key),
            };
        }
        if let Some(directory) = command.get_current_dir() {
            wrapped.current_dir(directory);
        }
        wrapped.env("XDG_CONFIG_HOME", crate::support::config_home(self));
        self.checked_command(
            wrapped,
            &format!("dbus-run-session -- {display}"),
            None,
            Mode::Stream,
        )
        .map(drop)
    }

    /// Runs a program that talks to the terminal directly.
    pub fn with_terminal(&self, mut command: Command) -> ModuleResult {
        if self.installer_mode() {
            command = offline(command);
        }
        let display = format!("{command:?}");
        let status = self
            .io
            .with_terminal(&mut command)
            .map_err(|error| format!("could not start {display}: {error}"))?;
        if !status.success() {
            return Err(format!("{display} failed: {status}").into());
        }
        Ok(())
    }

    // Interaction.

    pub fn has_terminal(&self) -> bool {
        self.io.has_terminal()
    }

    /// Asks a question that the module needs answered. When nobody can answer, it returns
    /// `Unanswered`, and the runner skips the module if its install returns that error
    /// unchanged.
    pub fn confirm(&self, question: &str) -> Result<bool, Unanswered> {
        self.io.confirm(question)
    }

    /// Asks an optional question after the install itself. Nobody answering counts as a
    /// no, and the runner does not skip the module for it.
    pub fn confirm_optional(&self, question: &str) -> bool {
        self.io.confirm(question).unwrap_or(false)
    }

    pub fn note(&self, message: &str) {
        self.io.note(message);
    }

    // Recorded changes.

    pub(crate) fn record(&self, change: Change) -> ModuleResult {
        let mut state = self.state.borrow_mut();
        if state.recorded_in_run(self.module.get(), &change) {
            return Ok(());
        }
        state.record(self.module.get(), change)
    }

    pub(crate) fn specs(&self, packages: &[Package]) -> ModuleResult<Vec<(Package, PackageSpec)>> {
        packages
            .iter()
            .map(|package| {
                resolve_package(*package, self.package_system)
                    .map(|spec| (*package, spec))
                    .ok_or_else(|| {
                        format!(
                            "no package mapping for {package:?} on {:?}",
                            self.package_system
                        )
                        .into()
                    })
            })
            .collect()
    }

    /// How this run's package system names `package`.
    pub(crate) fn spec(&self, package: Package) -> ModuleResult<PackageSpec> {
        Ok(self.specs(&[package])?[0].1)
    }

    /// The package system this run installs with.
    pub(crate) fn packages(&self) -> &'static dyn PackageManager {
        crate::package_manager::for_system(self.package_system)
    }

    /// Installs the missing packages and records each one before installing it, so
    /// `remove` uninstalls only those.
    pub fn install_packages(&self, packages: &[Package]) -> ModuleResult {
        let manager = self.packages();
        let mut missing = Vec::new();
        let mut targets = Vec::new();
        for (package, spec) in self.specs(packages)? {
            let installed = manager.installed(self, spec)?;
            if !installed {
                self.record(Change::PackageInstalled(package))?;
                missing.push(spec);
            }
            if !installed || manager.moves_installed(spec) {
                targets.push(spec);
            }
        }
        if targets.is_empty() {
            return Ok(());
        }
        manager.install(self, &targets)?;
        for spec in missing {
            if !manager.installed(self, spec)? {
                manager.still_missing(self, spec)?;
            }
        }
        Ok(())
    }

    /// Uninstalls the installed packages and records each one before uninstalling it, so
    /// `remove` can reinstall them.
    pub fn uninstall_packages(&self, packages: &[Package]) -> ModuleResult {
        let manager = self.packages();
        let mut installed = Vec::new();
        for (package, spec) in self.specs(packages)? {
            if manager.installed(self, spec)? {
                self.record(Change::PackageUninstalled(package))?;
                installed.push(spec);
            }
        }
        if installed.is_empty() {
            return Ok(());
        }
        manager.uninstall(self, &installed)
    }

    /// Changes a setting after recording its current value, or records around the call
    /// each key that its KDE tool changes. A setting that only exists or not goes through
    /// `add` instead.
    pub fn set(&self, setting: Setting, value: &str) -> ModuleResult {
        if setting.presence() {
            return Err(format!("{} has no value to set; add it", setting.describe()).into());
        }
        self.change(setting, value)
    }

    /// Adds a setting that only exists or not, after recording that it was missing.
    pub fn add(&self, setting: Setting) -> ModuleResult {
        if !setting.presence() {
            return Err(format!("{} needs a value", setting.describe()).into());
        }
        self.change(setting, crate::setting::PRESENT)
    }

    fn change(&self, setting: Setting, value: &str) -> ModuleResult {
        let previous = setting.read(self)?;
        let unchanged = previous.as_deref() == Some(value);
        if unchanged && !setting.reapplies() {
            return Ok(());
        }
        let files = setting.kde_files();
        if !files.is_empty() {
            return self.write_recording_kde_keys(&setting, value, files);
        }
        if !unchanged {
            self.record(Change::Setting {
                setting: setting.clone(),
                previous,
            })?;
        }
        setting.write(self, value)
    }

    /// Writes a setting whose KDE tool changes more keys than the value, then records each
    /// key it changed in `files` with the value from before, also when the tool fails part
    /// way. This is the one recording that follows the change: which keys the tool changes is
    /// known only afterwards. `remove` then puts back only those keys, so keys that the
    /// person or another module writes in the same files later stay.
    fn write_recording_kde_keys(
        &self,
        setting: &Setting,
        value: &str,
        files: &[&str],
    ) -> ModuleResult {
        let before = files
            .iter()
            .map(|file| crate::setting::kde_entries(self, file))
            .collect::<ModuleResult<Vec<_>>>()?;
        let written = setting.write(self, value);
        // Every changed key is recorded even when one record fails, so remove can still put
        // back the others; the first failure is reported after the loop.
        let mut recorded = Ok(());
        for (file, before) in files.iter().zip(&before) {
            let after = crate::setting::kde_entries(self, file)?;
            for (setting, previous) in crate::setting::kde_changes(file, before, &after) {
                let result = self.record(Change::Setting { setting, previous });
                if recorded.is_ok() {
                    recorded = result;
                }
            }
        }
        written.and(recorded)
    }

    /// Deletes a setting after recording its current value.
    pub fn unset(&self, setting: Setting) -> ModuleResult {
        let Some(previous) = setting.read(self)? else {
            return Ok(());
        };
        self.record(Change::Setting {
            setting: setting.clone(),
            previous: Some(previous),
        })?;
        setting.delete(self)
    }

    /// Records a setting that a program is about to change by itself.
    pub fn record_setting(&self, setting: Setting) -> ModuleResult {
        let previous = setting.read(self)?;
        self.record(Change::Setting { setting, previous })
    }

    /// Records a setting that a program already changed, with the value it had before.
    pub fn record_setting_previous(
        &self,
        setting: Setting,
        previous: Option<String>,
    ) -> ModuleResult {
        self.record(Change::Setting { setting, previous })
    }

    /// Creates the missing parent directories of `path` and records each one.
    fn create_parents(&self, path: &Path) -> ModuleResult {
        let Some(parent) = path.parent() else {
            return Ok(());
        };
        let mut missing = Vec::new();
        let mut current = Some(parent);
        while let Some(directory) = current {
            if fs::symlink_metadata(directory).is_ok() {
                break;
            }
            missing.push(directory.to_path_buf());
            current = directory.parent();
        }
        for directory in missing.into_iter().rev() {
            fs::create_dir(&directory)?;
            self.record(Change::CreatedDirectory {
                path: directory,
                system: false,
            })?;
        }
        Ok(())
    }

    /// Writes a file owned by the user after recording what was there.
    pub fn write_file(&self, path: &Path, contents: &[u8], executable: bool) -> ModuleResult {
        let previous = Previous::capture(path)?;
        match &previous {
            Previous::File {
                contents: current,
                executable: current_executable,
            } if Previous::contents(current)? == contents && *current_executable == executable => {
                return Ok(());
            }
            Previous::Directory { .. } => {
                return Err(
                    format!("expected a file, found a directory: {}", path.display()).into(),
                );
            }
            _ => {}
        }
        self.create_parents(path)?;
        self.record(Change::File {
            path: path.to_path_buf(),
            system: false,
            previous: previous.clone(),
        })?;
        if matches!(previous, Previous::Link { .. }) {
            fs::remove_file(path)?;
        }
        fs::write(path, contents)?;
        crate::state::set_executable(path, executable)?;
        if fs::read(path)? != contents {
            return Err(format!("written file differs: {}", path.display()).into());
        }
        Ok(())
    }

    /// Points `path` at `target` after recording what was there.
    pub fn link(&self, path: &Path, target: &Path) -> ModuleResult {
        if fs::read_link(path).is_ok_and(|current| current == target) {
            return Ok(());
        }
        let previous = Previous::capture(path)?;
        self.create_parents(path)?;
        self.record(Change::File {
            path: path.to_path_buf(),
            system: false,
            previous: previous.clone(),
        })?;
        remove_user_path(path)?;
        crate::state::symlink(target, path)
    }

    /// Deletes a path owned by the user after recording it.
    pub fn delete(&self, path: &Path) -> ModuleResult {
        let previous = Previous::capture(path)?;
        if previous == Previous::Missing {
            return Ok(());
        }
        self.record(Change::File {
            path: path.to_path_buf(),
            system: false,
            previous,
        })?;
        remove_user_path(path)
    }

    /// Writes a root-owned file with `sudo install` after recording what was there.
    pub fn write_system_file(&self, path: &Path, contents: &[u8], mode: &str) -> ModuleResult {
        let sh = self.shell;
        let previous = if self.succeeds(cmd!(sh, "sudo test -e {path}"))? {
            if !self.succeeds(cmd!(sh, "sudo test -f {path}"))? {
                return Err(format!("expected a file: {}", path.display()).into());
            }
            let current = self.read_bytes(cmd!(sh, "sudo cat {path}"))?;
            if current == contents {
                return Ok(());
            }
            Previous::file(&current, false)
        } else {
            Previous::Missing
        };
        let mut missing = Vec::new();
        let mut current = path.parent();
        while let Some(directory) = current {
            if self.succeeds(cmd!(sh, "sudo test -d {directory}"))? {
                break;
            }
            missing.push(directory.to_path_buf());
            current = directory.parent();
        }
        for directory in missing.into_iter().rev() {
            self.run(cmd!(sh, "sudo install -d {directory}"))?;
            self.record(Change::CreatedDirectory {
                path: directory,
                system: true,
            })?;
        }
        self.record(Change::File {
            path: path.to_path_buf(),
            system: true,
            previous,
        })?;
        crate::support::install_system_file(self, path, contents, mode)
    }

    /// Records a root-owned file or link before a program changes it by itself, so
    /// `remove` puts it back.
    pub fn record_system_path(&self, path: &Path) -> ModuleResult {
        let sh = self.shell;
        let previous = if self.succeeds(cmd!(sh, "sudo test -L {path}"))? {
            Previous::Link {
                target: PathBuf::from(self.read(cmd!(sh, "sudo readlink {path}"))?),
            }
        } else if self.succeeds(cmd!(sh, "sudo test -f {path}"))? {
            Previous::file(&self.read_bytes(cmd!(sh, "sudo cat {path}"))?, false)
        } else if self.succeeds(cmd!(sh, "sudo test -e {path}"))? {
            return Err(format!("expected a file or a link: {}", path.display()).into());
        } else {
            Previous::Missing
        };
        self.record(Change::File {
            path: path.to_path_buf(),
            system: true,
            previous,
        })
    }

    /// Records a user file before a program changes it by itself.
    pub fn record_path(&self, path: &Path) -> ModuleResult {
        let mut missing = Vec::new();
        let mut current = path.parent();
        while let Some(directory) = current {
            if fs::symlink_metadata(directory).is_ok() {
                break;
            }
            missing.push(directory.to_path_buf());
            current = directory.parent();
        }
        for directory in missing.into_iter().rev() {
            self.record(Change::CreatedDirectory {
                path: directory,
                system: false,
            })?;
        }
        self.record(Change::File {
            path: path.to_path_buf(),
            system: false,
            previous: Previous::capture(path)?,
        })
    }

    /// Records a package built from this repository that was not installed before.
    pub(crate) fn local_package_installed(&self, name: &str) -> ModuleResult {
        self.record(Change::LocalPackage {
            name: name.to_owned(),
        })
    }

    /// Deletes a root-owned file after recording its contents.
    pub fn delete_system_file(&self, path: &Path) -> ModuleResult {
        let sh = self.shell;
        if !self.succeeds(cmd!(sh, "sudo test -e {path}"))? {
            return Ok(());
        }
        if !self.succeeds(cmd!(sh, "sudo test -f {path}"))? {
            return Err(format!("expected a file: {}", path.display()).into());
        }
        let current = self.read_bytes(cmd!(sh, "sudo cat {path}"))?;
        self.record(Change::File {
            path: path.to_path_buf(),
            system: true,
            previous: Previous::file(&current, false),
        })?;
        self.run(cmd!(sh, "sudo rm -f -- {path}"))
    }

    /// Records a folder that a program is about to create, which `remove` deletes only
    /// when it is empty, so files that others put in it later stay.
    pub fn created_directory(&self, path: &Path, system: bool) -> ModuleResult {
        self.record(Change::CreatedDirectory {
            path: path.to_path_buf(),
            system,
        })
    }

    /// Records a path that did not exist before this module created it.
    pub fn created(&self, path: &Path, system: bool) -> ModuleResult {
        self.record(Change::Created {
            path: path.to_path_buf(),
            system,
            ask: false,
        })
    }

    /// Whether this module recorded creating `path` in a run that was not removed yet.
    pub(crate) fn was_created(&self, path: &Path) -> bool {
        self.state
            .borrow()
            .pending(self.module.get())
            .iter()
            .any(|entry| matches!(&entry.change, Change::Created { path: created, .. } if created == path))
    }

    /// Records a created file that people or agents edit later, so `remove` asks first.
    pub fn created_user_data(&self, path: &Path) -> ModuleResult {
        self.record(Change::Created {
            path: path.to_path_buf(),
            system: false,
            ask: true,
        })
    }

    /// Enables a service so it starts at boot or login, without starting it.
    pub fn enable_service(&self, unit: &str, scope: ServiceScope) -> ModuleResult {
        if self.record_service(unit, scope)? {
            return Ok(());
        }
        let sh = self.shell;
        match scope {
            ServiceScope::System => self.run(cmd!(sh, "sudo systemctl enable {unit}")),
            ServiceScope::User => self.run(cmd!(sh, "systemctl --user enable {unit}")),
        }
    }

    /// Records the enabled state of a service that a program is about to enable by itself.
    /// Returns whether the service is already enabled, which leaves nothing to record.
    pub fn record_service(&self, unit: &str, scope: ServiceScope) -> ModuleResult<bool> {
        if self.service_enabled(unit, scope)? {
            return Ok(true);
        }
        self.record(Change::Service {
            unit: unit.to_owned(),
            scope,
            was_enabled: false,
        })?;
        Ok(false)
    }

    /// Whether a service starts at boot or login. `systemctl is-enabled` reads the unit
    /// links, so it answers in installer mode too.
    fn service_enabled(&self, unit: &str, scope: ServiceScope) -> ModuleResult<bool> {
        let user = scope_argument(scope);
        let (_, state) =
            self.read_unchecked(cmd!(self.shell, "systemctl {user...} is-enabled {unit}"))?;
        Ok(state == "enabled")
    }

    /// Whether a service runs now. Nothing runs in installer mode.
    pub fn service_active(&self, unit: &str, scope: ServiceScope) -> ModuleResult<bool> {
        if self.installer_mode() {
            return Ok(false);
        }
        let user = scope_argument(scope);
        self.succeeds(cmd!(
            self.shell,
            "systemctl {user...} --quiet is-active {unit}"
        ))
    }

    /// Whether a service is enabled, and whether it is active.
    pub fn service_state(&self, unit: &str, scope: ServiceScope) -> ModuleResult<ServiceState> {
        Ok(ServiceState {
            enabled: self.service_enabled(unit, scope)?,
            active: self.service_active(unit, scope)?,
        })
    }

    /// Fails unless a service runs now. In installer mode it fails unless the service is
    /// enabled, so it starts once the installed system does.
    pub fn require_active(&self, unit: &str, scope: ServiceScope) -> ModuleResult<Applied> {
        if self.installer_mode() {
            if !self.service_enabled(unit, scope)? {
                return Err(format!(
                    "{unit} is not enabled, so nothing starts it on the installed system"
                )
                .into());
            }
            return Ok(Applied::Deferred);
        }
        if !self.service_active(unit, scope)? {
            return Err(format!("{unit} is not active").into());
        }
        Ok(Applied::Now)
    }

    /// Starts a service and checks that it runs. Installer mode only checks that the
    /// service is enabled.
    pub fn start_service(&self, unit: &str, scope: ServiceScope) -> ModuleResult<Applied> {
        self.change_running(unit, scope, "start")
    }

    /// Restarts a service, so it reads its new configuration, and checks that it runs.
    /// Installer mode only checks that the service is enabled.
    pub fn restart_service(&self, unit: &str, scope: ServiceScope) -> ModuleResult<Applied> {
        self.change_running(unit, scope, "restart")
    }

    fn change_running(
        &self,
        unit: &str,
        scope: ServiceScope,
        action: &str,
    ) -> ModuleResult<Applied> {
        if self.installer_mode() {
            self.deferred(&format!("{action} {unit}"));
            return self.require_active(unit, scope);
        }
        let sh = self.shell;
        match scope {
            ServiceScope::System => self.run(cmd!(sh, "sudo systemctl {action} {unit}"))?,
            ServiceScope::User => self.run(cmd!(sh, "systemctl --user {action} {unit}"))?,
        }
        self.require_active(unit, scope)
    }

    /// Stops a running service. Nothing runs in installer mode, so there it does nothing.
    pub fn stop_service(&self, unit: &str, scope: ServiceScope) -> ModuleResult {
        if self.installer_mode() {
            return Ok(());
        }
        let sh = self.shell;
        match scope {
            ServiceScope::System => self.run(cmd!(sh, "sudo systemctl stop {unit}")),
            ServiceScope::User => self.run(cmd!(sh, "systemctl --user stop {unit}")),
        }
    }

    /// Makes the user's systemd manager read changed unit files. Installer mode has no
    /// running manager: the one that starts at the first login reads them anyway.
    pub fn reload_user_manager(&self) -> ModuleResult {
        if self.installer_mode() {
            self.deferred("systemctl --user daemon-reload");
            return Ok(());
        }
        self.run(cmd!(self.shell, "systemctl --user daemon-reload"))
    }

    /// Whether the user's systemd manager, which runs the user services, has `group` among
    /// its groups. A login made after the group was added has it, but a manager that
    /// started earlier keeps its old groups until every session of the user ends. In
    /// installer mode the manager starts at the first login, after this run, so it has
    /// every group the run added.
    pub fn user_manager_has_group(&self, group: &str) -> ModuleResult<bool> {
        if self.installer_mode() {
            return Ok(true);
        }
        let sh = self.shell;
        let manager = format!("user@{}.service", self.read(cmd!(sh, "id -u"))?);
        let pid = self.read(cmd!(
            sh,
            "systemctl show --property=MainPID --value {manager}"
        ))?;
        if pid.is_empty() || pid == "0" {
            return Ok(false);
        }
        let groups = self.read(cmd!(sh, "ps -o supgrp= -p {pid}"))?;
        Ok(groups.split(',').any(|member| member.trim() == group))
    }

    /// Whether a graphical session of the user runs now. None runs in installer mode.
    pub fn graphical_session(&self) -> ModuleResult<bool> {
        self.service_active("graphical-session.target", ServiceScope::User)
    }

    /// Whether systemd keeps `user`'s services running without a login. Installer mode
    /// has no running login manager, so it reads the file that the login manager keeps.
    pub fn lingering(&self, user: &str) -> ModuleResult<bool> {
        if self.installer_mode() {
            return Ok(Path::new("/var/lib/systemd/linger").join(user).exists());
        }
        let (_, linger) = self.read_unchecked(cmd!(
            self.shell,
            "loginctl show-user {user} -p Linger --value"
        ))?;
        Ok(linger == "yes")
    }

    /// Whether systemd runs the services this run enables: the running system's systemd,
    /// or in installer mode the installed system's, which always boots with it.
    pub fn systemd_manages_services(&self) -> bool {
        self.installer_mode() || Path::new("/run/systemd/system").is_dir()
    }

    /// Runs a step that acts on the running system rather than on files, such as loading
    /// a kernel module or asking the running desktop to reload. In installer mode the
    /// running system is the installer's own, so the step does not run: the installed
    /// system does the same from its files when it starts. `step` names it in the output.
    pub fn on_running_system(
        &self,
        step: &str,
        action: impl FnOnce() -> ModuleResult,
    ) -> ModuleResult<Applied> {
        if self.installer_mode() {
            self.deferred(step);
            return Ok(Applied::Deferred);
        }
        action()?;
        Ok(Applied::Now)
    }

    /// Runs a step that only the system an installer-mode run builds needs, because a
    /// running system already has that part from whoever installed it, such as its boot
    /// manager. On a running system the step does not run, and the module works with what
    /// the system has.
    pub fn on_new_system(&self, action: impl FnOnce() -> ModuleResult) -> ModuleResult {
        if self.installer_mode() {
            return action();
        }
        Ok(())
    }

    /// Reports that `missing`, a part of the system that a module builds on, was not
    /// found. A running system may lack it, so there the run notes that it skips `skipped`
    /// and the module goes on without it. The system an installer-mode run builds must
    /// have it, so there it fails.
    pub fn missing_part(&self, missing: &str, skipped: &str) -> ModuleResult {
        if self.installer_mode() {
            return Err(format!("{missing} was not found, which {skipped} needs").into());
        }
        self.note(&format!(
            "Skipping {skipped} because {missing} was not found"
        ));
        Ok(())
    }

    /// Disables and stops a service after recording whether it was enabled and active.
    pub fn disable_service(&self, unit: &str, scope: ServiceScope) -> ModuleResult {
        let before = self.service_state(unit, scope)?;
        self.disable_service_previous(unit, scope, before)
    }

    /// Disables and stops a service that a program, such as a package install, may have
    /// turned on by itself. `before` is its state from before that program ran. Only a
    /// service that was on before is recorded, so `remove` turns on only what was on.
    pub fn disable_service_previous(
        &self,
        unit: &str,
        scope: ServiceScope,
        before: ServiceState,
    ) -> ModuleResult {
        if before.enabled || before.active {
            self.record(Change::ServiceDisabled {
                unit: unit.to_owned(),
                scope,
                was_enabled: before.enabled,
                was_active: before.active,
            })?;
        }
        let now = self.service_state(unit, scope)?;
        if !now.enabled && !now.active {
            return Ok(());
        }
        // Nothing runs in installer mode, so there is nothing to stop.
        let stop: &[&str] = if self.installer_mode() {
            &[]
        } else {
            &["--now"]
        };
        let sh = self.shell;
        match scope {
            ServiceScope::System => self.run(cmd!(sh, "sudo systemctl disable {stop...} {unit}")),
            ServiceScope::User => self.run(cmd!(sh, "systemctl --user disable {stop...} {unit}")),
        }
    }

    /// Records the command that undoes what a program is about to change by itself.
    /// `remove` runs it with the other undos, newest first.
    pub fn record_undo_command(&self, program: &Path, args: &[&str]) -> ModuleResult {
        self.record(Change::UndoCommand {
            program: program.to_path_buf(),
            args: args.iter().map(|arg| (*arg).to_owned()).collect(),
        })
    }

    /// Records that the person declined this module's install, so its `verify` passes.
    pub fn record_declined(&self) -> ModuleResult {
        if self.declined() {
            return Ok(());
        }
        self.record(Change::Declined)
    }

    /// Marks an earlier decline of this module undone, because the person accepted the install.
    pub fn record_accepted(&self) -> ModuleResult {
        let pending = self.state.borrow().pending(self.module.get());
        for entry in pending {
            if matches!(entry.change, Change::Declined) {
                self.record(Change::Undone {
                    snapshot: entry.snapshot,
                    index: entry.index,
                })?;
            }
        }
        Ok(())
    }

    /// Whether the person declined this module's install and has not accepted it since.
    pub fn declined(&self) -> bool {
        self.state
            .borrow()
            .pending(self.module.get())
            .iter()
            .any(|entry| matches!(entry.change, Change::Declined))
    }

    pub fn deploy_config(&self, package: &str) -> ModuleResult {
        crate::deploy::stow_package(self, package)
    }

    pub fn find_program(&self, name: &str) -> ModuleResult<PathBuf> {
        Ok(find_program(self.shell, name)?)
    }
}

pub(crate) fn scope_argument(scope: ServiceScope) -> Vec<&'static str> {
    match scope {
        ServiceScope::System => Vec::new(),
        ServiceScope::User => vec!["--user"],
    }
}

pub(crate) fn remove_user_path(path: &Path) -> ModuleResult {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_dir() => fs::remove_dir_all(path)?,
        Ok(_) => fs::remove_file(path)?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    Ok(())
}

pub(crate) fn temporary_path(purpose: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_nanos());
    std::env::temp_dir().join(format!("inayos-{purpose}-{}-{nanos}", std::process::id()))
}

/// Reads a pipe to the end, sending each line on when the output is streamed.
fn forward(pipe: impl Read, lines: Option<mpsc::Sender<String>>) -> std::io::Result<Vec<u8>> {
    let mut reader = BufReader::new(pipe);
    let mut collected = Vec::new();
    let mut line = Vec::new();
    loop {
        line.clear();
        if reader.read_until(b'\n', &mut line)? == 0 {
            break;
        }
        match &lines {
            Some(lines) => {
                let text = String::from_utf8_lossy(&line);
                let _ = lines.send(text.trim_end_matches(['\n', '\r']).to_owned());
            }
            None => collected.extend_from_slice(&line),
        }
    }
    Ok(collected)
}

fn text(bytes: Vec<u8>) -> String {
    let mut text = String::from_utf8_lossy(&bytes).into_owned();
    while text.ends_with(['\n', '\r']) {
        text.pop();
    }
    text
}
