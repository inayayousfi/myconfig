//! Installer modules, the trait they share, the recorded state, and the runner.
//!
//! `MODULES-DESIGN.md` at the repository root describes this structure.
mod context;
mod deploy;
pub mod runner;
mod setting;
pub mod state;
mod support;
#[cfg(all(test, unix))]
mod tests;

pub use context::{Context, Event, Interaction, ServiceScope, Step, Unanswered};
pub use myconfig_utils::PackageSystem;
pub use package_catalog::Package;
pub use setting::Setting;

mod agent_config;
mod agents_packages;
mod android_phone;
mod arch_wsl;
mod autohotkey;
mod axidev_osk;
mod base;
mod cachyos_setup;
mod cli;
mod cursor_theme;
mod docker;
mod emacs;
mod environment_inventory;
mod ghostty;
mod git_config;
mod handy;
mod kanata;
mod kanata_kde;
mod kde_plasma;
mod llvm_path;
mod oh_my_posh;
mod oh_my_posh_font;
mod package_group;
mod pipewire;
mod plasma_version;
mod powershell_profile;
mod psreadline;
mod refind;
mod registry_tweaks;
mod runtimes;
mod shared_desktop;
mod ssh;
mod tailscale;
mod taskbar_auto_hide;
mod terminal_tools;
mod windows_terminal;
mod zsh;

pub use agent_config::{AgentConfigCopied, AgentConfigStowed};
pub use agents_packages::AgentsPackages;
pub use android_phone::AndroidPhone;
pub use arch_wsl::ArchWsl;
pub use autohotkey::AutoHotkey;
pub use axidev_osk::AxidevOsk;
pub use base::Base;
pub use cachyos_setup::CachyosSetup;
pub use cli::Cli;
pub use cursor_theme::CursorTheme;
pub use docker::Docker;
pub use emacs::{EmacsCopied, EmacsOptions, EmacsStowed};
pub use environment_inventory::EnvironmentInventory;
pub use ghostty::Ghostty;
pub use git_config::GitConfig;
pub use handy::Handy;
pub use kanata::Kanata;
pub use kanata_kde::KanataKde;
pub use kde_plasma::KdePlasma;
pub use llvm_path::LlvmPath;
pub use oh_my_posh::OhMyPosh;
pub use oh_my_posh_font::OhMyPoshFont;
pub use package_group::PackageGroup;
pub use pipewire::Pipewire;
pub use plasma_version::PlasmaVersion;
pub use powershell_profile::PowerShellProfile;
pub use psreadline::PsReadLine;
pub use refind::Refind;
pub use registry_tweaks::RegistryTweaks;
pub use runtimes::Runtimes;
pub use shared_desktop::SharedDesktop;

/// Steps that `myconfig internal` runs on behalf of a module, such as an elevated child.
pub mod internal {
    pub use crate::kde_plasma::repair_glass;
    pub use crate::shared_desktop::{elevated_move, elevated_move_back};
}
pub use ssh::Ssh;
pub use tailscale::Tailscale;
pub use taskbar_auto_hide::TaskbarAutoHide;
pub use terminal_tools::TerminalTools;
pub use windows_terminal::WindowsTerminal;
pub use zsh::Zsh;

pub type Error = Box<dyn std::error::Error + Send + Sync>;
pub type ModuleResult<T = ()> = Result<T, Error>;

/// What a module needs on the machine. `remove` asks before undoing something
/// that another module in the profile lists here.
#[derive(Default)]
pub struct Footprint {
    pub packages: Vec<Package>,
    pub settings: Vec<Setting>,
}

pub trait Module: Sync {
    /// Name used by the command line, the screen and the recorded state.
    fn name(&self) -> &'static str;

    fn footprint(&self, ctx: &Context) -> Footprint;

    /// Changes the machine only through the recording functions on `Context`.
    fn install(&self, ctx: &Context) -> ModuleResult;

    /// Checks the result of `install` without changing anything.
    fn verify(&self, ctx: &Context) -> ModuleResult;

    /// Steps that recorded changes cannot express, such as an external uninstaller.
    /// The runner then undoes the recorded changes.
    fn remove(&self, ctx: &Context) -> ModuleResult;
}
