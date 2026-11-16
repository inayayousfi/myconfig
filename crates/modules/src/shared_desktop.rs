//! Moves the items on the shared Public and Default desktops onto your own desktop.
//!
//! Writing to the shared desktops needs administrator rights, so a non-elevated run
//! starts this program again elevated as `myconfig internal shared-desktop ...`. The
//! questions are asked before that step, so they go through `ctx.confirm` either way.
use std::{
    fs, io,
    path::{Path, PathBuf},
};

use crate::{
    Context, Footprint, Module, ModuleResult, Need, Unanswered,
    context::temporary_path,
    state::{Change, Previous},
    support::{powershell, powershell_quote},
};

pub struct SharedDesktop;

const ELEVATED_MOVE: &str = "move";
const ELEVATED_MOVE_BACK: &str = "move-back";

fn is_administrator(ctx: &Context) -> ModuleResult<bool> {
    let command = "$identity = [Security.Principal.WindowsIdentity]::GetCurrent(); $principal = [Security.Principal.WindowsPrincipal]::new($identity); $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)";
    Ok(powershell(ctx, command)?.trim() == "True")
}

/// Runs `myconfig internal shared-desktop <step>` elevated with `paths`, and waits.
fn run_elevated(ctx: &Context, step: &str, paths: &[&Path]) -> ModuleResult {
    let mut script = format!(
        "& {} 'internal' 'shared-desktop' {}",
        powershell_quote(&std::env::current_exe()?.to_string_lossy()),
        powershell_quote(step)
    );
    for path in paths {
        script.push(' ');
        script.push_str(&powershell_quote(&path.to_string_lossy()));
    }
    script.push_str("; exit $LASTEXITCODE");
    crate::support::run_elevated(ctx, &script)
}

/// Your Desktop folder as Windows reports it, which OneDrive can move out of the profile.
fn user_desktop(ctx: &Context) -> ModuleResult<PathBuf> {
    let desktop = powershell(ctx, "[Environment]::GetFolderPath('Desktop')")?;
    if desktop.is_empty() {
        return Err("Windows did not report a Desktop folder".into());
    }
    Ok(PathBuf::from(desktop))
}

fn shared_desktops() -> ModuleResult<Vec<PathBuf>> {
    let var = |name: &str| std::env::var_os(name).ok_or_else(|| format!("{name} is unset"));
    let system_drive = var("SystemDrive")?;
    Ok(vec![
        PathBuf::from(var("PUBLIC")?).join("Desktop"),
        PathBuf::from(format!(
            "{}\\Users\\Default\\Desktop",
            system_drive.to_string_lossy()
        )),
    ])
}

fn remove_item(path: &Path) -> io::Result<()> {
    if fs::symlink_metadata(path)?.is_dir() {
        fs::remove_dir_all(path)
    } else {
        fs::remove_file(path)
    }
}

fn copy_item(source: &Path, target: &Path) -> ModuleResult {
    Previous::capture(source)?.restore(target)
}

/// Moves one item, keeping the replaced target until the move succeeded.
fn move_item(source: &Path, target: &Path, replace: bool) -> ModuleResult {
    let pending = if replace {
        let mut path = PathBuf::from(format!(
            "{}.myconfig-pending-{}",
            target.display(),
            std::process::id()
        ));
        let mut suffix = 1;
        while fs::symlink_metadata(&path).is_ok() {
            path = format!("{}.{suffix}", path.display()).into();
            suffix += 1;
        }
        fs::rename(target, &path)?;
        Some(path)
    } else {
        None
    };
    let moved = match fs::rename(source, target) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::CrossesDevices => {
            copy_item(source, target).and_then(|()| Ok(remove_item(source)?))
        }
        Err(error) => Err(error.into()),
    };
    if let Err(error) = moved {
        if fs::symlink_metadata(target).is_ok() {
            remove_item(target)?;
        }
        if let Some(previous) = pending {
            fs::rename(previous, target)?;
        }
        return Err(error);
    }
    if let Some(previous) = pending {
        remove_item(&previous)?;
    }
    Ok(())
}

/// Each shared desktop item, as its path and its path on `desktop`.
fn shared_items(desktop: &Path) -> ModuleResult<Vec<(PathBuf, PathBuf)>> {
    let mut items = Vec::new();
    for source in shared_desktops()?.iter().filter(|source| source.is_dir()) {
        for entry in fs::read_dir(source)? {
            let entry = entry?;
            let name = entry.file_name();
            if !name.to_string_lossy().eq_ignore_ascii_case("desktop.ini") {
                items.push((entry.path(), desktop.join(&name)));
            }
        }
    }
    Ok(items)
}

/// Asks whether each shared item may replace the item of the same name on `desktop`.
/// Returns the shared items the person agreed to, and the first question nobody answered.
fn ask_replacements(
    ctx: &Context,
    desktop: &Path,
) -> ModuleResult<(Vec<PathBuf>, Option<Unanswered>)> {
    let mut replace = Vec::new();
    let mut unanswered = None;
    for (from, to) in shared_items(desktop)? {
        if fs::symlink_metadata(&to).is_err() {
            continue;
        }
        let question = format!(
            "The shared desktop item {} would replace {}. Replace it?",
            from.display(),
            to.display()
        );
        match ctx.confirm(&question) {
            Ok(true) => replace.push(from),
            Ok(false) => {}
            Err(error) => {
                unanswered.get_or_insert(error);
            }
        }
    }
    Ok((replace, unanswered))
}

/// Moves every shared item onto `desktop`. An item already there is replaced only when
/// its shared item is in `replace`, and kept otherwise. Each move is passed to `moved` as
/// soon as it happens; an item that fails is listed in the returned error after the
/// others were tried.
fn move_items(
    desktop: &Path,
    replace: &[PathBuf],
    moved: &mut dyn FnMut(Change) -> ModuleResult,
) -> ModuleResult {
    let mut failures = Vec::new();
    for (from, to) in shared_items(desktop)? {
        let replaced = Previous::capture(&to)?;
        let replacing = replaced != Previous::Missing;
        if replacing && !replace.contains(&from) {
            continue;
        }
        match move_item(&from, &to, replacing) {
            Ok(()) => moved(Change::Moved { from, to, replaced })?,
            Err(error) => failures.push(format!("{}: {error}", from.display())),
        }
    }
    if failures.is_empty() {
        Ok(())
    } else {
        Err(format!("could not move {}", failures.join("; ")).into())
    }
}

fn move_back_here(from: &Path, to: &Path, replaced: &Previous) -> ModuleResult {
    if fs::symlink_metadata(to).is_ok() {
        if fs::symlink_metadata(from).is_ok() {
            return Err(format!("{} already exists", from.display()).into());
        }
        move_item(to, from, false)?;
    }
    replaced.restore(to)
}

pub(crate) fn move_back(
    ctx: &Context,
    from: &Path,
    to: &Path,
    replaced: &Previous,
) -> ModuleResult {
    if is_administrator(ctx)? {
        return move_back_here(from, to, replaced);
    }
    let file = temporary_path("shared-desktop-undo");
    let change = Change::Moved {
        from: from.to_path_buf(),
        to: to.to_path_buf(),
        replaced: replaced.clone(),
    };
    fs::write(&file, serde_json::to_vec(&change)?)?;
    let result = run_elevated(ctx, ELEVATED_MOVE_BACK, &[&file]);
    let _ = fs::remove_file(&file);
    result
}

/// The elevated `move` step: moves the shared items onto `desktop`, replacing only the
/// items that the JSON list in `replace` names, and rewrites `record` after each move so
/// a failure still leaves the list of moves made.
pub fn elevated_move(record: &Path, desktop: &Path, replace: &Path) -> ModuleResult {
    let replace: Vec<PathBuf> = serde_json::from_slice(&fs::read(replace)?)?;
    let mut changes = Vec::new();
    let mut moved = |change: Change| -> ModuleResult {
        changes.push(change);
        fs::write(record, serde_json::to_vec(&changes)?)?;
        Ok(())
    };
    fs::write(record, b"[]")?;
    move_items(desktop, &replace, &mut moved)
}

/// The elevated `move-back` step: undoes the one move described in `change`.
pub fn elevated_move_back(change: &Path) -> ModuleResult {
    match serde_json::from_slice(&fs::read(change)?)? {
        Change::Moved { from, to, replaced } => move_back_here(&from, &to, &replaced),
        _ => Err("unexpected shared desktop change".into()),
    }
}

/// Moves the shared items onto `desktop`, here when this run is elevated and otherwise
/// through the elevated step, and records each move.
fn move_all(ctx: &Context, desktop: &Path, replace: &[PathBuf]) -> ModuleResult {
    if is_administrator(ctx)? {
        return move_items(desktop, replace, &mut |change| ctx.record(change));
    }
    let file = temporary_path("shared-desktop");
    let replace_file = temporary_path("shared-desktop-replace");
    fs::write(&replace_file, serde_json::to_vec(replace)?)?;
    let result =
        run_elevated(ctx, ELEVATED_MOVE, &[&file, desktop, &replace_file]).map_err(|error| {
            format!("the elevated step did not finish ({error}); untick shared-desktop to skip it")
        });
    let _ = fs::remove_file(&replace_file);
    // Record the moves the elevated step made, even when it stopped partway.
    let recorded = fs::read(&file)
        .ok()
        .map(|contents| serde_json::from_slice::<Vec<Change>>(&contents))
        .transpose()?
        .unwrap_or_default();
    let _ = fs::remove_file(&file);
    for change in recorded {
        ctx.record(change)?;
    }
    Ok(result?)
}

impl Module for SharedDesktop {
    fn name(&self) -> &'static str {
        "shared-desktop"
    }

    fn needs(&self) -> &'static [Need] {
        &[]
    }

    fn footprint(&self, _ctx: &Context) -> Footprint {
        Footprint::default()
    }

    fn install(&self, ctx: &Context) -> ModuleResult {
        if !cfg!(windows) {
            return Err("the shared desktop exists only on Windows".into());
        }
        let desktop = user_desktop(ctx)?;
        let (replace, unanswered) = ask_replacements(ctx, &desktop)?;
        move_all(ctx, &desktop, &replace)?;
        // The items nobody answered for stay, and the next install asks again.
        match unanswered {
            Some(unanswered) => Err(unanswered.into()),
            None => Ok(()),
        }
    }

    fn verify(&self, ctx: &Context) -> ModuleResult {
        let desktop = user_desktop(ctx)?;
        for source in shared_desktops()?.iter().filter(|source| source.is_dir()) {
            for entry in fs::read_dir(source)? {
                let name = entry?.file_name();
                // An item you chose to keep blocks the shared one with the same name.
                if !name.to_string_lossy().eq_ignore_ascii_case("desktop.ini")
                    && fs::symlink_metadata(desktop.join(&name)).is_err()
                {
                    return Err(format!(
                        "{} is still on a shared desktop",
                        source.join(name).display()
                    )
                    .into());
                }
            }
        }
        Ok(())
    }

    fn remove(&self, _ctx: &Context) -> ModuleResult {
        Ok(())
    }
}
