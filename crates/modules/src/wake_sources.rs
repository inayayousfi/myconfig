//! The wake sources of the InayOS desktop: a systemd-sleep hook that, before each sleep,
//! leaves only keyboards, the power button and the lid able to wake the system.
//!
//! The hook is under `dotfiles/assets/wake-sources`, installed at the same path under `/`.
//! `docs/wake-sources.md` describes which devices keep their wake.
use std::path::{Path, PathBuf};

use embedded_dotfiles::WAKE_SOURCES_ASSETS;
use typed_fs_rs::{EmbeddedDirectory, EmbeddedFile};

use crate::{Context, Footprint, Module, ModuleResult, Need, support::read_system_bytes};

pub struct WakeSources;

impl WakeSources {
    pub const NAME: &'static str = "wake-sources";
}

/// Each embedded file with the absolute path it is installed at. Every one is a program
/// that systemd-sleep runs.
fn files() -> Vec<(PathBuf, &'static EmbeddedFile)> {
    WAKE_SOURCES_ASSETS
        .files()
        .into_iter()
        .map(|file| (Path::new("/").join(file.path_from_root), file))
        .collect()
}

fn is_current(ctx: &Context, path: &Path, file: &EmbeddedFile) -> ModuleResult<bool> {
    let path = path.to_str().ok_or("a wake sources path is not UTF-8")?;
    Ok(read_system_bytes(ctx, path)?.as_deref() == Some(file.content))
}

impl Module for WakeSources {
    fn name(&self) -> &'static str {
        Self::NAME
    }

    /// The hook runs with Bash, `find` and udev's `udevadm`, which base installs.
    fn needs(&self) -> &'static [Need] {
        &[Need::Hard("base")]
    }

    fn footprint(&self, _ctx: &Context) -> Footprint {
        Footprint::default()
    }

    /// The hook takes effect at the next sleep, so nothing runs now.
    fn install(&self, ctx: &Context) -> ModuleResult {
        for (path, file) in files() {
            ctx.write_system_file(&path, file.content, "0755")?;
        }
        Ok(())
    }

    fn verify(&self, ctx: &Context) -> ModuleResult {
        for (path, file) in files() {
            if !is_current(ctx, &path, file)? {
                return Err(format!(
                    "{} does not hold the InayOS wake sources hook",
                    path.display()
                )
                .into());
            }
        }
        Ok(())
    }

    fn remove(&self, _ctx: &Context) -> ModuleResult {
        Ok(())
    }
}
