//! Values outside a module's own files that `remove` must put back.
//!
//! Each kind lives in its own file and implements `Kind`. The variants keep their names
//! and fields, which are the serde names that `state.json` stores.
use serde::{Deserialize, Serialize};

use crate::{Context, ModuleResult, support::config_home};

pub(crate) use kde_key::Entries as KdeEntries;

/// The value of a setting that only exists or not, such as a group membership.
pub(crate) const PRESENT: &str = "present";

/// The `XDG_CONFIG_DIRS` for the `plasma-apply-*` tools, with the user's `kdedefaults`
/// folder first. A Plasma session puts it there, and it holds the global theme's defaults:
/// without it, `plasma-apply-colorscheme --accent-color` rebuilds light Breeze colors.
fn kde_config_dirs(ctx: &Context) -> std::ffi::OsString {
    let defaults = config_home(ctx).join("kdedefaults");
    let current = ctx
        .shell
        .var("XDG_CONFIG_DIRS")
        .ok()
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "/etc/xdg".to_owned());
    if std::env::split_paths(&current).any(|directory| directory == defaults) {
        return current.into();
    }
    let mut dirs = defaults.into_os_string();
    dirs.push(":");
    dirs.push(current);
    dirs
}

mod accent_color;
mod claude_mcp_server;
mod color_scheme;
mod controlled_folder_access_app;
mod cursor_theme;
mod docker_context;
mod docker_current_context;
mod flatpak_remote;
mod git_config;
mod group_exists;
mod group_member;
mod gsettings;
mod initramfs_hook;
mod json_key;
mod kde_key;
mod kwin_rule;
mod login_shell;
mod look_and_feel;
mod machine_path_entry;
mod plasma_panels;
pub(crate) mod registry_value;
mod rustup_default;
mod tailscale_operator;
mod taskbar_auto_hide;
mod user_environment;
mod vscodium_extension;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum Setting {
    KdeKey {
        file: String,
        groups: Vec<String>,
        key: String,
    },
    /// The applied KDE Plasma global theme.
    LookAndFeel,
    /// The applied KDE cursor theme.
    CursorTheme,
    /// The KDE Plasma panel file, changed while the Plasma shell of the user whose runtime
    /// directory is `runtime` is stopped.
    PlasmaPanels {
        runtime: String,
    },
    /// The KDE accent color, as `red,green,blue`.
    AccentColor,
    /// The applied KDE color scheme, by the name of its `.colors` file.
    ColorScheme,
    Gsettings {
        schema: String,
        key: String,
    },
    GitConfig {
        key: String,
    },
    LoginShell {
        user: String,
    },
    RustupDefault,
    GroupExists {
        group: String,
    },
    GroupMember {
        group: String,
        user: String,
    },
    ClaudeMcpServer {
        name: String,
    },
    /// A Docker context, whose value is its Docker endpoint.
    DockerContext {
        name: String,
    },
    /// The Docker context that the `docker` command uses.
    DockerCurrentContext,
    /// A Windows environment variable of the current user.
    UserEnvironment {
        name: String,
    },
    /// The user allowed to change Tailscale without sudo.
    TailscaleOperator,
    /// A system-wide Flatpak remote, added from its `.flatpakrepo` file.
    FlatpakRemote {
        name: String,
        from: String,
    },
    RegistryValue {
        key: String,
        name: String,
    },
    MachinePathEntry {
        entry: String,
    },
    TaskbarAutoHide,
    /// A hook in the initramfs hook list.
    InitramfsHook {
        hook: String,
    },
    /// An extension installed in VSCodium or VS Code, through the editor's `command`.
    VscodiumExtension {
        id: String,
        #[serde(default = "vscodium_command")]
        editor: String,
    },
    /// A program that Controlled Folder Access lets write to protected folders.
    /// One top-level key of a JSON object file, by its path relative to the home
    /// directory. The value is JSON text.
    JsonKey {
        file: String,
        key: String,
    },
    ControlledFolderAccessApp {
        path: String,
    },
    /// A window rule named in KWin's rule list, which only exists or not.
    KwinRule {
        name: String,
    },
}

/// The editor of a recorded extension that names none.
fn vscodium_command() -> String {
    "codium".to_owned()
}

/// How one kind of setting is named, read, written and deleted.
trait Kind {
    fn describe(&self) -> String;

    /// The current value, or `None` when the setting does not exist.
    fn read(&self, ctx: &Context) -> ModuleResult<Option<String>>;

    fn write(&self, ctx: &Context, value: &str) -> ModuleResult;

    fn delete(&self, ctx: &Context) -> ModuleResult;

    /// Whether a program that also updates the running session applies it, so it runs
    /// again even when the stored value already matches.
    fn reapplies(&self) -> bool {
        false
    }

    /// Whether the setting only exists or not. Its `read` then returns `PRESENT`, and its
    /// `write` adds it.
    fn presence(&self) -> bool {
        false
    }

    /// The KDE configuration files, by name under the config home, where `write` runs a
    /// KDE tool that changes more keys than the value. `set` records each key the tool
    /// changed there as its own KDE key setting.
    fn kde_files(&self) -> &'static [&'static str] {
        &[]
    }
}

impl Setting {
    pub fn kde(file: &str, groups: &[&str], key: &str) -> Self {
        Self::KdeKey {
            file: file.to_owned(),
            groups: groups.iter().map(|group| (*group).to_owned()).collect(),
            key: key.to_owned(),
        }
    }

    fn kind(&self) -> Box<dyn Kind + '_> {
        match self {
            Self::KdeKey { file, groups, key } => Box::new(kde_key::KdeKey { file, groups, key }),
            Self::LookAndFeel => Box::new(look_and_feel::LookAndFeel),
            Self::CursorTheme => Box::new(cursor_theme::CursorTheme),
            Self::PlasmaPanels { runtime } => Box::new(plasma_panels::PlasmaPanels { runtime }),
            Self::AccentColor => Box::new(accent_color::AccentColor),
            Self::ColorScheme => Box::new(color_scheme::ColorScheme),
            Self::Gsettings { schema, key } => Box::new(gsettings::Gsettings { schema, key }),
            Self::GitConfig { key } => Box::new(git_config::GitConfig { key }),
            Self::LoginShell { user } => Box::new(login_shell::LoginShell { user }),
            Self::RustupDefault => Box::new(rustup_default::RustupDefault),
            Self::GroupExists { group } => Box::new(group_exists::GroupExists { group }),
            Self::GroupMember { group, user } => {
                Box::new(group_member::GroupMember { group, user })
            }
            Self::ClaudeMcpServer { name } => Box::new(claude_mcp_server::ClaudeMcpServer { name }),
            Self::DockerContext { name } => Box::new(docker_context::DockerContext { name }),
            Self::DockerCurrentContext => Box::new(docker_current_context::DockerCurrentContext),
            Self::UserEnvironment { name } => Box::new(user_environment::UserEnvironment { name }),
            Self::TailscaleOperator => Box::new(tailscale_operator::TailscaleOperator),
            Self::FlatpakRemote { name, from } => {
                Box::new(flatpak_remote::FlatpakRemote { name, from })
            }
            Self::RegistryValue { key, name } => {
                Box::new(registry_value::RegistryValue { key, name })
            }
            Self::MachinePathEntry { entry } => {
                Box::new(machine_path_entry::MachinePathEntry { entry })
            }
            Self::TaskbarAutoHide => Box::new(taskbar_auto_hide::TaskbarAutoHide),
            Self::InitramfsHook { hook } => Box::new(initramfs_hook::InitramfsHook { hook }),
            Self::VscodiumExtension { id, editor } => {
                Box::new(vscodium_extension::VscodiumExtension { id, editor })
            }
            Self::JsonKey { file, key } => Box::new(json_key::JsonKey { file, key }),
            Self::ControlledFolderAccessApp { path } => {
                Box::new(controlled_folder_access_app::ControlledFolderAccessApp { path })
            }
            Self::KwinRule { name } => Box::new(kwin_rule::KwinRule { name }),
        }
    }

    pub(crate) fn reapplies(&self) -> bool {
        self.kind().reapplies()
    }

    pub(crate) fn presence(&self) -> bool {
        self.kind().presence()
    }

    pub(crate) fn kde_files(&self) -> &'static [&'static str] {
        self.kind().kde_files()
    }

    pub fn describe(&self) -> String {
        self.kind().describe()
    }

    /// The current value, or `None` when the setting does not exist.
    pub fn read(&self, ctx: &Context) -> ModuleResult<Option<String>> {
        self.kind().read(ctx)
    }

    pub fn write(&self, ctx: &Context, value: &str) -> ModuleResult {
        self.kind().write(ctx, value)
    }

    pub fn delete(&self, ctx: &Context) -> ModuleResult {
        self.kind().delete(ctx)
    }
}

/// The keys of the KDE configuration file `file` under the config home, empty when the
/// file does not exist.
pub(crate) fn kde_entries(ctx: &Context, file: &str) -> ModuleResult<KdeEntries> {
    match std::fs::read(config_home(ctx).join(file)) {
        Ok(bytes) => Ok(kde_key::parse(&String::from_utf8_lossy(&bytes))),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(KdeEntries::new()),
        Err(error) => Err(error.into()),
    }
}

/// Each key of `file` whose value differs between `before` and `after`, as a KDE key
/// setting with its value from `before`, or `None` when it was absent.
pub(crate) fn kde_changes(
    file: &str,
    before: &KdeEntries,
    after: &KdeEntries,
) -> Vec<(Setting, Option<String>)> {
    let keys: std::collections::BTreeSet<_> = before.keys().chain(after.keys()).collect();
    keys.into_iter()
        .filter(|entry| before.get(*entry) != after.get(*entry))
        .map(|entry @ (groups, key)| {
            let setting = Setting::KdeKey {
                file: file.to_owned(),
                groups: groups.clone(),
                key: key.clone(),
            };
            (setting, before.get(entry).cloned())
        })
        .collect()
}
