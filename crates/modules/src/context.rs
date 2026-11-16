//! What a module receives: commands, questions, and recorded changes.
use std::{
    cell::{Cell, RefCell},
    fs,
    io::{BufRead, BufReader, Read, Write},
    path::{Path, PathBuf},
    process::{Command, ExitStatus, Stdio},
    sync::mpsc,
};

use myconfig_utils::{PackageSpec, PackageSystem, find_program, resolve_package};
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

pub struct Context<'a> {
    pub package_system: PackageSystem,
    pub shell: &'a Shell,
    pub home: &'a Path,
    io: &'a dyn Interaction,
    state: &'a RefCell<StateStore>,
    module: Cell<&'static str>,
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
        }
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
        if matches!(mode, Mode::Stream) {
            self.io.output(&format!("$ {display}"));
        }
        let mut command = Command::from(cmd);
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
        let finished = self.execute(cmd, input, mode)?;
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

    /// Runs a program that talks to the terminal directly.
    pub fn with_terminal(&self, mut command: Command) -> ModuleResult {
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
        let user = scope_argument(scope);
        let (_, state) =
            self.read_unchecked(cmd!(self.shell, "systemctl {user...} is-enabled {unit}"))?;
        if state == "enabled" {
            return Ok(true);
        }
        self.record(Change::Service {
            unit: unit.to_owned(),
            scope,
            was_enabled: false,
        })?;
        Ok(false)
    }

    /// Whether a service is enabled, and whether it is active.
    pub fn service_state(&self, unit: &str, scope: ServiceScope) -> ModuleResult<ServiceState> {
        let sh = self.shell;
        let user: &[&str] = &scope_argument(scope);
        let (_, enabled) =
            self.read_unchecked(cmd!(sh, "systemctl {user...} is-enabled {unit}"))?;
        Ok(ServiceState {
            enabled: enabled == "enabled",
            active: self.succeeds(cmd!(sh, "systemctl {user...} --quiet is-active {unit}"))?,
        })
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
        let sh = self.shell;
        match scope {
            ServiceScope::System => self.run(cmd!(sh, "sudo systemctl disable --now {unit}")),
            ServiceScope::User => self.run(cmd!(sh, "systemctl --user disable --now {unit}")),
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
    std::env::temp_dir().join(format!("myconfig-{purpose}-{}-{nanos}", std::process::id()))
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
