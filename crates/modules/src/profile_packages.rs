//! The check that every package a profile installs has a name on its package system.
use std::{
    cell::RefCell,
    process::{Command, ExitStatus},
    sync::atomic::{AtomicUsize, Ordering},
};

use inayos_utils::{PackageSystem, resolve_package};
use xshell::Shell;

use crate::{Context, Event, Interaction, Module, Package, Unanswered, state::StateStore};

/// Answers nothing and shows nothing, because reading a footprint asks no question.
struct Silent;

impl Interaction for Silent {
    fn output(&self, _line: &str) {}
    fn note(&self, _message: &str) {}
    fn confirm(&self, question: &str) -> Result<bool, Unanswered> {
        Err(Unanswered {
            question: question.to_owned(),
        })
    }
    fn has_terminal(&self) -> bool {
        false
    }
    fn with_terminal(&self, _command: &mut Command) -> std::io::Result<ExitStatus> {
        Err(std::io::Error::other("no terminal"))
    }
    fn event(&self, _event: Event) {}
}

/// Each module of `profile`, by name, with each package in its footprint that has no name
/// on `package_system`.
///
/// The footprints are read with a context whose home and recorded state live in a
/// temporary folder, so the check leaves the live state alone.
pub fn unnamed_packages(
    profile: &[&dyn Module],
    package_system: PackageSystem,
) -> Vec<(&'static str, Package)> {
    static RUNS: AtomicUsize = AtomicUsize::new(0);
    let home = std::env::temp_dir().join(format!(
        "inayos-unnamed-packages-{}-{}",
        std::process::id(),
        RUNS.fetch_add(1, Ordering::Relaxed)
    ));
    let shell = Shell::new().expect("a shell for reading footprints");
    let state = RefCell::new(StateStore::open(&home.join("state")).expect("a temporary state"));
    let ctx = Context::new(package_system, &shell, &home, &Silent, &state);
    let unnamed = profile
        .iter()
        .flat_map(|module| {
            module
                .footprint(&ctx)
                .packages
                .into_iter()
                .filter(|package| resolve_package(*package, package_system).is_none())
                .map(|package| (module.name(), package))
        })
        .collect();
    drop(state);
    let _ = std::fs::remove_dir_all(&home);
    unnamed
}
