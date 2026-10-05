//! The recorded state: what each run changed, with the values from before.
use std::{
    collections::BTreeMap,
    fs,
    io::Write,
    path::{Path, PathBuf},
};

use base64::{Engine, engine::general_purpose::STANDARD};
use package_catalog::Package;
use serde::{Deserialize, Serialize};

use crate::{ModuleResult, ServiceScope, Setting};

/// Format of `state.json`. Increase it and add a migration in `migrate` when the format changes.
pub const FORMAT: u32 = 1;
const WRITTEN_BY: &str = env!("CARGO_PKG_VERSION");

#[derive(Serialize, Deserialize)]
struct StateFile {
    format: u32,
    written_by: String,
    snapshots: Vec<Snapshot>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    Install,
    Remove,
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
    /// A config package deployed with GNU Stow.
    Stowed {
        package: String,
    },
    /// A retired config package that install unstowed. Its deployed directory stays.
    Unstowed {
        package: String,
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
            Self::Stowed { package } => format!("stowed config package {package}"),
            Self::Unstowed { package } => format!("unstowed retired package {package}"),
            Self::Moved { from, to, .. } => format!("moved {} to {}", from.display(), to.display()),
            Self::WslDistribution { name } => format!("WSL distribution {name}"),
            Self::LocalPackage { name } => format!("local package {name}"),
            Self::Undone { .. } | Self::Removed => String::new(),
        }
    }
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
    pub fn open(directory: &Path) -> ModuleResult<Self> {
        fs::create_dir_all(directory)?;
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
                "another myconfig run is using {}; wait for it to finish",
                directory.display()
            )
            .into());
        }
        let path = directory.join("state.json");
        let file = match fs::read(&path) {
            Ok(contents) => migrate(serde_json::from_slice(&contents)?)?,
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

    /// Whether this run already recorded a change to the same setting or path.
    pub fn recorded_in_run(&self, change: &Change) -> bool {
        let Some(target) = change.target() else {
            return false;
        };
        self.file.snapshots.last().is_some_and(|snapshot| {
            snapshot
                .modules
                .values()
                .flatten()
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
        let mut pending: Vec<Pending> = Vec::new();
        for snapshot in &self.file.snapshots {
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

fn migrate(file: StateFile) -> ModuleResult<StateFile> {
    if file.format > FORMAT {
        return Err(format!(
            "state.json was written by myconfig {} in format {}; this myconfig {WRITTEN_BY} reads format {FORMAT} or older",
            file.written_by, file.format
        )
        .into());
    }
    // Format 1 is the first format, so there is nothing to migrate yet.
    Ok(file)
}

/// Where the recorded state lives for this user.
pub fn state_directory(home: &Path) -> PathBuf {
    if cfg!(windows) {
        std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join("AppData/Local"))
            .join("myconfig/state")
    } else {
        std::env::var_os("XDG_STATE_HOME")
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join(".local/state"))
            .join("myconfig")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn directory(name: &str) -> PathBuf {
        let path =
            std::env::temp_dir().join(format!("myconfig-state-{name}-{}", std::process::id()));
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
        assert!(error.contains("myconfig 9.0.0 in format 99"));
        fs::remove_dir_all(path).unwrap();
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
