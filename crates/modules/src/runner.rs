//! The only code that loops over modules. The command line and the screen call it.
use std::{
    collections::{HashMap, HashSet},
    fs,
};

use myconfig_utils::resolve_package;
use xshell::cmd;

use crate::{
    Context, Event, Module, ModuleResult, Need, Package, Setting, Step, Unanswered,
    context::{remove_user_path, scope_argument},
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
    let result = action();
    // An install that nobody could answer is a skip, which the runner reports itself.
    if step == Step::Install
        && let Err(error) = &result
        && error.is::<Unanswered>()
    {
        return result;
    }
    let result = result.map_err(|error| format!("{name}: {error}").into());
    ctx.interaction().event(Event::Finished {
        module: name.to_owned(),
        step,
        error: result.as_ref().err().map(ToString::to_string),
    });
    if step == Step::Verify && result.is_ok() && ctx.declined() {
        ctx.interaction().event(Event::Declined {
            module: name.to_owned(),
        });
    }
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

/// Why a needed module is not in place.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Unmet {
    /// It failed earlier in this run.
    Failed,
    /// It was skipped earlier in this run.
    Skipped,
    /// A hard need that the run left out, and whose verify failed with this error.
    NotInstalled(String),
}

/// Why the runner did not install a module.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SkipReason {
    /// Each need that was not in place.
    Needs(Vec<(&'static str, Unmet)>),
    /// The module's install asked this question and nobody could answer it.
    Unanswered(String),
    /// The module changes packages, and the package database refresh failed with this error.
    PackageDatabase(String),
}

/// A module that the runner did not install, with the reason.
#[derive(Debug)]
pub struct Skipped {
    pub module: &'static str,
    pub reason: SkipReason,
}

impl std::fmt::Display for Skipped {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let unmet = match &self.reason {
            SkipReason::Needs(unmet) => unmet,
            SkipReason::Unanswered(question) => {
                return write!(f, "{}: no answer to: {question}", self.module);
            }
            SkipReason::PackageDatabase(error) => {
                return write!(
                    f,
                    "{} changes packages, and the package database refresh failed: {error}",
                    self.module
                );
            }
        };
        let reasons: Vec<String> = unmet
            .iter()
            .map(|(name, unmet)| match unmet {
                Unmet::Failed => format!("{name}, which failed"),
                Unmet::Skipped => format!("{name}, which was skipped"),
                Unmet::NotInstalled(error) => {
                    format!("{name}, which is not installed ({error})")
                }
            })
            .collect();
        write!(f, "{} needs {}", self.module, reasons.join(" and "))
    }
}

/// A module whose install or verify failed.
#[derive(Debug)]
pub struct Failed {
    pub module: &'static str,
    pub error: String,
}

/// The modules of an install run that are not in place, each failure and each skip apart.
#[derive(Debug, Default)]
pub struct InstallReport {
    pub failed: Vec<Failed>,
    pub skipped: Vec<Skipped>,
}

impl InstallReport {
    /// Whether nothing failed and every skip comes from a question that nobody could
    /// answer, directly or through a needed module skipped for that reason.
    pub fn only_unanswered(&self) -> bool {
        self.failed.is_empty()
            && self.skipped.iter().all(|skipped| match &skipped.reason {
                SkipReason::Unanswered(_) => true,
                SkipReason::PackageDatabase(_) => false,
                SkipReason::Needs(unmet) => unmet.iter().all(|(_, unmet)| *unmet == Unmet::Skipped),
            })
    }
}

#[derive(Debug)]
pub enum InstallError {
    /// The run could not start, so no module ran: the recorded state could not open a
    /// snapshot.
    NotStarted(crate::Error),
    /// Every module ran or was skipped, and these are not in place. The others are
    /// installed and verified.
    Incomplete(InstallReport),
}

impl std::fmt::Display for InstallError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotStarted(error) => write!(f, "no module was installed: {error}"),
            Self::Incomplete(report) => {
                if !report.failed.is_empty() {
                    writeln!(f, "{} module(s) failed:", report.failed.len())?;
                    for failed in &report.failed {
                        writeln!(f, "  - {}", failed.error)?;
                    }
                }
                if !report.skipped.is_empty() {
                    writeln!(f, "{} module(s) skipped:", report.skipped.len())?;
                    for skipped in &report.skipped {
                        writeln!(f, "  - {skipped}")?;
                    }
                }
                if report.only_unanswered() {
                    write!(
                        f,
                        "Run the installer again in a terminal to answer the questions above."
                    )
                } else {
                    write!(
                        f,
                        "Fix the failures above, then run the installer again. Completed steps are safe to repeat."
                    )
                }
            }
        }
    }
}

impl std::error::Error for InstallError {}

/// What became of a module that this run already reached or checked.
enum Reached {
    Installed,
    Failed,
    Skipped,
    /// Left out of the run; its verify passed, or failed with this error.
    Verified(Result<(), String>),
}

/// Installs and verifies each module in profile order, going on after a failed one. A
/// module whose need failed or was skipped is not installed, and a module whose install
/// returns `Unanswered` is skipped. `profile` holds the modules that a hard need left out
/// of `modules` is looked up in.
pub fn install(
    ctx: &Context,
    profile: &[&dyn Module],
    modules: &[&dyn Module],
) -> Result<(), InstallError> {
    ctx.state()
        .borrow_mut()
        .begin(Action::Install)
        .map_err(InstallError::NotStarted)?;
    // Installing packages right after a failed `pacman -Syu` would leave a partly upgraded
    // system, so only the modules that leave packages alone run after a failed refresh.
    let refresh_error = prepare_packages(ctx).err().map(|error| error.to_string());
    let mut reached: HashMap<&'static str, Reached> = HashMap::new();
    let mut report = InstallReport::default();
    for module in modules {
        let name = module.name();
        let mut skip = |reached: &mut HashMap<_, _>, reason| {
            let skipped = Skipped {
                module: name,
                reason,
            };
            ctx.interaction().event(Event::Skipped {
                module: name.to_owned(),
                reason: skipped.to_string(),
            });
            report.skipped.push(skipped);
            reached.insert(name, Reached::Skipped);
        };
        let unmet = unmet_needs(ctx, profile, *module, &mut reached);
        if !unmet.is_empty() {
            skip(&mut reached, SkipReason::Needs(unmet));
            continue;
        }
        if let Some(error) = &refresh_error
            && module.changes_packages(ctx)
        {
            skip(&mut reached, SkipReason::PackageDatabase(error.clone()));
            continue;
        }
        let result = match step(ctx, *module, Step::Install, || module.install(ctx)) {
            Err(error) => match error.downcast::<Unanswered>() {
                Ok(unanswered) => {
                    skip(&mut reached, SkipReason::Unanswered(unanswered.question));
                    continue;
                }
                Err(error) => Err(error),
            },
            Ok(()) => step(ctx, *module, Step::Verify, || module.verify(ctx)),
        };
        match result {
            Ok(()) => reached.insert(name, Reached::Installed),
            Err(error) => {
                report.failed.push(Failed {
                    module: name,
                    error: error.to_string(),
                });
                reached.insert(name, Reached::Failed)
            }
        };
    }
    if report.failed.is_empty() && report.skipped.is_empty() {
        Ok(())
    } else {
        Err(InstallError::Incomplete(report))
    }
}

/// The needs of `module` that are not in place. A soft need that the run has not reached
/// is fine. A hard need that the run has not reached must verify. `need_problems` keeps
/// every hard need in the profile, so one that `profile` lacks has nothing to verify.
fn unmet_needs(
    ctx: &Context,
    profile: &[&dyn Module],
    module: &dyn Module,
    reached: &mut HashMap<&'static str, Reached>,
) -> Vec<(&'static str, Unmet)> {
    let mut unmet = Vec::new();
    for (name, hard) in named_needs(profile, module) {
        if !reached.contains_key(name) {
            if !hard {
                continue;
            }
            let Some(needed) = profile.iter().find(|candidate| candidate.name() == name) else {
                continue;
            };
            ctx.set_module(name);
            let verified = needed.verify(ctx).map_err(|error| error.to_string());
            reached.insert(name, Reached::Verified(verified));
        }
        match &reached[name] {
            Reached::Installed | Reached::Verified(Ok(())) => {}
            Reached::Failed => unmet.push((name, Unmet::Failed)),
            Reached::Skipped => unmet.push((name, Unmet::Skipped)),
            Reached::Verified(Err(error)) => unmet.push((name, Unmet::NotInstalled(error.clone()))),
        }
    }
    unmet
}

/// Each module that `module` needs, by name, with whether the need is hard.
/// `Need::EveryEarlier` becomes a soft need on each module before it in `profile`.
fn named_needs(profile: &[&dyn Module], module: &dyn Module) -> Vec<(&'static str, bool)> {
    let earlier = profile
        .iter()
        .position(|candidate| candidate.name() == module.name())
        .map_or(&[][..], |index| &profile[..index]);
    let mut named = Vec::new();
    for need in module.needs() {
        match need {
            Need::Hard(name) => named.push((*name, true)),
            Need::Soft(name) => named.push((*name, false)),
            Need::EveryEarlier => {
                named.extend(earlier.iter().map(|candidate| (candidate.name(), false)));
            }
        }
    }
    named
}

/// The ways `profile` breaks the order rule of needs: every need comes earlier in the
/// list, and every hard need is in it. Each app's tests check its profile with this.
pub fn need_problems(profile: &[&dyn Module]) -> Vec<String> {
    let mut problems = Vec::new();
    for (index, module) in profile.iter().enumerate() {
        for (name, hard) in named_needs(profile, *module) {
            match profile
                .iter()
                .position(|candidate| candidate.name() == name)
            {
                Some(position) if position < index => {}
                Some(_) => problems.push(format!(
                    "{} needs {name}, which comes later in the profile",
                    module.name()
                )),
                None if hard => problems.push(format!(
                    "{} needs {name}, which the profile does not have",
                    module.name()
                )),
                None => {}
            }
        }
    }
    problems
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

/// Whether the recorded state holds changes from an install that was not removed. A
/// declined install changed nothing, so it does not count.
pub fn installed(ctx: &Context, module: &dyn Module) -> bool {
    ctx.state()
        .borrow()
        .pending(module.name())
        .iter()
        .any(|entry| !matches!(entry.change, Change::Declined))
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
                support::install_system_file(ctx, path, &Previous::contents(contents)?, "0644")?;
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
        Change::ServiceDisabled {
            unit,
            scope,
            was_enabled,
            was_active,
        } => {
            let user: &[&str] = &scope_argument(*scope);
            let (_, state) =
                ctx.read_unchecked(cmd!(sh, "systemctl {user...} is-enabled {unit}"))?;
            let systemctl = |action: &str| match scope {
                crate::ServiceScope::System => ctx.run(cmd!(sh, "sudo systemctl {action} {unit}")),
                crate::ServiceScope::User => ctx.run(cmd!(sh, "systemctl --user {action} {unit}")),
            };
            if *was_enabled && state != "enabled" {
                systemctl("enable")?;
            }
            if *was_active
                && !ctx.succeeds(cmd!(sh, "systemctl {user...} --quiet is-active {unit}"))?
            {
                systemctl("start")?;
            }
        }
        Change::UndoCommand { program, args } => ctx.run(cmd!(sh, "{program} {args...}"))?,
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
        Change::Declined | Change::Undone { .. } | Change::Removed => {}
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
