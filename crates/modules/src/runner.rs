//! The only code that loops over modules. The command line and the screen call it.
use std::{collections::HashSet, fs};

use myconfig_utils::resolve_package;
use xshell::cmd;

use crate::{
    Context, Event, Module, ModuleResult, Package, Setting, Step,
    context::{remove_user_path, scope_argument, temporary_path},
    state::{Action, Change, Previous},
    support,
};

/// Packages that `remove` itself runs, so removing them first would break later undos.
const INSTALLER_NEEDS: [Package; 2] = [Package::Sudo, Package::Stow];

#[derive(Default, Debug)]
pub struct RemoveReport {
    /// A question had no answer, so something was kept. The command line exits with code 3.
    pub unanswered: bool,
}

enum Outcome {
    /// The change was undone, or there was nothing left to undo.
    Done,
    /// The person chose to keep it.
    Kept,
    /// Nobody could answer, so it stays recorded for a later remove.
    Unanswered,
}

fn step(
    ctx: &Context,
    module: &dyn Module,
    step: Step,
    action: impl FnOnce() -> ModuleResult,
) -> ModuleResult {
    let name = module.name();
    ctx.set_module(name);
    ctx.interaction().event(Event::Started {
        module: name.to_owned(),
        step,
    });
    let result = action().map_err(|error| format!("{name}: {error}").into());
    ctx.interaction().event(Event::Finished {
        module: name.to_owned(),
        step,
        error: result.as_ref().err().map(ToString::to_string),
    });
    result
}

/// Refreshes the package database so installs get current versions.
fn prepare_packages(ctx: &Context) -> ModuleResult {
    match ctx.package_system {
        crate::PackageSystem::Arch => ctx.run(cmd!(ctx.shell, "sudo pacman -Syu --noconfirm")),
        crate::PackageSystem::Apt => ctx.run(cmd!(ctx.shell, "sudo apt-get update")),
        crate::PackageSystem::Winget => Ok(()),
    }
}

/// Installs each module, then verifies it, in profile order. Stops at the first failure.
pub fn install(ctx: &Context, modules: &[&dyn Module]) -> ModuleResult {
    ctx.state().borrow_mut().begin(Action::Install)?;
    prepare_packages(ctx)?;
    for module in modules {
        step(ctx, *module, Step::Install, || module.install(ctx))?;
        step(ctx, *module, Step::Verify, || module.verify(ctx))?;
    }
    Ok(())
}

/// Verifies every module and reports each result.
pub fn verify(ctx: &Context, modules: &[&dyn Module]) -> Vec<(&'static str, ModuleResult)> {
    modules
        .iter()
        .map(|module| {
            (
                module.name(),
                step(ctx, *module, Step::Verify, || module.verify(ctx)),
            )
        })
        .collect()
}

/// Whether the recorded state holds changes from an install that was not removed.
pub fn installed(ctx: &Context, module: &dyn Module) -> bool {
    !ctx.state().borrow().pending(module.name()).is_empty()
}

/// Removes each module in turn. Stops at the first failure and moves on to no other module.
pub fn remove(
    ctx: &Context,
    profile: &[&dyn Module],
    modules: &[&dyn Module],
    force: bool,
) -> ModuleResult<RemoveReport> {
    ctx.state().borrow_mut().begin(Action::Remove)?;
    let removing: HashSet<_> = modules.iter().map(|module| module.name()).collect();
    let others: Vec<_> = profile
        .iter()
        .filter(|module| !removing.contains(module.name()))
        .copied()
        .collect();
    let mut report = RemoveReport::default();
    for module in modules {
        step(ctx, *module, Step::Remove, || {
            remove_one(ctx, &others, *module, force, &mut report)
        })?;
    }
    Ok(report)
}

fn remove_one(
    ctx: &Context,
    others: &[&dyn Module],
    module: &dyn Module,
    force: bool,
    report: &mut RemoveReport,
) -> ModuleResult {
    module.remove(ctx)?;
    let pending = ctx.state().borrow().pending(module.name());
    let mut complete = true;
    for entry in pending.into_iter().rev() {
        let outcome = undo(ctx, others, &entry.change, force).map_err(|error| {
            format!(
                "could not undo {}: {error}. This change and the older ones are still in place; run remove again after fixing it",
                entry.change.describe()
            )
        })?;
        match outcome {
            Outcome::Done | Outcome::Kept => ctx.record(Change::Undone {
                snapshot: entry.snapshot,
                index: entry.index,
            })?,
            Outcome::Unanswered => {
                complete = false;
                report.unanswered = true;
            }
        }
    }
    if complete {
        ctx.record(Change::Removed)?;
    }
    Ok(())
}

/// Asks a question. With no answer, the reason goes to the person and the item stays.
fn ask(ctx: &Context, question: &str, kept: &str) -> Outcome {
    match ctx.confirm(question) {
        Ok(true) => Outcome::Done,
        Ok(false) => {
            ctx.note(&format!("Kept {kept}"));
            Outcome::Kept
        }
        Err(unanswered) => {
            ctx.note(&format!("Kept {kept}: {unanswered}"));
            Outcome::Unanswered
        }
    }
}

/// Asks before undoing something that other modules or packages still need.
fn still_needed(ctx: &Context, needers: &[String], what: &str, force: bool) -> Option<Outcome> {
    if needers.is_empty() || force {
        return None;
    }
    match ask(
        ctx,
        &format!(
            "{what} is still needed by {}. Remove it anyway?",
            needers.join(", ")
        ),
        what,
    ) {
        Outcome::Done => None,
        outcome => Some(outcome),
    }
}

fn undo(
    ctx: &Context,
    others: &[&dyn Module],
    change: &Change,
    force: bool,
) -> ModuleResult<Outcome> {
    let sh = ctx.shell;
    match change {
        Change::PackageInstalled(package) => {
            let spec = resolve_package(*package, ctx.package_system)
                .ok_or_else(|| format!("no package mapping for {package:?}"))?;
            if !ctx.package_installed(spec)? {
                return Ok(Outcome::Done);
            }
            if support::running_kernel_package(ctx)?.as_deref() == Some(spec.name) {
                ctx.note(&format!("Kept {}: it is the running kernel", spec.name));
                return Ok(Outcome::Kept);
            }
            let mut needers: Vec<String> = INSTALLER_NEEDS
                .contains(package)
                .then(|| "myconfig itself, to remove the other modules".to_owned())
                .into_iter()
                .collect();
            needers.extend(
                others
                    .iter()
                    .filter(|module| module.footprint(ctx).packages.contains(package))
                    .map(|module| format!("module {}", module.name())),
            );
            needers.extend(
                support::dependents(ctx, spec)?
                    .into_iter()
                    .map(|name| format!("package {name}")),
            );
            if let Some(outcome) =
                still_needed(ctx, &needers, &format!("package {}", spec.name), force)
            {
                return Ok(outcome);
            }
            ctx.uninstall_specs(&[spec])?;
        }
        Change::PackageUninstalled(package) => {
            let spec = resolve_package(*package, ctx.package_system)
                .ok_or_else(|| format!("no package mapping for {package:?}"))?;
            if ctx.package_installed(spec)? {
                return Ok(Outcome::Done);
            }
            let outcome = ask(
                ctx,
                &format!(
                    "Install {} again? Install uninstalled it on purpose.",
                    spec.name
                ),
                &format!("{} uninstalled", spec.name),
            );
            if !matches!(outcome, Outcome::Done) {
                return Ok(outcome);
            }
            ctx.install_specs(&[spec])?;
        }
        Change::Setting { setting, previous } => {
            let needers = setting_needers(ctx, others, setting);
            if let Some(outcome) = still_needed(ctx, &needers, &setting.describe(), force) {
                return Ok(outcome);
            }
            let current = setting.read(ctx)?;
            match previous {
                Some(value) if current.as_deref() != Some(value) => setting.write(ctx, value)?,
                None if current.is_some() => setting.delete(ctx)?,
                _ => {}
            }
        }
        Change::File {
            path,
            system: false,
            previous,
        } => {
            remove_user_path(path)?;
            previous.restore(path)?;
        }
        Change::File {
            path,
            system: true,
            previous,
        } => match previous {
            Previous::File { contents, .. } => {
                let staged = temporary_path("restore");
                fs::write(&staged, Previous::contents(contents)?)?;
                let result = ctx.run(cmd!(sh, "sudo install -m 0644 {staged} {path}"));
                fs::remove_file(&staged)?;
                result?;
            }
            Previous::Missing => ctx.run(cmd!(sh, "sudo rm -f -- {path}"))?,
            _ => return Err(format!("unexpected recorded system path: {}", path.display()).into()),
        },
        Change::Created {
            path,
            system,
            ask: confirm,
        } => {
            if fs::symlink_metadata(path).is_err()
                && !(*system && ctx.succeeds(cmd!(sh, "sudo test -e {path}"))?)
            {
                return Ok(Outcome::Done);
            }
            if *confirm {
                let outcome = ask(
                    ctx,
                    &format!(
                        "Delete {}? It may hold changes made after install.",
                        path.display()
                    ),
                    &path.display().to_string(),
                );
                if !matches!(outcome, Outcome::Done) {
                    return Ok(outcome);
                }
            }
            if *system {
                ctx.run(cmd!(sh, "sudo rm -rf -- {path}"))?;
            } else {
                remove_user_path(path)?;
            }
        }
        Change::CreatedDirectory { path, system } => {
            if *system {
                ctx.succeeds(cmd!(sh, "sudo rmdir -- {path}"))?;
            } else if let Err(error) = fs::remove_dir(path)
                && error.kind() != std::io::ErrorKind::NotFound
            {
                ctx.note(&format!("Kept {}: it is not empty", path.display()));
            }
        }
        Change::Service {
            unit,
            scope,
            was_enabled,
        } => {
            let user = scope_argument(*scope);
            let (_, state) =
                ctx.read_unchecked(cmd!(sh, "systemctl {user...} is-enabled {unit}"))?;
            if !*was_enabled && state == "enabled" {
                match scope {
                    crate::ServiceScope::System => {
                        ctx.run(cmd!(sh, "sudo systemctl disable --now {unit}"))?
                    }
                    crate::ServiceScope::User => {
                        ctx.run(cmd!(sh, "systemctl --user disable --now {unit}"))?
                    }
                }
            }
        }
        Change::Stowed { package } => {
            crate::deploy::unstow(ctx, package)?;
            remove_user_path(&ctx.home.join("dotfiles").join(package))?;
        }
        Change::Unstowed { package } => {
            if !ctx.home.join("dotfiles").join(package).is_dir() {
                return Ok(Outcome::Done);
            }
            let outcome = ask(
                ctx,
                &format!("Link the retired config package {package} into your home again?"),
                &format!("{package} unstowed"),
            );
            if !matches!(outcome, Outcome::Done) {
                return Ok(outcome);
            }
            crate::deploy::restow(ctx, package)?;
        }
        Change::Moved { from, to, replaced } => {
            let outcome = ask(
                ctx,
                &format!("Move {} back to {}?", to.display(), from.display()),
                &format!("{} on your desktop", to.display()),
            );
            if !matches!(outcome, Outcome::Done) {
                return Ok(outcome);
            }
            crate::shared_desktop::move_back(ctx, from, to, replaced)?;
        }
        Change::WslDistribution { name } => {
            let outcome = ask(
                ctx,
                &format!("Unregister the WSL distribution {name} and delete everything inside it?"),
                &format!("WSL distribution {name}"),
            );
            if !matches!(outcome, Outcome::Done) {
                return Ok(outcome);
            }
            ctx.run(cmd!(sh, "wsl.exe --unregister {name}"))?;
        }
        Change::LocalPackage { name } => {
            if ctx.succeeds(cmd!(sh, "pacman -Q {name}"))? {
                ctx.run(cmd!(sh, "sudo pacman -R --noconfirm {name}"))?;
            }
        }
        Change::Undone { .. } | Change::Removed => {}
    }
    Ok(Outcome::Done)
}

fn setting_needers(ctx: &Context, others: &[&dyn Module], setting: &Setting) -> Vec<String> {
    others
        .iter()
        .filter(|module| module.footprint(ctx).settings.contains(setting))
        .map(|module| format!("module {}", module.name()))
        .collect()
}
