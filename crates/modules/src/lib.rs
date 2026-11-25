//! Installer modules, the trait they share, the recorded state, and the runner.
//!
//! `MODULES-DESIGN.md` at the repository root describes this structure.
mod chrome;
mod context;
mod deploy;
pub mod leftovers;
mod package_manager;
pub mod runner;
mod setting;
pub mod state;
mod support;
#[cfg(all(test, unix))]
mod tests;

pub use chrome::Chrome;
pub use context::{
    Applied, Context, Event, Interaction, RunMode, ServiceScope, ServiceState, Step, Unanswered,
};
pub use inayos_utils::PackageSystem;
pub use package_catalog::Package;
pub use setting::Setting;

mod agent_config;
mod agents_packages;
mod android_phone;
mod appimage;
mod apt_repository;
mod arch_wsl;
mod autohotkey;
mod axidev_osk;
mod base;
mod boot_config;
mod boot_guard;
mod boot_rescue;
mod bootvine;
mod cli;
mod docker;
mod encryption;
mod environment_inventory;
mod flatpak;
mod ghostty;
mod git_config;
mod handy;
mod herdr;
mod hibernation;
mod inayos_repository;
mod kanata;
mod kanata_kde;
mod kde_plasma;
mod llvm_path;
mod login;
mod maintenance;
mod math;
mod media;
mod neovim;
mod oh_my_posh;
mod oh_my_posh_font;
mod package_group;
mod pipewire;
mod plasma_version;
mod plex_serif_font;
mod powershell_profile;
mod printing;
mod profile_packages;
mod refind;
mod registry_tweaks;
mod release_archive;
mod runtimes;
mod secure_boot;
mod shared_desktop;
mod snapshots;
mod ssh;
mod system_monitor;
mod system_tuning;
mod tailscale;
mod taskbar_auto_hide;
mod terminal_tools;
mod wake_sources;
mod windows_editor;
mod windows_terminal;
mod zsh;

pub use agent_config::{AgentConfigCopied, AgentConfigStowed};
pub use agents_packages::AgentsPackages;
pub use android_phone::AndroidPhone;
pub use appimage::AppImage;
pub use arch_wsl::ArchWsl;
pub use autohotkey::AutoHotkey;
pub use axidev_osk::AxidevOsk;
pub use base::Base;
pub use boot_guard::BootGuard;
pub use boot_rescue::BootRescue;
pub use bootvine::Bootvine;
pub use cli::Cli;
pub use docker::Docker;
pub use encryption::Encryption;
pub use environment_inventory::EnvironmentInventory;
pub use flatpak::Flatpak;
pub use ghostty::Ghostty;
pub use git_config::GitConfig;
pub use handy::Handy;
pub use herdr::{Herdr, HerdrCore, HerdrCoreSource};
pub use hibernation::Hibernation;
pub use inayos_repository::InayosRepository;
pub use kanata::Kanata;
pub use kanata_kde::KanataKde;
pub use kde_plasma::KdePlasma;
pub use llvm_path::LlvmPath;
pub use login::Login;
pub use maintenance::Maintenance;
pub use math::Math;
pub use media::Media;
pub use neovim::{Neovim, NeovimSource};
pub use oh_my_posh::OhMyPosh;
pub use oh_my_posh_font::OhMyPoshFont;
pub use package_group::PackageGroup;
pub use pipewire::Pipewire;
pub use plasma_version::PlasmaVersion;
pub use plex_serif_font::PlexSerifFont;
pub use powershell_profile::PowerShellProfile;
pub use printing::Printing;
pub use profile_packages::unnamed_packages;
pub use refind::Refind;
pub use registry_tweaks::RegistryTweaks;
pub use runtimes::Runtimes;
pub use secure_boot::SecureBoot;
pub use shared_desktop::SharedDesktop;
pub use snapshots::Snapshots;

/// Steps that `inayos internal` runs on behalf of a module, such as an elevated child.
pub mod internal {
    pub use crate::shared_desktop::{elevated_move, elevated_move_back};
}
pub use ssh::{Ssh, SshServer};
pub use system_monitor::SystemMonitor;
pub use system_tuning::SystemTuning;
pub use tailscale::Tailscale;
pub use taskbar_auto_hide::TaskbarAutoHide;
pub use terminal_tools::TerminalTools;
pub use wake_sources::WakeSources;
pub use windows_editor::WindowsEditor;
pub use windows_terminal::WindowsTerminal;
pub use zsh::Zsh;

pub type Error = Box<dyn std::error::Error + Send + Sync>;
pub type ModuleResult<T = ()> = Result<T, Error>;

/// What a module needs on the machine. `remove` asks before undoing something
/// that another installed module in the profile lists here.
#[derive(Default)]
pub struct Footprint {
    pub packages: Vec<Package>,
    pub settings: Vec<Setting>,
}

/// Another module, named as `Module::name` returns it, that a module relies on. The
/// runner skips the module when a module it needs failed or was skipped in the same run.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Need {
    /// The module cannot work without this one, so the profile must hold it, earlier in
    /// the order. When a run leaves it out, the runner installs the module only if this one
    /// verifies.
    Hard(&'static str),
    /// The module relies on this one only when the run includes it, earlier in the order.
    /// No profile check catches a misspelled name, so name it through a constant of the
    /// needed module, such as `PlasmaVersion::NAME`.
    Soft(&'static str),
    /// A soft need on every module that comes earlier in the profile, for a module that
    /// describes the whole run.
    EveryEarlier,
}

pub trait Module: Sync {
    /// Name used by the command line, the screen and the recorded state.
    fn name(&self) -> &'static str;

    /// The modules this one relies on, such as the one that installs a program it runs.
    fn needs(&self) -> &'static [Need];

    fn footprint(&self, ctx: &Context) -> Footprint;

    /// Whether install installs or removes packages. The runner skips such a module when
    /// the package database refresh failed.
    fn changes_packages(&self, ctx: &Context) -> bool {
        !self.footprint(ctx).packages.is_empty()
    }

    /// Changes the machine only through the recording functions on `Context`.
    fn install(&self, ctx: &Context) -> ModuleResult;

    /// Checks the result of `install` without changing anything.
    fn verify(&self, ctx: &Context) -> ModuleResult;

    /// Steps that recorded changes cannot express, such as letting a package manager
    /// take out what it installed. The runner then undoes the recorded changes.
    fn remove(&self, ctx: &Context) -> ModuleResult;
}
