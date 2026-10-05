//! What a module receives: commands, questions, and recorded changes.
use std::{
    cell::{Cell, RefCell},
    fs,
    io::{BufRead, BufReader, Read, Write},
    path::{Path, PathBuf},
    process::{Command, ExitStatus, Stdio},
    sync::mpsc,
};

use myconfig_utils::{PackageSpec, PackageSystem, PackageTool, find_program, resolve_package};
use package_catalog::Package;
use serde::{Deserialize, Serialize};
use xshell::{Cmd, Shell, cmd};

use crate::{
    ModuleResult, Setting,
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

    pub fn confirm(&self, question: &str) -> Result<bool, Unanswered> {
        self.io.confirm(question)
    }

    pub fn note(&self, message: &str) {
        self.io.note(message);
    }

    // Recorded changes.

    pub(crate) fn record(&self, change: Change) -> ModuleResult {
        let mut state = self.state.borrow_mut();
        if state.recorded_in_run(&change) {
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

    pub(crate) fn package_installed(&self, spec: PackageSpec) -> ModuleResult<bool> {
        let sh = self.shell;
        let name = spec.name;
        match spec.tool {
            PackageTool::Pacman | PackageTool::Paru => self.succeeds(cmd!(sh, "pacman -Q {name}")),
            PackageTool::Apt => {
                let format = "-f=${Status}";
                let (_, status) = self.read_unchecked(cmd!(sh, "dpkg-query -W {format} {name}"))?;
                Ok(status.ends_with("install ok installed"))
            }
            PackageTool::Winget => self.succeeds(cmd!(
                sh,
                "winget list --id {name} --exact --accept-source-agreements"
            )),
        }
    }

    /// Installs packages without recording them.
    pub(crate) fn install_specs(&self, specs: &[PackageSpec]) -> ModuleResult {
        let sh = self.shell;
        let names = |tool: PackageTool| -> Vec<&str> {
            specs
                .iter()
                .filter(|spec| spec.tool == tool)
                .map(|spec| spec.name)
                .collect()
        };
        let official = names(PackageTool::Pacman);
        if !official.is_empty() {
            self.run(cmd!(
                sh,
                "sudo pacman -S --needed --noconfirm {official...}"
            ))?;
        }
        let aur = names(PackageTool::Paru);
        if !aur.is_empty() {
            crate::support::ensure_paru(self)?;
            self.run(cmd!(
                sh,
                "paru -S --needed --noconfirm --skipreview {aur...}"
            ))?;
        }
        let apt = names(PackageTool::Apt);
        if !apt.is_empty() {
            self.run(cmd!(sh, "sudo apt-get install -y {apt...}"))?;
        }
        let winget = names(PackageTool::Winget);
        if !winget.is_empty() {
            crate::support::import_winget_packages(self, &winget)?;
        }
        Ok(())
    }

    /// Uninstalls packages without recording them.
    pub(crate) fn uninstall_specs(&self, specs: &[PackageSpec]) -> ModuleResult {
        let sh = self.shell;
        let arch: Vec<_> = specs
            .iter()
            .filter(|spec| matches!(spec.tool, PackageTool::Pacman | PackageTool::Paru))
            .map(|spec| spec.name)
            .collect();
        if !arch.is_empty() {
            self.run(cmd!(sh, "sudo pacman -R --noconfirm {arch...}"))?;
        }
        let apt: Vec<_> = specs
            .iter()
            .filter(|spec| spec.tool == PackageTool::Apt)
            .map(|spec| spec.name)
            .collect();
        if !apt.is_empty() {
            self.run(cmd!(sh, "sudo apt-get remove -y {apt...}"))?;
        }
        for spec in specs.iter().filter(|spec| spec.tool == PackageTool::Winget) {
            let name = spec.name;
            self.run(cmd!(
                sh,
                "winget uninstall --id {name} --exact --silent --accept-source-agreements"
            ))?;
        }
        Ok(())
    }

    /// Installs the missing packages and records them, so `remove` uninstalls only those.
    pub fn install_packages(&self, packages: &[Package]) -> ModuleResult {
        let specs = self.specs(packages)?;
        let mut missing = Vec::new();
        for (package, spec) in specs {
            if !self.package_installed(spec)? {
                missing.push((package, spec));
            }
        }
        if missing.is_empty() {
            return Ok(());
        }
        let missing_specs: Vec<_> = missing.iter().map(|(_, spec)| *spec).collect();
        self.install_specs(&missing_specs)?;
        for (package, spec) in missing {
            if self.package_installed(spec)? {
                self.record(Change::PackageInstalled(package))?;
            } else if spec.tool == PackageTool::Winget {
                self.note(&format!("Winget could not install {}", spec.name));
            } else {
                return Err(format!("{} is not installed after installation", spec.name).into());
            }
        }
        Ok(())
    }

    /// Uninstalls the installed packages and records them, so `remove` can reinstall them.
    pub fn uninstall_packages(&self, packages: &[Package]) -> ModuleResult {
        let mut installed = Vec::new();
        for (package, spec) in self.specs(packages)? {
            if self.package_installed(spec)? {
                installed.push((package, spec));
            }
        }
        if installed.is_empty() {
            return Ok(());
        }
        let specs: Vec<_> = installed.iter().map(|(_, spec)| *spec).collect();
        self.uninstall_specs(&specs)?;
        for (package, _) in installed {
            self.record(Change::PackageUninstalled(package))?;
        }
        Ok(())
    }

    /// Changes a setting after recording its current value.
    pub fn set(&self, setting: Setting, value: &str) -> ModuleResult {
        let previous = setting.read(self)?;
        if previous.as_deref() == Some(value) {
            return if setting.reapplies() {
                setting.write(self, value)
            } else {
                Ok(())
            };
        }
        self.record(Change::Setting {
            setting: setting.clone(),
            previous,
        })?;
        setting.write(self, value)
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
        let staged = temporary_path("system-file");
        fs::write(&staged, contents)?;
        let result = self.run(cmd!(sh, "sudo install -m {mode} {staged} {path}"));
        let cleanup = fs::remove_file(&staged);
        result?;
        cleanup?;
        Ok(())
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
        let sh = self.shell;
        let user = scope_argument(scope);
        let (_, state) = self.read_unchecked(cmd!(sh, "systemctl {user...} is-enabled {unit}"))?;
        let was_enabled = state == "enabled";
        if was_enabled {
            return Ok(());
        }
        self.record(Change::Service {
            unit: unit.to_owned(),
            scope,
            was_enabled,
        })?;
        match scope {
            ServiceScope::System => self.run(cmd!(sh, "sudo systemctl enable {unit}")),
            ServiceScope::User => self.run(cmd!(sh, "systemctl --user enable {unit}")),
        }
    }

    pub fn deploy_config(&self, package: &str) -> ModuleResult {
        crate::deploy::stow_package(self, package)
    }

    /// Unstows a retired config package, keeping its deployed directory for `remove`.
    pub fn unstow_retired(&self, package: &str) -> ModuleResult {
        crate::deploy::unstow_retired(self, package)
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
