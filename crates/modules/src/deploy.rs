//! Deploys a module's config package from `dotfiles/<package>/` with GNU Stow.
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

use embedded_dotfiles::DOTFILES;
use myconfig_utils::install_embedded_file;
use typed_fs_rs::EmbeddedDirectory;
use xshell::cmd;

use crate::{Context, ModuleResult, state::Change};

/// Real directories that keep Stow from folding them into one package, where
/// programs and Claude data written later would land inside the deployed tree.
const UNFOLDED: [&str; 2] = [".local/bin", ".claude"];

/// Top-level folders of `dotfiles/` that are not config packages.
const NOT_PACKAGES: [&str; 2] = ["assets", "old"];

/// Folders that two or more config packages contain, such as `.config/systemd/user`.
/// Each module stows only its own package, so Stow would otherwise link a shared
/// folder into whichever package comes first, and later files would land inside it.
pub(crate) fn shared_directories() -> BTreeSet<PathBuf> {
    let mut owners: BTreeMap<PathBuf, BTreeSet<String>> = BTreeMap::new();
    for file in DOTFILES.files() {
        let mut components = Path::new(file.path_from_root).components();
        let Some(package) = components.next() else {
            continue;
        };
        let package = package.as_os_str().to_string_lossy().into_owned();
        if NOT_PACKAGES.contains(&package.as_str()) {
            continue;
        }
        let relative: PathBuf = components.collect();
        for directory in relative.ancestors().skip(1) {
            if directory.as_os_str().is_empty() {
                break;
            }
            owners
                .entry(directory.to_path_buf())
                .or_default()
                .insert(package.clone());
        }
    }
    owners
        .into_iter()
        .filter(|(_, packages)| packages.len() > 1)
        .map(|(directory, _)| directory)
        .collect()
}

/// Creates `directory` and its missing parents as real folders, recording each one.
fn create_real_directory(ctx: &Context, directory: &Path) -> ModuleResult {
    let mut missing = Vec::new();
    let mut current = Some(directory);
    while let Some(path) = current {
        if exists(path) {
            break;
        }
        missing.push(path.to_path_buf());
        current = path.parent();
    }
    for path in missing.into_iter().rev() {
        fs::create_dir(&path)?;
        ctx.record(Change::CreatedDirectory {
            path,
            system: false,
        })?;
    }
    Ok(())
}

fn exists(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok()
}

pub(crate) fn stow_package(ctx: &Context, package: &str) -> ModuleResult {
    let home = ctx.home;
    let dotfiles = home.join("dotfiles");
    let files: Vec<_> = DOTFILES
        .files()
        .into_iter()
        .filter(|file| Path::new(file.path_from_root).starts_with(package))
        .collect();
    if files.is_empty() {
        return Err(format!("embedded dotfile package has no files: {package}").into());
    }
    if !exists(&dotfiles) {
        fs::create_dir(&dotfiles)?;
        ctx.record(Change::CreatedDirectory {
            path: dotfiles.clone(),
            system: false,
        })?;
    }

    // The deployed directory is generated from the repository, so it is replaced, not recorded.
    let staging = dotfiles.join(format!(".{package}.stage-{}", std::process::id()));
    if exists(&staging) {
        fs::remove_dir_all(&staging)?;
    }
    fs::create_dir(&staging)?;
    let staged = (|| -> ModuleResult {
        for file in &files {
            let relative: PathBuf = Path::new(file.path_from_root)
                .components()
                .skip(1)
                .collect();
            let destination = staging.join(relative);
            install_embedded_file(file, &destination)?;
            if fs::read(&destination)? != file.content {
                return Err(format!("staged file differs: {}", file.path_from_root).into());
            }
        }
        Ok(())
    })();
    if let Err(error) = staged {
        fs::remove_dir_all(&staging)?;
        return Err(error);
    }
    let deployed = dotfiles.join(package);
    if exists(&deployed) {
        fs::remove_dir_all(&deployed)?;
    }
    fs::rename(&staging, &deployed)?;

    for file in &files {
        let source = Path::new(file.path_from_root);
        let relative: PathBuf = source.components().skip(1).collect();
        let mut ancestor = home.to_path_buf();
        let components: Vec<_> = relative.components().collect();
        for part in &components[..components.len() - 1] {
            ancestor.push(part);
            let metadata = fs::symlink_metadata(&ancestor);
            let is_link = metadata
                .as_ref()
                .is_ok_and(|entry| entry.file_type().is_symlink());
            let expected = deployed.join(ancestor.strip_prefix(home)?);
            if is_link
                && matches!(
                    (fs::canonicalize(&ancestor), fs::canonicalize(&expected)),
                    (Ok(current), Ok(expected)) if current == expected
                )
            {
                continue;
            }
            if metadata.is_ok() && (is_link || !ancestor.is_dir()) {
                ctx.delete(&ancestor)?;
                fs::create_dir(&ancestor)?;
                ctx.record(Change::CreatedDirectory {
                    path: ancestor.clone(),
                    system: false,
                })?;
            }
        }
        let target = home.join(&relative);
        if exists(&target) {
            let expected = fs::canonicalize(deployed.join(&relative));
            let actual = fs::canonicalize(&target);
            if let (Ok(expected), Ok(actual)) = (expected, actual)
                && expected == actual
            {
                continue;
            }
            ctx.delete(&target)?;
        }
    }

    let unfolded = UNFOLDED
        .iter()
        .map(PathBuf::from)
        .chain(shared_directories());
    for relative in unfolded {
        if deployed.join(&relative).is_dir() {
            create_real_directory(ctx, &home.join(relative))?;
        }
    }
    ctx.record(Change::Stowed {
        package: package.to_owned(),
    })?;
    ctx.run(cmd!(
        ctx.shell,
        "stow --dir {dotfiles} --target {home} --restow {package}"
    ))?;
    for file in &files {
        let relative: PathBuf = Path::new(file.path_from_root)
            .components()
            .skip(1)
            .collect();
        if fs::read(home.join(relative))? != file.content {
            return Err(format!("installed file differs: {}", file.path_from_root).into());
        }
    }
    Ok(())
}

/// Checks that every file of a stowed package resolves into `~/dotfiles/<package>`.
pub(crate) fn verify_package(ctx: &Context, package: &str) -> ModuleResult {
    let deployed = ctx.home.join("dotfiles").join(package);
    for file in DOTFILES
        .files()
        .into_iter()
        .filter(|file| Path::new(file.path_from_root).starts_with(package))
    {
        let relative: PathBuf = Path::new(file.path_from_root)
            .components()
            .skip(1)
            .collect();
        let live = ctx.home.join(&relative);
        if fs::canonicalize(&live)? != fs::canonicalize(deployed.join(&relative))? {
            return Err(format!(
                "{} is not linked into {}",
                live.display(),
                deployed.display()
            )
            .into());
        }
        if fs::read(&live)? != file.content {
            return Err(format!("{} differs from the repository", live.display()).into());
        }
    }
    Ok(())
}

pub(crate) fn unstow(ctx: &Context, package: &str) -> ModuleResult {
    let dotfiles = ctx.home.join("dotfiles");
    let home = ctx.home;
    if dotfiles.join(package).is_dir() {
        ctx.run(cmd!(
            ctx.shell,
            "stow --dir {dotfiles} --target {home} --delete {package}"
        ))?;
    }
    Ok(())
}

pub(crate) fn unstow_retired(ctx: &Context, package: &str) -> ModuleResult {
    if !ctx.home.join("dotfiles").join(package).is_dir() {
        return Ok(());
    }
    ctx.record(Change::Unstowed {
        package: package.to_owned(),
    })?;
    unstow(ctx, package)
}

pub(crate) fn restow(ctx: &Context, package: &str) -> ModuleResult {
    let dotfiles = ctx.home.join("dotfiles");
    let home = ctx.home;
    if dotfiles.join(package).is_dir() {
        ctx.run(cmd!(
            ctx.shell,
            "stow --dir {dotfiles} --target {home} --restow {package}"
        ))?;
    }
    Ok(())
}
