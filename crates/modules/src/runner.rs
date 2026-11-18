//! The only code that loops over modules. The command line and the screen call it.
use std::collections::{HashMap, HashSet};

use inayos_utils::resolve_package;

use crate::{
    Context, Event, Module, ModuleResult, Need, Package, Step, Unanswered,
    state::{Action, Change, Needed, Outcome, Questions},
};

/// Packages that `remove` itself runs, so removing them first would break later undos.
const INSTALLER_NEEDS: [Package; 2] = [Package::Sudo, Package::Stow];

#[derive(Default, Debug)]
pub struct RemoveReport {
    /// A question had no answer, so something was kept. The command line exits with code 3.
    pub unanswered: bool,
}

/// The question that made a failed install a skip. An install is skipped only when the
/// error it returns is `Unanswered`, so a failure after an unanswered question still
/// counts as a failure.
fn unanswered(error: &crate::Error) -> Option<String> {
    error
        .downcast_ref::<Unanswered>()
        .map(|unanswered| unanswered.question.clone())
}

fn step(
    ctx: &Context,
    name: &'static str,
    step: Step,
    action: impl FnOnce() -> ModuleResult,
) -> ModuleResult {
    ctx.set_module(name);
    ctx.interaction().event(Event::Started {
        module: name.to_owned(),
        step,
    });
    let result = action();
    // An install that nobody could answer is a skip, which the runner reports itself.
    if step == Step::Install
        && let Err(error) = &result
        && unanswered(error).is_some()
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
/// returns `Unanswered`, or fails after a question that nobody could answer, is skipped. `profile` holds the modules that a hard need left out
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
    let refresh_error = ctx
        .packages()
        .refresh(ctx)
        .err()
        .map(|error| error.to_string());
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
        let result = match step(ctx, name, Step::Install, || module.install(ctx)) {
            Err(error) => match unanswered(&error) {
                Some(question) => {
                    skip(&mut reached, SkipReason::Unanswered(question));
                    continue;
                }
                None => Err(error),
            },
            Ok(()) => step(ctx, name, Step::Verify, || module.verify(ctx)),
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
                step(ctx, module.name(), Step::Verify, || module.verify(ctx)),
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
    remove_run(ctx, profile, modules, false, force)
}

/// Removes each module in turn as `remove` does, then undoes the leftovers group of
/// myconfig, the earlier name of InayOS. No module owns that group, so it has no steps of
/// its own and no module needs it.
pub fn remove_with_leftovers(
    ctx: &Context,
    profile: &[&dyn Module],
    modules: &[&dyn Module],
    force: bool,
) -> ModuleResult<RemoveReport> {
    remove_run(ctx, profile, modules, true, force)
}

fn remove_run(
    ctx: &Context,
    profile: &[&dyn Module],
    modules: &[&dyn Module],
    leftovers: bool,
    force: bool,
) -> ModuleResult<RemoveReport> {
    ctx.state().borrow_mut().begin(Action::Remove)?;
    let removing: HashSet<_> = modules.iter().map(|module| module.name()).collect();
    // Only a module that is still installed can need something.
    let others: Vec<_> = profile
        .iter()
        .filter(|module| !removing.contains(module.name()) && installed(ctx, **module))
        .copied()
        .collect();
    let questions = RemoveQuestions { ctx, others, force };
    let mut report = RemoveReport::default();
    // A module needs only modules before it in the profile, so removing in reverse profile
    // order removes each module while what it needs is still in place.
    let position = |module: &&dyn Module| {
        profile
            .iter()
            .position(|other| other.name() == module.name())
    };
    let mut ordered = modules.to_vec();
    ordered.sort_by_key(|module| std::cmp::Reverse(position(module)));
    for module in &ordered {
        step(ctx, module.name(), Step::Remove, || {
            module.remove(ctx)?;
            undo_recorded(ctx, &questions, module.name(), &mut report)
        })?;
    }
    // The leftovers come from the earliest installs, so they go after every module.
    if leftovers {
        step(ctx, crate::leftovers::GROUP, Step::Remove, || {
            undo_recorded(ctx, &questions, crate::leftovers::GROUP, &mut report)
        })?;
    }
    Ok(report)
}

/// Undoes every change recorded for `group` since its last removal, newest first.
fn undo_recorded(
    ctx: &Context,
    questions: &RemoveQuestions,
    group: &str,
    report: &mut RemoveReport,
) -> ModuleResult {
    let pending = ctx.state().borrow().pending(group);
    let mut complete = true;
    for entry in pending.into_iter().rev() {
        let outcome = entry.change.undo(ctx, questions).map_err(|error| {
            format!(
                "could not undo {}: {error}. This change and the older ones are still in place; run remove again after fixing it",
                entry.change.describe()
            )
        })?;
        let undone = Change::Undone {
            snapshot: entry.snapshot,
            index: entry.index,
        };
        match outcome {
            Outcome::Done | Outcome::Kept => ctx.record(undone)?,
            Outcome::PassedTo(owner) => {
                // The new owner records the change first, so a crash in between leaves
                // it owned twice rather than not at all.
                ctx.state()
                    .borrow_mut()
                    .record(owner, entry.change.clone())?;
                ctx.record(undone)?;
            }
            Outcome::Stays => complete = false,
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

/// The questions of one remove run. `others` holds the installed modules that the run
/// does not remove.
struct RemoveQuestions<'a, 'c> {
    ctx: &'a Context<'c>,
    others: Vec<&'a dyn Module>,
    force: bool,
}

impl Questions for RemoveQuestions<'_, '_> {
    fn confirm(&self, question: &str, kept: &str) -> Outcome {
        ask(self.ctx, question, kept)
    }

    fn still_needed(&self, needed: Needed<'_>, dependents: Vec<String>) -> Option<Outcome> {
        let ctx = self.ctx;
        // Two catalog packages can share a name on one system, so packages compare by name.
        let same_package = |spec: inayos_utils::PackageSpec, package: &Package| {
            resolve_package(*package, ctx.package_system)
                .is_some_and(|candidate| candidate.name == spec.name)
        };
        let (what, mut needers) = match needed {
            Needed::Package(spec) => (
                format!("package {}", spec.name),
                INSTALLER_NEEDS
                    .iter()
                    .any(|package| same_package(spec, package))
                    .then(|| "InayOS itself, to remove the other modules".to_owned())
                    .into_iter()
                    .collect::<Vec<_>>(),
            ),
            Needed::Setting(setting) => (setting.describe(), Vec::new()),
        };
        let modules: Vec<&'static str> = self
            .others
            .iter()
            .filter(|module| {
                let footprint = module.footprint(ctx);
                match needed {
                    Needed::Package(spec) => footprint
                        .packages
                        .iter()
                        .any(|package| same_package(spec, package)),
                    Needed::Setting(setting) => footprint.settings.contains(setting),
                }
            })
            .map(|module| module.name())
            .collect();
        needers.extend(modules.iter().map(|name| format!("module {name}")));
        let dependents: Vec<String> = dependents
            .into_iter()
            .map(|name| format!("package {name}"))
            .collect();
        needers.extend(dependents.iter().cloned());
        if needers.is_empty() || self.force {
            return None;
        }
        let also = if dependents.is_empty() {
            String::new()
        } else {
            format!(" Removing it also removes {}.", dependents.join(", "))
        };
        let question = format!(
            "{what} is still needed by {}.{also} Remove it anyway?",
            needers.join(", ")
        );
        match ask(ctx, &question, &what) {
            Outcome::Done => None,
            Outcome::Kept => Some(match modules.first() {
                Some(owner) => {
                    ctx.note(&format!(
                        "{what} now belongs to module {owner}, so removing that module removes it"
                    ));
                    Outcome::PassedTo(owner)
                }
                None => {
                    ctx.note(&format!(
                        "{what} stays recorded, so a later remove asks again"
                    ));
                    Outcome::Stays
                }
            }),
            outcome => Some(outcome),
        }
    }
}
