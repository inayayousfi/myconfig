//! Deploys a module's config package from `dotfiles/<package>/` with GNU Stow.
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

use embedded_dotfiles::CONFIG_PACKAGES;
use myconfig_utils::install_embedded_file;
use typed_fs_rs::EmbeddedFile;
use xshell::cmd;

use crate::{Context, ModuleResult, state::Change};

/// Real directories that keep Stow from folding them into one package, where data that
/// programs write later would land inside the deployed tree. systemd ignores a linked
/// drop-in directory, so those stay real directories as well. Each one applies only to a
/// package that contains it.
const UNFOLDED: [&str; 12] = [
    ".local/bin",
    ".claude",
    ".local/share/applications",
    ".local/share/kwin/scripts",
    ".local/share/plasma/plasmoids",
    ".config/fontconfig/conf.d",
    ".config/systemd/user/app-com.mitchellh.ghostty.service.d",
    // VSCodium keeps its history and workspace storage beside its settings.
    ".config/VSCodium/User",
    // T3 Code keeps its database and secrets beside its client settings.
    ".t3/userdata",
    // agent-config writes the fx MCP servers and the instruction links after deploy.
    ".fx",
    // OpenCode writes its own files beside its config.
    ".config/opencode",
    // Neovim writes its plugin lockfile beside its config.
    ".config/nvim",
];

/// The embedded files of a config package, with paths relative to the package folder.
fn package_files(package: &str) -> ModuleResult<Vec<&'static EmbeddedFile>> {
    CONFIG_PACKAGES
        .iter()
        .find(|(name, _)| *name == package)
        .map(|(_, files)| files.files())
        .ok_or_else(|| format!("no embedded config package named {package}").into())
}

/// Folders that two or more config packages contain, such as `.config/systemd/user`.
/// Each module stows only its own package, so Stow would otherwise link a shared
/// folder into whichever package comes first, and later files would land inside it.
pub(crate) fn shared_directories() -> BTreeSet<PathBuf> {
    let mut owners: BTreeMap<PathBuf, BTreeSet<String>> = BTreeMap::new();
    for (package, files) in CONFIG_PACKAGES {
        for file in files.files() {
            for directory in Path::new(file.path_from_root).ancestors().skip(1) {
                if directory.as_os_str().is_empty() {
                    break;
                }
                owners
                    .entry(directory.to_path_buf())
                    .or_default()
                    .insert(package.to_owned());
            }
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

/// The current UTC time as `YYYYmmdd_HHMMSS`.
fn timestamp() -> String {
    let seconds = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs());
    let (days, time) = (seconds / 86_400, seconds % 86_400);
    // Converts days since 1970-01-01 to a civil date (Howard Hinnant's algorithm).
    let shifted = days + 719_468;
    let era = shifted / 146_097;
    let day_of_era = shifted % 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_index = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_index + 2) / 5 + 1;
    let month = if month_index < 10 {
        month_index + 3
    } else {
        month_index - 9
    };
    let year = year_of_era + era * 400 + u64::from(month <= 2);
    format!(
        "{year:04}{month:02}{day:02}_{:02}{:02}{:02}",
        time / 3_600,
        time / 60 % 60,
        time % 60
    )
}

/// Whether `folder` holds exactly the embedded `files`: the same paths with the same
/// contents, and no other file, link or folder.
fn holds_only(folder: &Path, files: &[&EmbeddedFile]) -> ModuleResult<bool> {
    let mut expected: BTreeMap<PathBuf, &[u8]> = BTreeMap::new();
    let mut folders = BTreeSet::new();
    for file in files {
        let path = Path::new(file.path_from_root);
        expected.insert(path.to_path_buf(), file.content);
        folders.extend(
            path.ancestors()
                .skip(1)
                .filter(|ancestor| !ancestor.as_os_str().is_empty())
                .map(Path::to_path_buf),
        );
    }
    let mut matched = 0;
    let mut pending = vec![folder.to_path_buf()];
    while let Some(directory) = pending.pop() {
        for entry in fs::read_dir(&directory)? {
            let path = entry?.path();
            let relative = path.strip_prefix(folder)?;
            let kind = fs::symlink_metadata(&path)?.file_type();
            if kind.is_dir() && folders.contains(relative) {
                pending.push(path);
            } else if kind.is_file()
                && expected
                    .get(relative)
                    .is_some_and(|content| fs::read(&path).is_ok_and(|read| read == *content))
            {
                matched += 1;
            } else {
                return Ok(false);
            }
        }
    }
    Ok(matched == expected.len())
}

/// Takes the deployed `~/dotfiles/<package>` away before `deploy_config` replaces it or
/// `remove` undoes the deploy. Programs write through the links into the deployed
/// package, so a package that differs from the embedded one moves into a new
/// `~/dotfiles.backup.<time>` folder instead of being deleted.
pub(crate) fn clear_deployed_package(ctx: &Context, package: &str) -> ModuleResult {
    let deployed = ctx.home.join("dotfiles").join(package);
    if !exists(&deployed) {
        return Ok(());
    }
    if deployed.is_dir() && holds_only(&deployed, &package_files(package)?)? {
        fs::remove_dir_all(&deployed)?;
        return Ok(());
    }
    let base = format!("dotfiles.backup.{}", timestamp());
    let mut backup = ctx.home.join(&base);
    let mut suffix = 1;
    while exists(&backup.join(package)) {
        backup = ctx.home.join(format!("{base}.{suffix}"));
        suffix += 1;
    }
    fs::create_dir_all(&backup)?;
    fs::rename(&deployed, backup.join(package))?;
    Ok(())
}

pub(crate) fn stow_package(ctx: &Context, package: &str) -> ModuleResult {
    let home = ctx.home;
    let dotfiles = home.join("dotfiles");
    let files = package_files(package)?;
    if !exists(&dotfiles) {
        fs::create_dir(&dotfiles)?;
        ctx.record(Change::CreatedDirectory {
            path: dotfiles.clone(),
            system: false,
        })?;
    }

    // The deployed directory is generated from the repository, so it is replaced, not
    // recorded.
    let staging = dotfiles.join(format!(".{package}.stage-{}", std::process::id()));
    if exists(&staging) {
        fs::remove_dir_all(&staging)?;
    }
    fs::create_dir(&staging)?;
    let staged = (|| -> ModuleResult {
        for file in &files {
            let destination = staging.join(file.path_from_root);
            install_embedded_file(file, &destination)?;
            if fs::read(&destination)? != file.content {
                return Err(
                    format!("staged file differs: {package}/{}", file.path_from_root).into(),
                );
            }
        }
        Ok(())
    })();
    if let Err(error) = staged {
        fs::remove_dir_all(&staging)?;
        return Err(error);
    }
    let deployed = dotfiles.join(package);
    clear_deployed_package(ctx, package)?;
    fs::rename(&staging, &deployed)?;

    for file in &files {
        let relative = Path::new(file.path_from_root);
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
        let target = home.join(relative);
        if exists(&target) {
            let expected = fs::canonicalize(deployed.join(relative));
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
    // A later install records nothing new, so remove still unstows after undoing what the
    // first install did with the deployed files, such as enabling a unit they hold.
    let already_stowed = ctx.pending().iter().any(
        |entry| matches!(&entry.change, Change::Stowed { package: stowed } if stowed == package),
    );
    if !already_stowed {
        ctx.record(Change::Stowed {
            package: package.to_owned(),
        })?;
    }
    ctx.run(cmd!(
        ctx.shell,
        "stow --dir {dotfiles} --target {home} --restow {package}"
    ))?;
    for file in &files {
        if fs::read(home.join(file.path_from_root))? != file.content {
            return Err(
                format!("installed file differs: {package}/{}", file.path_from_root).into(),
            );
        }
    }
    Ok(())
}

/// Checks that every file of a stowed package resolves into `~/dotfiles/<package>`.
pub(crate) fn verify_package(ctx: &Context, package: &str) -> ModuleResult {
    let deployed = ctx.home.join("dotfiles").join(package);
    for file in package_files(package)? {
        let live = ctx.home.join(file.path_from_root);
        if fs::canonicalize(&live)? != fs::canonicalize(deployed.join(file.path_from_root))? {
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
