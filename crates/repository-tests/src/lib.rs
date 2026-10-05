//! Tests of repository files that are not Rust code: the config packages under
//! `dotfiles/`, their scripts and assets, and the root `install.sh`.
//!
//! Some checks run the program a file is written for, such as `zsh`, `node`, `lua`,
//! `python3`, `resvg` or `systemd-analyze`. CI installs them. The tests run on Unix only.
use std::{
    path::{Path, PathBuf},
    process::{Command, Output},
};

pub fn repository() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap()
}

pub fn dotfiles() -> PathBuf {
    repository().join("dotfiles")
}

/// A fresh temporary directory for one test, removed when dropped.
pub struct Scratch(pub PathBuf);

impl Scratch {
    pub fn new(name: &str) -> Self {
        let path =
            std::env::temp_dir().join(format!("myconfig-repository-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).unwrap();
        Self(path)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Runs a command and fails the test with its output when it does not succeed.
pub fn run(command: &mut Command) -> Output {
    let output = command
        .output()
        .unwrap_or_else(|error| panic!("could not start {command:?}: {error}"));
    assert!(
        output.status.success(),
        "{command:?} failed: {}\n{}{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    output
}
