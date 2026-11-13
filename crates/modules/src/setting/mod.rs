//! Values outside a module's own files that `remove` must put back.
//!
//! Each kind lives in its own file and implements `Kind`. The variants keep their names
//! and fields, which are the serde names that `state.json` stores.
use serde::{Deserialize, Serialize};

use crate::{Context, ModuleResult};

mod accent_color;
mod claude_mcp_server;
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
mod kde_key;
mod login_shell;
mod look_and_feel;
mod machine_path_entry;
pub(crate) mod registry_value;
mod rustup_default;
mod tailscale_operator;
mod taskbar_auto_hide;
mod ufw_enabled;
mod ufw_rule;
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
    /// The KDE accent color, as `red,green,blue`.
    AccentColor,
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
    /// A `ufw` rule, given as the arguments that add it.
    UfwRule {
        rule: Vec<String>,
    },
    UfwEnabled,
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
    /// An installed VSCodium extension.
    VscodiumExtension {
        id: String,
    },
    /// A program that Controlled Folder Access lets write to protected folders.
    ControlledFolderAccessApp {
        path: String,
    },
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
            Self::AccentColor => Box::new(accent_color::AccentColor),
            Self::Gsettings { schema, key } => Box::new(gsettings::Gsettings { schema, key }),
            Self::GitConfig { key } => Box::new(git_config::GitConfig { key }),
            Self::LoginShell { user } => Box::new(login_shell::LoginShell { user }),
            Self::RustupDefault => Box::new(rustup_default::RustupDefault),
            Self::UfwRule { rule } => Box::new(ufw_rule::UfwRule { rule }),
            Self::UfwEnabled => Box::new(ufw_enabled::UfwEnabled),
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
            Self::VscodiumExtension { id } => {
                Box::new(vscodium_extension::VscodiumExtension { id })
            }
            Self::ControlledFolderAccessApp { path } => {
                Box::new(controlled_folder_access_app::ControlledFolderAccessApp { path })
            }
        }
    }

    pub(crate) fn reapplies(&self) -> bool {
        self.kind().reapplies()
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
