//! The Black & Pink crosshair cursor theme.
use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
};

use embedded_dotfiles::DOTFILES;
use typed_fs_rs::EmbeddedDirectory;

use crate::{Context, Footprint, Module, ModuleResult, support::verify_embedded};

pub struct CursorTheme;

const SOURCE: &str = "assets/cursor-theme/cursors/blacknpink-crosshair";

fn destination(ctx: &Context) -> PathBuf {
    ctx.home.join(".local/share/icons/blacknpink-crosshair")
}

fn theme_files() -> Vec<&'static typed_fs_rs::EmbeddedFile> {
    DOTFILES
        .files()
        .into_iter()
        .filter(|file| Path::new(file.path_from_root).starts_with(SOURCE))
        .collect()
}

/// Deletes files under `current` that the theme no longer ships.
fn remove_unselected(
    ctx: &Context,
    root: &Path,
    current: &Path,
    selected: &HashSet<PathBuf>,
) -> ModuleResult {
    for entry in fs::read_dir(current)? {
        let path = entry?.path();
        if fs::symlink_metadata(&path)?.is_dir() {
            remove_unselected(ctx, root, &path, selected)?;
            if fs::read_dir(&path)?.next().is_none() {
                ctx.delete(&path)?;
            }
        } else if !selected.contains(path.strip_prefix(root)?) {
            ctx.delete(&path)?;
        }
    }
    Ok(())
}

impl Module for CursorTheme {
    fn name(&self) -> &'static str {
        "cursor-theme"
    }

    fn footprint(&self, _ctx: &Context) -> Footprint {
        Footprint::default()
    }

    fn install(&self, ctx: &Context) -> ModuleResult {
        let files = theme_files();
        for required in ["index.theme", "cursors/default", "cursors/crosshair"] {
            if !files.iter().any(|file| {
                Path::new(file.path_from_root).strip_prefix(SOURCE).ok()
                    == Some(Path::new(required))
            }) {
                return Err(format!("cursor theme is missing {required}").into());
            }
        }
        let destination = destination(ctx);
        let mut selected = HashSet::new();
        for file in files {
            let relative = Path::new(file.path_from_root).strip_prefix(SOURCE)?;
            selected.insert(relative.to_path_buf());
            let mut ancestor = destination.clone();
            for component in relative.parent().into_iter().flat_map(Path::components) {
                ancestor.push(component);
                if fs::symlink_metadata(&ancestor)
                    .is_ok_and(|metadata| metadata.file_type().is_symlink())
                {
                    return Err(format!(
                        "cursor theme parent is a symbolic link: {}",
                        ancestor.display()
                    )
                    .into());
                }
            }
            ctx.write_file(&destination.join(relative), file.content, file.executable)?;
        }
        remove_unselected(ctx, &destination, &destination, &selected)
    }

    fn verify(&self, ctx: &Context) -> ModuleResult {
        verify_embedded(theme_files(), Path::new(SOURCE), &destination(ctx))
    }

    fn remove(&self, _ctx: &Context) -> ModuleResult {
        Ok(())
    }
}
