//! The VM test harness behind `cargo xtask vm`. It builds the installers from the working
//! tree and runs them in throwaway QEMU guests. It runs only on a Linux host.
#![cfg(target_os = "linux")]

mod answers;
mod binary;
mod cli;
mod collect;
mod compare;
mod cycle;
mod disk;
mod download;
mod drive;
mod firmware;
mod host;
mod image;
mod lock;
mod machine;
mod os;
mod paths;
mod qmp;
mod release;
mod run;
mod seed;
mod snapshot;
mod ssh;
mod start;
mod updates;
mod vm;

use std::path::PathBuf;

pub use cli::main;

/// The repository root, which holds the workspace this crate belongs to.
fn repository() -> PathBuf {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest
        .ancestors()
        .nth(2)
        .expect("the crate sits two folders below the repository root")
        .to_owned()
}
