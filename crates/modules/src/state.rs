//! The recorded state: what each run changed, with the values from before.
use std::{
    collections::BTreeMap,
    fs,
    io::Write,
    path::{Path, PathBuf},
};

use base64::{Engine, engine::general_purpose::STANDARD};
use inayos_utils::PackageSpec;
use package_catalog::Package;
use serde::{Deserialize, Serialize};
use xshell::cmd;

use crate::{
    Context, ModuleResult, ServiceScope, Setting,
    context::{remove_user_path, scope_argument},
};

/// Format of `state.json`. Increase it and add a migration in `migrate` when the format changes.
///
/// - Format 1: the state of myconfig, the earlier name of InayOS, in its `myconfig` folder.
/// - Format 2: the state of InayOS in its `inayos` folder. Recorded paths inside the
///   `myconfig` state folder name the `inayos` one, where `open_moving` moved them.
/// - Format 3: what myconfig installed under its own names sits in the leftovers group,
///   which `crate::leftovers` describes, instead of under the modules that recorded it.
pub const FORMAT: u32 = 3;
const WRITTEN_BY: &str = env!("CARGO_PKG_VERSION");

#[derive(Serialize, Deserialize)]
struct StateFile {
    format: u32,
    written_by: String,
    snapshots: Vec<Snapshot>,
}

impl StateFile {
    /// Changes the group recorded since its last complete removal, oldest first.
    fn pending(&self, module: &str) -> Vec<Pending> {
        let mut pending: Vec<Pending> = Vec::new();
        for snapshot in &self.snapshots {
            let Some(changes) = snapshot.modules.get(module) else {
                continue;
            };
            for (index, change) in changes.iter().enumerate() {
                match change {
                    Change::Removed => pending.clear(),
                    Change::Undone { snapshot, index } => {
                        pending.retain(|entry| (entry.snapshot, entry.index) != (*snapshot, *index))
                    }
                    change => pending.push(Pending {
                        snapshot: snapshot.id,
                        index,
                        change: change.clone(),
                    }),
                }
            }
        }
        pending
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    Install,
    Remove,
    /// A migration between state formats that moved records from one group to another.
    Migrate,
}

#[derive(Serialize, Deserialize)]
struct Snapshot {
    id: u64,
    action: Action,
    started: u64,
    modules: BTreeMap<String, Vec<Change>>,
}

/// What a path held before a run changed it.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Previous {
    Missing,
    File {
        /// Base64 of the file contents.
        contents: String,
        executable: bool,
    },
    Link {
        target: PathBuf,
    },
    Directory {
        entries: Vec<(String, Previous)>,
    },
}

impl Previous {
    pub fn file(contents: &[u8], executable: bool) -> Self {
        Self::File {
            contents: STANDARD.encode(contents),
            executable,
        }
    }

    pub fn contents(encoded: &str) -> ModuleResult<Vec<u8>> {
        Ok(STANDARD.decode(encoded)?)
    }

    /// Reads a path owned by the current user, following nothing.
    pub fn capture(path: &Path) -> ModuleResult<Self> {
        let metadata = match fs::symlink_metadata(path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Self::Missing),
            Err(error) => return Err(error.into()),
        };
        if metadata.file_type().is_symlink() {
            return Ok(Self::Link {
                target: fs::read_link(path)?,
            });
        }
        if metadata.is_dir() {
            let mut entries = Vec::new();
            for entry in fs::read_dir(path)? {
                let entry = entry?;
                let name = entry
                    .file_name()
                    .into_string()
                    .map_err(|name| format!("file name is not UTF-8: {name:?}"))?;
                entries.push((name, Self::capture(&entry.path())?));
            }
            entries.sort_by(|left, right| left.0.cmp(&right.0));
            return Ok(Self::Directory { entries });
        }
        Ok(Self::file(&fs::read(path)?, is_executable(&metadata)))
    }

    /// Puts a captured path back. The path must not exist.
    pub fn restore(&self, path: &Path) -> ModuleResult {
        match self {
            Self::Missing => {}
            Self::File {
                contents,
                executable,
            } => {
                if let Some(parent) = path.parent() {
                    fs::create_dir_all(parent)?;
                }
                fs::write(path, Self::contents(contents)?)?;
                set_executable(path, *executable)?;
            }
            Self::Link { target } => {
                if let Some(parent) = path.parent() {
                    fs::create_dir_all(parent)?;
                }
                symlink(target, path)?;
            }
            Self::Directory { entries } => {
                fs::create_dir_all(path)?;
                for (name, entry) in entries {
                    entry.restore(&path.join(name))?;
                }
            }
        }
        Ok(())
    }
}

#[cfg(unix)]
fn is_executable(metadata: &fs::Metadata) -> bool {
    use std::os::unix::fs::PermissionsExt;
    metadata.permissions().mode() & 0o111 != 0
}

#[cfg(not(unix))]
fn is_executable(_metadata: &fs::Metadata) -> bool {
    false
}

#[cfg(unix)]
pub(crate) fn set_executable(path: &Path, executable: bool) -> ModuleResult {
    use std::os::unix::fs::PermissionsExt;
    if executable {
        let mut permissions = fs::metadata(path)?.permissions();
        permissions.set_mode(permissions.mode() | 0o111);
        fs::set_permissions(path, permissions)?;
    }
    Ok(())
}

#[cfg(not(unix))]
pub(crate) fn set_executable(_path: &Path, _executable: bool) -> ModuleResult {
    Ok(())
}

#[cfg(unix)]
pub(crate) fn symlink(target: &Path, path: &Path) -> ModuleResult {
    std::os::unix::fs::symlink(target, path)?;
    Ok(())
}

#[cfg(windows)]
pub(crate) fn symlink(target: &Path, path: &Path) -> ModuleResult {
    let resolved = path.parent().map(|parent| parent.join(target));
    if resolved.is_some_and(|resolved| resolved.is_dir()) {
        std::os::windows::fs::symlink_dir(target, path)?;
    } else {
        std::os::windows::fs::symlink_file(target, path)?;
    }
    Ok(())
}

/// One change made by a module, with what is needed to undo it.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Change {
    /// The package was missing and the module installed it.
    PackageInstalled(Package),
    /// The package was installed and the module uninstalled it.
    PackageUninstalled(Package),
    Setting {
        setting: Setting,
        previous: Option<String>,
    },
    File {
        path: PathBuf,
        system: bool,
        previous: Previous,
    },
    /// The path did not exist and the module created it, directly or through a program.
    Created {
        path: PathBuf,
        system: bool,
        /// Asks before deleting, because people or agents edit it after install.
        ask: bool,
    },
    /// A parent directory created for a file. Undo deletes it only when it is empty.
    CreatedDirectory {
        path: PathBuf,
        system: bool,
    },
    Service {
        unit: String,
        scope: ServiceScope,
        was_enabled: bool,
    },
    /// A service that the module disabled and stopped, with its state from before.
    ServiceDisabled {
        unit: String,
        scope: ServiceScope,
        was_enabled: bool,
        was_active: bool,
    },
    /// A command that undoes what a program changed by itself.
    UndoCommand {
        program: PathBuf,
        args: Vec<String>,
    },
    /// A config package deployed with GNU Stow.
    Stowed {
        package: String,
    },
    /// A retired config package that install unstowed. Its deployed directory stays.
    Unstowed {
        package: String,
    },
    /// The links that Stow made into the deployed config package for files under the
    /// names of myconfig, the earlier name of InayOS. Undo deletes those whose target is
    /// gone.
    EarlierNameLinks {
        package: String,
    },
    /// A block of lines in a root-owned file, from the line `begin` to the line `end`,
    /// that an install added. Undo takes out exactly those lines, with the empty line
    /// right before `begin` that came with them.
    MarkedBlock {
        path: PathBuf,
        begin: String,
        end: String,
    },
    Moved {
        from: PathBuf,
        to: PathBuf,
        replaced: Previous,
    },
    WslDistribution {
        name: String,
    },
    /// A package built from this repository, outside the package catalog.
    LocalPackage {
        name: String,
    },
    /// The person declined the module's install, so its verify passes. The next install
    /// asks again, and a yes marks this record undone.
    Declined,
    /// Marks a change from an earlier snapshot as undone or deliberately kept.
    Undone {
        snapshot: u64,
        index: usize,
    },
    /// The module was removed completely. Earlier changes no longer apply.
    Removed,
}

impl Change {
    /// Identifies what the change touches, so a run records the oldest value only once.
    fn target(&self) -> Option<String> {
        match self {
            Self::Setting { setting, .. } => Some(format!("setting:{setting:?}")),
            Self::File { path, .. } => Some(format!("path:{}", path.display())),
            _ => None,
        }
    }

    pub fn describe(&self) -> String {
        match self {
            Self::PackageInstalled(package) => format!("installed package {package:?}"),
            Self::PackageUninstalled(package) => format!("uninstalled package {package:?}"),
            Self::Setting { setting, .. } => format!("setting {}", setting.describe()),
            Self::File { path, .. } => format!("file {}", path.display()),
            Self::Created { path, .. } | Self::CreatedDirectory { path, .. } => {
                format!("created {}", path.display())
            }
            Self::Service { unit, .. } => format!("service {unit}"),
            Self::ServiceDisabled { unit, .. } => format!("disabled service {unit}"),
            Self::UndoCommand { program, args } => {
                format!(
                    "change undone by `{} {}`",
                    program.display(),
                    args.join(" ")
                )
            }
            Self::Stowed { package } => format!("stowed config package {package}"),
            Self::Unstowed { package } => format!("unstowed retired package {package}"),
            Self::EarlierNameLinks { package } => {
                format!("dead links of config package {package} under myconfig names")
            }
            Self::MarkedBlock { path, begin, .. } => {
                format!("block {begin} in {}", path.display())
            }
            Self::Moved { from, to, .. } => format!("moved {} to {}", from.display(), to.display()),
            Self::WslDistribution { name } => format!("WSL distribution {name}"),
            Self::LocalPackage { name } => format!("local package {name}"),
            Self::Declined => "declined install".to_owned(),
            Self::Undone { .. } | Self::Removed => String::new(),
        }
    }

    /// Puts back what the change replaced. A question that the undo raises goes to
    /// `questions`, and its answer can stop the undo.
    pub(crate) fn undo(&self, ctx: &Context, questions: &dyn Questions) -> ModuleResult<Outcome> {
        let sh = ctx.shell;
        match self {
            Self::PackageInstalled(package) => {
                let manager = ctx.packages();
                let spec = ctx.spec(*package)?;
                if !manager.installed(ctx, spec)? {
                    return Ok(Outcome::Done);
                }
                if manager.running_kernel(ctx)?.as_deref() == Some(spec.name) {
                    ctx.note(&format!(
                        "Kept {}: it is the running kernel. Remove tries again later.",
                        spec.name
                    ));
                    return Ok(Outcome::Stays);
                }
                let dependents = manager.dependents(ctx, spec)?;
                let with_dependents = !dependents.is_empty();
                if let Some(outcome) = questions.still_needed(Needed::Package(spec), dependents) {
                    return Ok(outcome);
                }
                if with_dependents {
                    manager.uninstall_with_dependents(ctx, spec)?;
                } else {
                    manager.uninstall(ctx, &[spec])?;
                }
            }
            Self::PackageUninstalled(package) => {
                let manager = ctx.packages();
                let spec = ctx.spec(*package)?;
                if manager.installed(ctx, spec)? {
                    return Ok(Outcome::Done);
                }
                let outcome = questions.confirm(
                    &format!(
                        "Install {} again? Install uninstalled it on purpose.",
                        spec.name
                    ),
                    &format!("{} uninstalled", spec.name),
                );
                if !matches!(outcome, Outcome::Done) {
                    return Ok(outcome);
                }
                manager.install(ctx, &[spec])?;
                manager.restore_saved_configuration(ctx, spec)?;
            }
            Self::Setting { setting, previous } => {
                if let Some(outcome) = questions.still_needed(Needed::Setting(setting), Vec::new())
                {
                    return Ok(outcome);
                }
                let current = setting.read(ctx)?;
                match previous {
                    Some(value) if current.as_deref() != Some(value) => {
                        setting.write(ctx, value)?
                    }
                    None if current.is_some() => setting.delete(ctx)?,
                    _ => {}
                }
            }
            Self::File {
                path,
                system: false,
                previous,
            } => {
                remove_user_path(path)?;
                previous.restore(path)?;
            }
            Self::File {
                path,
                system: true,
                previous,
            } => match previous {
                Previous::File { contents, .. } => {
                    crate::support::install_system_file(
                        ctx,
                        path,
                        &Previous::contents(contents)?,
                        "0644",
                    )?;
                }
                Previous::Missing => ctx.run(cmd!(sh, "sudo rm -f -- {path}"))?,
                Previous::Link { target } => {
                    ctx.run(cmd!(sh, "sudo ln -sfn -- {target} {path}"))?;
                }
                _ => {
                    return Err(
                        format!("unexpected recorded system path: {}", path.display()).into(),
                    );
                }
            },
            Self::Created {
                path,
                system,
                ask: confirm,
            } => {
                if fs::symlink_metadata(path).is_err()
                    && !(*system && ctx.succeeds(cmd!(sh, "sudo test -e {path}"))?)
                {
                    return Ok(Outcome::Done);
                }
                if *confirm {
                    let outcome = questions.confirm(
                        &format!(
                            "Delete {}? It may hold changes made after install.",
                            path.display()
                        ),
                        &path.display().to_string(),
                    );
                    if !matches!(outcome, Outcome::Done) {
                        return Ok(outcome);
                    }
                }
                if *system {
                    ctx.run(cmd!(sh, "sudo rm -rf -- {path}"))?;
                } else {
                    remove_user_path(path)?;
                }
            }
            Self::CreatedDirectory { path, system } => {
                if *system {
                    ctx.succeeds(cmd!(sh, "sudo rmdir -- {path}"))?;
                } else if let Err(error) = fs::remove_dir(path)
                    && error.kind() != std::io::ErrorKind::NotFound
                {
                    ctx.note(&format!("Kept {}: it is not empty", path.display()));
                }
            }
            Self::Service {
                unit,
                scope,
                was_enabled,
            } => {
                let user = scope_argument(*scope);
                let (_, state) =
                    ctx.read_unchecked(cmd!(sh, "systemctl {user...} is-enabled {unit}"))?;
                // A unit file that is gone can still leave the links that enabled it, and
                // disable removes them without the file.
                if !*was_enabled && (state == "enabled" || state == "not-found") {
                    let user = scope_argument(*scope);
                    let (_, active) =
                        ctx.read_unchecked(cmd!(sh, "systemctl {user...} is-active {unit}"))?;
                    // The unit stops first: disable deletes a unit file that is a link from
                    // outside systemd's folders, such as a stowed one, and stop then fails.
                    match scope {
                        ServiceScope::System => {
                            if active == "active" {
                                ctx.run(cmd!(sh, "sudo systemctl stop {unit}"))?;
                            }
                            ctx.run(cmd!(sh, "sudo systemctl disable {unit}"))?
                        }
                        ServiceScope::User => {
                            if active == "active" {
                                ctx.run(cmd!(sh, "systemctl --user stop {unit}"))?;
                            }
                            ctx.run(cmd!(sh, "systemctl --user disable {unit}"))?
                        }
                    }
                }
            }
            Self::ServiceDisabled {
                unit,
                scope,
                was_enabled,
                was_active,
            } => {
                let user: &[&str] = &scope_argument(*scope);
                let (_, state) =
                    ctx.read_unchecked(cmd!(sh, "systemctl {user...} is-enabled {unit}"))?;
                let systemctl = |action: &str| match scope {
                    ServiceScope::System => ctx.run(cmd!(sh, "sudo systemctl {action} {unit}")),
                    ServiceScope::User => ctx.run(cmd!(sh, "systemctl --user {action} {unit}")),
                };
                if *was_enabled && state != "enabled" {
                    systemctl("enable")?;
                }
                if *was_active
                    && !ctx.succeeds(cmd!(sh, "systemctl {user...} --quiet is-active {unit}"))?
                {
                    systemctl("start")?;
                }
            }
            Self::UndoCommand { program, args } => ctx.run(cmd!(sh, "{program} {args...}"))?,
            Self::Stowed { package } => {
                crate::deploy::unstow(ctx, package)?;
                crate::deploy::clear_deployed_package(ctx, package)?;
            }
            Self::Unstowed { package } => {
                if !ctx.home.join("dotfiles").join(package).is_dir() {
                    return Ok(Outcome::Done);
                }
                let outcome = questions.confirm(
                    &format!("Link the retired config package {package} into your home again?"),
                    &format!("{package} unstowed"),
                );
                if !matches!(outcome, Outcome::Done) {
                    return Ok(outcome);
                }
                crate::deploy::restow(ctx, package)?;
            }
            Self::EarlierNameLinks { package } => {
                crate::deploy::remove_dead_links(ctx, package, crate::leftovers::earlier_name)?
            }
            Self::MarkedBlock { path, begin, end } => {
                let shown = path.display();
                let Some(bytes) = crate::support::read_system_bytes(ctx, &path.to_string_lossy())?
                else {
                    return Ok(Outcome::Done);
                };
                let text = String::from_utf8(bytes)
                    .map_err(|_| format!("{shown} is not text, so its block {begin} stays"))?;
                if let Some(edited) = without_marked_block(&text, begin, end)
                    .map_err(|error| format!("{shown}: {error}"))?
                {
                    crate::support::install_system_file(ctx, path, edited.as_bytes(), "0644")?;
                }
            }
            Self::Moved { from, to, replaced } => {
                let outcome = questions.confirm(
                    &format!("Move {} back to {}?", to.display(), from.display()),
                    &format!("{} on your desktop", to.display()),
                );
                if !matches!(outcome, Outcome::Done) {
                    return Ok(outcome);
                }
                crate::shared_desktop::move_back(ctx, from, to, replaced)?;
            }
            Self::WslDistribution { name } => {
                if !crate::arch_wsl::distributions(ctx)?
                    .iter()
                    .any(|registered| registered.eq_ignore_ascii_case(name))
                {
                    return Ok(Outcome::Done);
                }
                let outcome = questions.confirm(
                    &format!(
                        "Unregister the WSL distribution {name} and delete everything inside it?"
                    ),
                    &format!("WSL distribution {name}"),
                );
                if !matches!(outcome, Outcome::Done) {
                    return Ok(outcome);
                }
                ctx.run(cmd!(sh, "wsl.exe --unregister {name}"))?;
            }
            Self::LocalPackage { name } => ctx.packages().uninstall_local(ctx, name)?,
            Self::Declined | Self::Undone { .. } | Self::Removed => {}
        }
        Ok(Outcome::Done)
    }
}

/// `text` without each block of lines from the line `begin` to the line `end`, and
/// without the empty line right before each `begin`. Every other byte stays. `None` when
/// `text` holds no `begin` line. A `begin` line without its `end` line is an error, so a
/// damaged block is left for the person to look at.
fn without_marked_block(text: &str, begin: &str, end: &str) -> Result<Option<String>, String> {
    let line = |piece: &str| {
        piece
            .strip_suffix('\n')
            .unwrap_or(piece)
            .trim_end_matches('\r')
            .to_owned()
    };
    let pieces: Vec<&str> = text.split_inclusive('\n').collect();
    let mut kept: Vec<&str> = Vec::new();
    let mut found = false;
    let mut index = 0;
    while index < pieces.len() {
        if line(pieces[index]) != begin {
            kept.push(pieces[index]);
            index += 1;
            continue;
        }
        let Some(last) = (index..pieces.len()).find(|&candidate| line(pieces[candidate]) == end)
        else {
            return Err(format!("the line {begin} has no {end} after it"));
        };
        if kept
            .last()
            .is_some_and(|previous| line(previous).is_empty())
        {
            kept.pop();
        }
        found = true;
        index = last + 1;
    }
    Ok(found.then(|| kept.concat()))
}

/// What became of a recorded change that `remove` reached.
pub(crate) enum Outcome {
    /// The change was undone, or there was nothing left to undo.
    Done,
    /// The person chose to keep it, so nothing owns it any more.
    Kept,
    /// The person kept it for this module, which still needs it and now owns it.
    PassedTo(&'static str),
    /// It stays recorded under its module, so a later remove tries again.
    Stays,
    /// Nobody could answer, so it stays recorded for a later remove.
    Unanswered,
}

/// A recorded change that something other than its own module may still need.
#[derive(Clone, Copy)]
pub(crate) enum Needed<'a> {
    Package(PackageSpec),
    Setting(&'a Setting),
}

/// The questions that undoing a change can raise. The runner answers them, because it
/// knows the other modules and whether `--force` was given.
pub(crate) trait Questions {
    /// Asks before a one-way undo. `kept` names what stays when the answer is no.
    fn confirm(&self, question: &str, kept: &str) -> Outcome;

    /// Asks before undoing something that other modules or the installed `dependents`
    /// may still need. `None` lets the undo go on.
    fn still_needed(&self, needed: Needed<'_>, dependents: Vec<String>) -> Option<Outcome>;
}

/// A recorded change that has not been undone yet.
pub struct Pending {
    pub snapshot: u64,
    pub index: usize,
    pub change: Change,
}

/// The open `state.json`, locked against other runs until dropped.
pub struct StateStore {
    path: PathBuf,
    file: StateFile,
    _lock: fs::File,
}

impl StateStore {
    /// Opens the state of this user, after moving it from the folder of myconfig, the
    /// earlier name of InayOS.
    pub fn open_user(home: &Path) -> ModuleResult<Self> {
        Self::open_moving(&state_directory(home), &earlier_state_directory(home))
    }

    /// Opens the state in `directory`. When only `earlier` holds a `state.json`, its
    /// entries move into `directory` first, and the migration from format 1 points the
    /// recorded paths inside `earlier` to `directory`.
    pub fn open_moving(directory: &Path, earlier: &Path) -> ModuleResult<Self> {
        move_earlier_state(earlier, directory)?;
        Self::open_with(directory, Some(earlier))
    }

    pub fn open(directory: &Path) -> ModuleResult<Self> {
        Self::open_with(directory, None)
    }

    fn open_with(directory: &Path, earlier: Option<&Path>) -> ModuleResult<Self> {
        fs::create_dir_all(directory)?;
        let lock = lock(directory)?;
        let path = directory.join("state.json");
        let file = match fs::read(&path) {
            Ok(contents) => migrate(
                serde_json::from_slice(&contents)?,
                earlier.map(|earlier| (earlier, directory)),
            )?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => StateFile {
                format: FORMAT,
                written_by: WRITTEN_BY.to_owned(),
                snapshots: Vec::new(),
            },
            Err(error) => return Err(error.into()),
        };
        Ok(Self {
            path,
            file,
            _lock: lock,
        })
    }

    /// Starts the snapshot for this run.
    pub fn begin(&mut self, action: Action) -> ModuleResult {
        let id = self.file.snapshots.last().map_or(1, |last| last.id + 1);
        let started = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_secs();
        self.file.snapshots.push(Snapshot {
            id,
            action,
            started,
            modules: BTreeMap::new(),
        });
        self.save()
    }

    fn current(&mut self) -> ModuleResult<&mut Snapshot> {
        Ok(self
            .file
            .snapshots
            .last_mut()
            .ok_or("no run has started in the recorded state")?)
    }

    /// Whether this run already recorded a change by `module` to the same setting or path.
    pub fn recorded_in_run(&self, module: &str, change: &Change) -> bool {
        let Some(target) = change.target() else {
            return false;
        };
        self.file
            .snapshots
            .last()
            .and_then(|snapshot| snapshot.modules.get(module))
            .is_some_and(|changes| {
                changes
                    .iter()
                    .any(|recorded| recorded.target().as_ref() == Some(&target))
            })
    }

    pub fn record(&mut self, module: &str, change: Change) -> ModuleResult {
        self.current()?
            .modules
            .entry(module.to_owned())
            .or_default()
            .push(change);
        self.save()
    }

    /// Changes the module made since its last complete removal, oldest first.
    pub fn pending(&self, module: &str) -> Vec<Pending> {
        self.file.pending(module)
    }

    fn save(&mut self) -> ModuleResult {
        self.file.written_by = WRITTEN_BY.to_owned();
        let contents = serde_json::to_vec_pretty(&self.file)?;
        let temporary = self
            .path
            .with_extension(format!("json.{}", std::process::id()));
        let result = (|| -> ModuleResult {
            let mut file = fs::OpenOptions::new()
                .create(true)
                .truncate(true)
                .write(true)
                .open(&temporary)?;
            file.write_all(&contents)?;
            file.sync_all()?;
            fs::rename(&temporary, &self.path)?;
            Ok(())
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temporary);
        }
        result
    }
}

/// Locks `directory/state.lock` against other runs until the returned file is dropped.
fn lock(directory: &Path) -> ModuleResult<fs::File> {
    let lock = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(directory.join("state.lock"))?;
    // A program started at the same moment can hold the lock for an instant, so retry briefly.
    let mut attempts = 0;
    while lock.try_lock().is_err() {
        attempts += 1;
        if attempts < 20 {
            std::thread::sleep(std::time::Duration::from_millis(100));
            continue;
        }
        return Err(format!(
            "another InayOS run is using {}; wait for it to finish",
            directory.display()
        )
        .into());
    }
    Ok(lock)
}

/// Moves the entries of the state folder of myconfig into the InayOS one, such as
/// `state.json` and the `plasma-layout-version` that the layout command writes beside it.
/// It does nothing once `directory` holds a `state.json`, and an entry that `directory`
/// already holds stays where it is. `state.json` moves last, so a move cut short runs
/// again at the next start.
fn move_earlier_state(earlier: &Path, directory: &Path) -> ModuleResult {
    let state = |folder: &Path| fs::symlink_metadata(folder.join("state.json")).is_ok();
    if !state(earlier) || state(directory) {
        return Ok(());
    }
    // An earlier myconfig binary that is still running keeps its lock.
    let earlier_lock = lock(earlier)?;
    if !state(earlier) || state(directory) {
        return Ok(());
    }
    fs::create_dir_all(directory)?;
    let mut names = Vec::new();
    for entry in fs::read_dir(earlier)? {
        let name = entry?.file_name();
        if name != "state.lock" && name != "state.json" {
            names.push(name);
        }
    }
    names.push("state.json".into());
    for name in names {
        let destination = directory.join(&name);
        if fs::symlink_metadata(&destination).is_err() {
            fs::rename(earlier.join(&name), destination)?;
        }
    }
    drop(earlier_lock);
    let _ = fs::remove_file(earlier.join("state.lock"));
    // The folder stays when something else is left in it.
    let _ = fs::remove_dir(earlier);
    Ok(())
}

/// Brings `file` to the current format. `moved` names the state folder of myconfig and
/// the InayOS one, when the state may have come from the first.
fn migrate(mut file: StateFile, moved: Option<(&Path, &Path)>) -> ModuleResult<StateFile> {
    if file.format > FORMAT {
        return Err(format!(
            "state.json was written by InayOS {} in format {}; this InayOS {WRITTEN_BY} reads format {FORMAT} or older",
            file.written_by, file.format
        )
        .into());
    }
    if file.format == 1 {
        if let Some((earlier, directory)) = moved {
            migrate_from_1(&mut file, earlier, directory);
        }
        file.format = 2;
    }
    if file.format == 2 {
        migrate_from_2(&mut file)?;
        file.format = 3;
    }
    Ok(file)
}

/// Format 3: every pending record that names something myconfig installed under its own
/// name moves into the leftovers group, after the records that `leftovers::companions`
/// derives from the pending records. Each moved record is marked undone under its
/// module, which no longer owns it. The move is one new snapshot, so the earlier
/// snapshots stay as they were.
fn migrate_from_2(file: &mut StateFile) -> ModuleResult {
    let mut marks: BTreeMap<String, Vec<Change>> = BTreeMap::new();
    let mut moved: Vec<(u64, String, usize, Change)> = Vec::new();
    let mut companions: Vec<Change> = Vec::new();
    let groups: Vec<String> = file
        .snapshots
        .iter()
        .flat_map(|snapshot| snapshot.modules.keys().cloned())
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .filter(|group| group != crate::leftovers::GROUP)
        .collect();
    for group in groups {
        for entry in file.pending(&group) {
            if let Some(companion) = crate::leftovers::companions(&entry.change) {
                let known = serde_json::to_string(&companion)?;
                if !companions
                    .iter()
                    .any(|other| serde_json::to_string(other).is_ok_and(|other| other == known))
                {
                    companions.push(companion);
                }
            }
            if !crate::leftovers::names_earlier_thing(&entry.change) {
                continue;
            }
            marks
                .entry(group.clone())
                .or_default()
                .push(Change::Undone {
                    snapshot: entry.snapshot,
                    index: entry.index,
                });
            moved.push((entry.snapshot, group.clone(), entry.index, entry.change));
        }
    }
    if moved.is_empty() && companions.is_empty() {
        return Ok(());
    }
    // Oldest first, as each run recorded them, so remove undoes the newest first. The
    // companions go first, so their undo comes last, after the services whose unit files
    // the links are.
    moved.sort_by(|left, right| (left.0, &left.1, left.2).cmp(&(right.0, &right.1, right.2)));
    let leftovers: Vec<Change> = companions
        .into_iter()
        .chain(moved.into_iter().map(|(_, _, _, change)| change))
        .collect();
    marks.insert(crate::leftovers::GROUP.to_owned(), leftovers);
    let id = file.snapshots.last().map_or(1, |last| last.id + 1);
    let started = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_secs();
    file.snapshots.push(Snapshot {
        id,
        action: Action::Migrate,
        started,
        modules: marks,
    });
    Ok(())
}

/// Format 2: the state folder was renamed, so the recorded paths inside the earlier one,
/// such as `plasma-layout-version`, name the same entry in the new one.
fn migrate_from_1(file: &mut StateFile, earlier: &Path, directory: &Path) {
    let moved = |path: &mut PathBuf| {
        if let Ok(relative) = path.strip_prefix(earlier) {
            *path = directory.join(relative);
        }
    };
    for snapshot in &mut file.snapshots {
        for change in snapshot.modules.values_mut().flatten() {
            match change {
                Change::File { path, .. }
                | Change::Created { path, .. }
                | Change::CreatedDirectory { path, .. } => moved(path),
                _ => {}
            }
        }
    }
}

/// Where the recorded state lives for this user.
pub fn state_directory(home: &Path) -> PathBuf {
    named_state_directory(home, "inayos")
}

/// Where myconfig, the earlier name of InayOS, kept the recorded state.
fn earlier_state_directory(home: &Path) -> PathBuf {
    named_state_directory(home, "myconfig")
}

fn named_state_directory(home: &Path, name: &str) -> PathBuf {
    if cfg!(windows) {
        std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join("AppData/Local"))
            .join(name)
            .join("state")
    } else {
        std::env::var_os("XDG_STATE_HOME")
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join(".local/state"))
            .join(name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn directory(name: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!("inayos-state-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&path);
        path
    }

    #[test]
    fn pending_skips_undone_changes_and_restarts_after_removal() {
        let path = directory("pending");
        let mut store = StateStore::open(&path).unwrap();
        store.begin(Action::Install).unwrap();
        store
            .record("zsh", Change::PackageInstalled(Package::Zsh))
            .unwrap();
        store
            .record(
                "zsh",
                Change::Stowed {
                    package: "zsh".into(),
                },
            )
            .unwrap();
        assert_eq!(store.pending("zsh").len(), 2);

        store.begin(Action::Remove).unwrap();
        store
            .record(
                "zsh",
                Change::Undone {
                    snapshot: 1,
                    index: 1,
                },
            )
            .unwrap();
        let pending = store.pending("zsh");
        assert_eq!(pending.len(), 1);
        assert!(matches!(
            pending[0].change,
            Change::PackageInstalled(Package::Zsh)
        ));

        store.record("zsh", Change::Removed).unwrap();
        assert!(store.pending("zsh").is_empty());
        drop(store);

        let reopened = StateStore::open(&path).unwrap();
        assert!(reopened.pending("zsh").is_empty());
        let contents: serde_json::Value =
            serde_json::from_slice(&fs::read(path.join("state.json")).unwrap()).unwrap();
        assert_eq!(contents["format"], FORMAT);
        assert_eq!(contents["written_by"], WRITTEN_BY);
        drop(reopened);
        fs::remove_dir_all(path).unwrap();
    }

    /// `state.json` written by an earlier InayOS must read back the same, so every
    /// change kind keeps its serialized form.
    #[test]
    fn every_change_kind_keeps_its_serialized_form() {
        let stored = r#"[{"package_installed":"Zsh"},{"package_uninstalled":"Git"},{"setting":{"setting":{"GitConfig":{"key":"core.editor"}},"previous":"vim"}},{"setting":{"setting":{"GitConfig":{"key":"core.pager"}},"previous":null}},{"file":{"path":"/home/me/.zshrc","system":false,"previous":{"file":{"contents":"b2xkCg==","executable":true}}}},{"file":{"path":"/etc/x","system":true,"previous":"missing"}},{"file":{"path":"/home/me/link","system":false,"previous":{"link":{"target":"/elsewhere"}}}},{"file":{"path":"/home/me/dir","system":false,"previous":{"directory":{"entries":[["inner","missing"]]}}}},{"created":{"path":"/home/me/.oh-my-zsh","system":false,"ask":true}},{"created_directory":{"path":"/etc/inayos","system":true}},{"service":{"unit":"sshd.service","scope":"system","was_enabled":false}},{"service_disabled":{"unit":"kanata.service","scope":"user","was_enabled":true,"was_active":false}},{"undo_command":{"program":"/usr/bin/tool","args":["undo","--all"]}},{"stowed":{"package":"zsh"}},{"unstowed":{"package":"old"}},{"earlier_name_links":{"package":"plasma"}},{"marked_block":{"path":"/boot/EFI/refind/refind.conf","begin":"BEGIN X","end":"END X"}},{"moved":{"from":"C:\\a","to":"C:\\b","replaced":"missing"}},{"wsl_distribution":{"name":"archlinux"}},{"local_package":{"name":"bootvine"}},"declined",{"undone":{"snapshot":1,"index":2}},"removed"]"#;
        let changes: Vec<Change> = serde_json::from_str(stored).unwrap();
        assert_eq!(changes.len(), 23);
        assert_eq!(serde_json::to_string(&changes).unwrap(), stored);
    }

    #[test]
    fn a_second_run_cannot_open_the_locked_state() {
        let path = directory("lock");
        let first = StateStore::open(&path).unwrap();
        assert!(StateStore::open(&path).is_err());
        drop(first);
        StateStore::open(&path).unwrap();
        fs::remove_dir_all(path).unwrap();
    }

    #[test]
    fn a_newer_format_is_refused() {
        let path = directory("newer");
        fs::create_dir_all(&path).unwrap();
        fs::write(
            path.join("state.json"),
            br#"{"format":99,"written_by":"9.0.0","snapshots":[]}"#,
        )
        .unwrap();
        let error = StateStore::open(&path).err().unwrap().to_string();
        assert!(error.contains("InayOS 9.0.0 in format 99"));
        fs::remove_dir_all(path).unwrap();
    }

    /// A format 1 `state.json` as myconfig left it, with the layout version file that the
    /// layout command wrote beside it.
    fn earlier_state(earlier: &Path) {
        fs::create_dir_all(earlier).unwrap();
        let version = earlier.join("plasma-layout-version");
        fs::write(&version, "1\n").unwrap();
        fs::write(earlier.join("state.lock"), "").unwrap();
        let state = serde_json::json!({
            "format": 1,
            "written_by": "0.9.0",
            "snapshots": [{
                "id": 1,
                "action": "install",
                "started": 1,
                "modules": {
                    "kde-plasma": [
                        {"file": {"path": version, "system": false, "previous": "missing"}},
                        {"file": {"path": "/home/me/.config/kwinrc", "system": false, "previous": "missing"}},
                        {"service": {"unit": "myconfig-plasma-layout.service", "scope": "user", "was_enabled": false}}
                    ]
                }
            }]
        });
        fs::write(
            earlier.join("state.json"),
            serde_json::to_vec(&state).unwrap(),
        )
        .unwrap();
    }

    fn recorded_paths(store: &StateStore) -> Vec<PathBuf> {
        store
            .pending("kde-plasma")
            .into_iter()
            .filter_map(|entry| match entry.change {
                Change::File { path, .. } => Some(path),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn the_state_of_myconfig_moves_to_the_inayos_folder_and_migrates() {
        let root = directory("move");
        let earlier = root.join("myconfig");
        let current = root.join("inayos");
        earlier_state(&earlier);

        let mut store = StateStore::open_moving(&current, &earlier).unwrap();
        assert!(!earlier.exists(), "the folder of myconfig was left behind");
        assert_eq!(
            fs::read_to_string(current.join("plasma-layout-version")).unwrap(),
            "1\n"
        );
        assert_eq!(
            recorded_paths(&store),
            [
                current.join("plasma-layout-version"),
                PathBuf::from("/home/me/.config/kwinrc")
            ]
        );
        // The unit that myconfig enabled is a leftover of the earlier name.
        assert!(
            store
                .pending(crate::leftovers::GROUP)
                .iter()
                .any(|entry| matches!(
                    &entry.change,
                    Change::Service { unit, .. } if unit == "myconfig-plasma-layout.service"
                ))
        );
        store.begin(Action::Install).unwrap();
        drop(store);

        let contents: serde_json::Value =
            serde_json::from_slice(&fs::read(current.join("state.json")).unwrap()).unwrap();
        assert_eq!(contents["format"], FORMAT);
        let reopened = StateStore::open_moving(&current, &earlier).unwrap();
        assert_eq!(
            recorded_paths(&reopened)[0],
            current.join("plasma-layout-version")
        );
        assert!(
            !earlier.exists(),
            "opening again created the folder of myconfig"
        );
        drop(reopened);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn an_existing_inayos_state_leaves_the_myconfig_folder_alone() {
        let root = directory("keep");
        let earlier = root.join("myconfig");
        let current = root.join("inayos");
        earlier_state(&earlier);
        let mut first = StateStore::open(&current).unwrap();
        first.begin(Action::Install).unwrap();
        drop(first);

        let store = StateStore::open_moving(&current, &earlier).unwrap();
        assert!(store.pending("kde-plasma").is_empty());
        assert!(earlier.join("state.json").is_file());
        assert!(!current.join("plasma-layout-version").exists());
        drop(store);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_running_myconfig_keeps_its_state_where_it_is() {
        let root = directory("busy");
        let earlier = root.join("myconfig");
        let current = root.join("inayos");
        earlier_state(&earlier);
        let held = lock(&earlier).unwrap();
        assert!(StateStore::open_moving(&current, &earlier).is_err());
        assert!(earlier.join("state.json").is_file());
        assert!(!current.join("state.json").exists());
        drop(held);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_new_machine_gets_no_myconfig_folder() {
        let root = directory("fresh");
        let earlier = root.join("myconfig");
        let current = root.join("inayos");
        drop(StateStore::open_moving(&current, &earlier).unwrap());
        assert!(!earlier.exists());
        assert!(current.join("state.lock").exists());
        fs::remove_dir_all(root).unwrap();
    }

    fn changes(store: &StateStore, group: &str) -> Vec<String> {
        store
            .pending(group)
            .into_iter()
            .map(|entry| serde_json::to_string(&entry.change).unwrap())
            .collect()
    }

    /// Format 3 moves what myconfig installed under its own names into the leftovers
    /// group, and leaves the rest where it was.
    #[test]
    fn records_of_earlier_names_move_into_the_leftovers_group() {
        let path = directory("leftovers");
        fs::create_dir_all(&path).unwrap();
        let unit = |name: &str| serde_json::json!({"service": {"unit": name, "scope": "user", "was_enabled": false}});
        let system = |name: &str| serde_json::json!({"file": {"path": name, "system": true, "previous": "missing"}});
        let state = serde_json::json!({
            "format": 2,
            "written_by": "0.9.0",
            "snapshots": [
                {"id": 1, "action": "install", "started": 1, "modules": {
                    "kanata": [
                        {"stowed": {"package": "kanata"}},
                        system("/etc/modules-load.d/myconfig-kanata.conf"),
                        unit("myconfig-kanata.service"),
                        system("/etc/udev/rules.d/99-myconfig-retired.rules")
                    ],
                    "git-config": [
                        {"setting": {"setting": {"GitConfig": {"key": "url.git@github.com:someone/myconfig.git.insteadOf"}}, "previous": null}}
                    ],
                    "ssh": [system("/etc/ssh/sshd_config.d/10-myconfig.conf")],
                    "handy": [unit("myconfig-handy.service")]
                }},
                {"id": 2, "action": "remove", "started": 2, "modules": {
                    "kanata": [{"undone": {"snapshot": 1, "index": 3}}],
                    "handy": ["removed"]
                }}
            ]
        });
        fs::write(path.join("state.json"), serde_json::to_vec(&state).unwrap()).unwrap();

        let mut store = StateStore::open(&path).unwrap();
        assert_eq!(
            changes(&store, crate::leftovers::GROUP),
            [
                r#"{"earlier_name_links":{"package":"kanata"}}"#,
                r#"{"file":{"path":"/etc/modules-load.d/myconfig-kanata.conf","system":true,"previous":"missing"}}"#,
                r#"{"service":{"unit":"myconfig-kanata.service","scope":"user","was_enabled":false}}"#,
                r#"{"file":{"path":"/etc/ssh/sshd_config.d/10-myconfig.conf","system":true,"previous":"missing"}}"#,
            ]
        );
        assert_eq!(
            changes(&store, "kanata"),
            [r#"{"stowed":{"package":"kanata"}}"#]
        );
        assert_eq!(changes(&store, "git-config").len(), 1);
        assert!(changes(&store, "ssh").is_empty());
        assert!(changes(&store, "handy").is_empty());

        store.begin(Action::Install).unwrap();
        drop(store);
        let reopened = StateStore::open(&path).unwrap();
        assert_eq!(changes(&reopened, crate::leftovers::GROUP).len(), 4);
        assert_eq!(changes(&reopened, "kanata").len(), 1);
        drop(reopened);
        fs::remove_dir_all(path).unwrap();
    }

    #[test]
    fn a_state_with_nothing_of_the_earlier_name_gains_no_leftovers() {
        let path = directory("no-leftovers");
        fs::create_dir_all(&path).unwrap();
        fs::write(
            path.join("state.json"),
            br#"{"format":2,"written_by":"0.9.0","snapshots":[{"id":1,"action":"install","started":1,"modules":{"zsh":[{"package_installed":"Zsh"}]}}]}"#,
        )
        .unwrap();
        let store = StateStore::open(&path).unwrap();
        assert!(store.pending(crate::leftovers::GROUP).is_empty());
        assert_eq!(store.file.snapshots.len(), 1);
        drop(store);
        fs::remove_dir_all(path).unwrap();
    }

    #[test]
    fn a_marked_block_goes_with_its_empty_line_and_nothing_else() {
        let text = "timeout 5\n\n# BEGIN OLD\ninclude a.conf\n# END OLD\n\n# BEGIN NEW\ninclude a.conf\n# END NEW\n";
        assert_eq!(
            without_marked_block(text, "# BEGIN OLD", "# END OLD")
                .unwrap()
                .as_deref(),
            Some("timeout 5\n\n# BEGIN NEW\ninclude a.conf\n# END NEW\n")
        );
        // A block in the middle, with no empty line before it and no newline at the end.
        assert_eq!(
            without_marked_block(
                "a\n# BEGIN OLD\nx\n# END OLD\nb",
                "# BEGIN OLD",
                "# END OLD"
            )
            .unwrap()
            .as_deref(),
            Some("a\nb")
        );
        assert_eq!(
            without_marked_block("timeout 5\n", "# BEGIN OLD", "# END OLD").unwrap(),
            None
        );
        assert!(without_marked_block("# BEGIN OLD\nx\n", "# BEGIN OLD", "# END OLD").is_err());
    }

    #[test]
    fn captured_paths_restore_files_links_and_directories() {
        let path = directory("capture");
        fs::create_dir_all(path.join("tree/inner")).unwrap();
        fs::write(path.join("tree/inner/file"), b"contents").unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink("inner/file", path.join("tree/link")).unwrap();
        let captured = Previous::capture(&path.join("tree")).unwrap();
        fs::remove_dir_all(path.join("tree")).unwrap();
        captured.restore(&path.join("tree")).unwrap();
        assert_eq!(fs::read(path.join("tree/inner/file")).unwrap(), b"contents");
        #[cfg(unix)]
        assert_eq!(
            fs::read_link(path.join("tree/link")).unwrap(),
            Path::new("inner/file")
        );
        assert_eq!(
            Previous::capture(&path.join("absent")).unwrap(),
            Previous::Missing
        );
        fs::remove_dir_all(path).unwrap();
    }
}
