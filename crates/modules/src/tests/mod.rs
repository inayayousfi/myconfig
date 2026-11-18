//! Behavior tests through the runner, with fake system programs in a temporary home.
mod modules;
use std::{
    cell::RefCell,
    fs,
    path::{Path, PathBuf},
    process::{Command, ExitStatus},
};

use embedded_dotfiles::ZSH;
use xshell::Shell;

use crate::{
    Context, EnvironmentInventory, Event, Footprint, Interaction, Module, ModuleResult, Need,
    Package, PackageSystem, Setting, Unanswered, Zsh, runner, state::StateStore,
};

/// Answers every question the same way, or not at all when `reply` is `None`.
pub(crate) struct Answers {
    reply: Option<bool>,
    pub(crate) questions: RefCell<Vec<String>>,
    pub(crate) notes: RefCell<Vec<String>>,
    pub(crate) events: RefCell<Vec<Event>>,
}

impl Answers {
    pub(crate) fn new(reply: Option<bool>) -> Self {
        Self {
            reply,
            questions: RefCell::new(Vec::new()),
            notes: RefCell::new(Vec::new()),
            events: RefCell::new(Vec::new()),
        }
    }
}

impl Interaction for Answers {
    fn output(&self, _line: &str) {}
    fn note(&self, message: &str) {
        self.notes.borrow_mut().push(message.to_owned());
    }
    fn confirm(&self, question: &str) -> Result<bool, Unanswered> {
        self.questions.borrow_mut().push(question.to_owned());
        self.reply.ok_or_else(|| Unanswered {
            question: question.to_owned(),
        })
    }
    fn has_terminal(&self) -> bool {
        false
    }
    fn with_terminal(&self, command: &mut Command) -> std::io::Result<ExitStatus> {
        command.status()
    }
    fn event(&self, event: Event) {
        self.events.borrow_mut().push(event);
    }
}

/// A temporary home with fake programs first on PATH, for one package system. Fake
/// `sudo` runs its command directly. The fake package programs, `pacman` and `paru` on
/// Arch or `apt-get` and `dpkg-query` on APT, keep installed packages as files under
/// `installed/`.
pub(crate) struct Sandbox {
    pub(crate) root: PathBuf,
    pub(crate) home: PathBuf,
    pub(crate) shell: Shell,
    package_system: PackageSystem,
}

impl Sandbox {
    pub(crate) fn new(name: &str) -> Self {
        Self::for_system(name, PackageSystem::Arch)
    }

    pub(crate) fn for_system(name: &str, package_system: PackageSystem) -> Self {
        let root =
            std::env::temp_dir().join(format!("inayos-behavior-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let home = root.join("home");
        for directory in [&home, &root.join("bin"), &root.join("installed")] {
            fs::create_dir_all(directory).unwrap();
        }
        let shell = Shell::new().unwrap();
        shell.set_var(
            "PATH",
            format!("{}:/usr/bin:/bin", root.join("bin").display()),
        );
        shell.set_var("HOME", &home);
        shell.set_var("XDG_CONFIG_HOME", home.join(".config"));
        shell.set_var("USER", "tester");
        shell.set_var("INAYOS_TEST_ROOT", &root);
        shell.set_var("INAYOS_TEST_LOG", root.join("calls"));
        let sandbox = Self {
            root,
            home,
            shell,
            package_system,
        };
        sandbox.fake("sudo", r#"exec "$@""#);
        // Never reach the real paru or the AUR: paru counts as installed, and installs
        // the same way as the fake pacman.
        fs::write(sandbox.root.join("installed/paru"), "").unwrap();
        sandbox.fake(
            "paru",
            r#"printf 'paru %s\n' "$*" >> "$INAYOS_TEST_LOG"
for name in "$@"; do case "$name" in -*) ;; *) : > "$INAYOS_TEST_ROOT/installed/$name" ;; esac; done"#,
        );
        sandbox.fake(
            "pacman",
            r#"printf 'pacman %s\n' "$*" >> "$INAYOS_TEST_LOG"
installed="$INAYOS_TEST_ROOT/installed"
case "$1" in
    -Q) [ "$2" = plasma-workspace ] && { printf 'plasma-workspace 6.7.4-1\n'; exit 0; }
        [ -e "$installed/$2" ] ;;
    -Si) [ -e "$INAYOS_TEST_ROOT/repository/$2" ] ;;
    -U) shift; for file in "$@"; do case "$file" in -*) ;; *) printf 'local %s\n' "$file" >> "$INAYOS_TEST_LOG" ;; esac; done ;;
    -Qi) printf 'Name            : %s\nRequired By     : %s\n' "$2" "$(cat "$INAYOS_TEST_ROOT/required-by/$2" 2>/dev/null || echo None)" ;;
    -Qqo) exit 1 ;;
    -Syu) ;;
    -S) shift; for name in "$@"; do case "$name" in -*) ;; *) : > "$installed/$name" ;; esac; done ;;
    -R|-Rs) shift; for name in "$@"; do case "$name" in -*) ;; *) rm -f "$installed/$name" ;; esac; done ;;
    -Rsc) shift; for name in "$@"; do case "$name" in -*) ;; *) for dependent in $(cat "$INAYOS_TEST_ROOT/required-by/$name" 2>/dev/null); do rm -f "$installed/$dependent"; done; rm -f "$installed/$name" ;; esac; done ;;
esac"#,
        );
        if package_system == PackageSystem::Apt {
            // `depends/<package>` lists the dependencies that installing it adds, which
            // `auto/` marks as installed automatically.
            sandbox.fake(
                "apt-get",
                r#"printf 'apt-get %s\n' "$*" >> "$INAYOS_TEST_LOG"
root="$INAYOS_TEST_ROOT"
case " $* " in *" --simulate autoremove "*) simulate=1 ;; esac
action="$1"; shift
if [ -n "$simulate" ]; then
    for auto in "$root"/auto/*; do
        name="${auto##*/}"
        [ -e "$root/installed/$name" ] || continue
        needed=
        for list in "$root"/depends/*; do
            [ -e "$root/installed/${list##*/}" ] && grep -Fxq "$name" "$list" && needed=1
        done
        [ -n "$needed" ] || printf 'Remv %s [1.0]\n' "$name"
    done
    exit 0
fi
for name in "$@"; do
    case "$action:$name" in
        *:-*) ;;
        install:*) : > "$root/installed/$name"
            [ -e "$root/depends/$name" ] && while read -r dependency; do
                [ -e "$root/installed/$dependency" ] && continue
                mkdir -p "$root/auto"; : > "$root/installed/$dependency"; : > "$root/auto/$dependency"
            done < "$root/depends/$name" ;;
        purge:*) rm -f "$root/installed/$name" "$root/auto/$name" ;;
    esac
done"#,
            );
            sandbox.fake(
                "dpkg-query",
                r#"for name; do :; done
if [ -e "$INAYOS_TEST_ROOT/installed/$name" ]; then printf 'install ok installed'; else
    printf 'dpkg-query: no packages found matching %s\n' "$name" >&2; exit 1; fi"#,
            );
            // No installed package depends on another, and none owns the running kernel.
            sandbox.fake("apt-cache", r#"printf '%s\nReverse Depends:\n' "$3""#);
            sandbox.fake(
                "dpkg",
                r#"[ "$1" = --print-architecture ] && echo amd64 || exit 1"#,
            );
        }
        sandbox
    }

    pub(crate) fn fake(&self, name: &str, script: &str) {
        program(&self.root.join("bin").join(name), script);
    }

    pub(crate) fn installed(&self, name: &str) -> bool {
        self.root.join("installed").join(name).exists()
    }

    pub(crate) fn calls(&self, pattern: &str) -> usize {
        fs::read_to_string(self.root.join("calls"))
            .unwrap_or_default()
            .matches(pattern)
            .count()
    }

    pub(crate) fn with<T>(&self, io: &dyn Interaction, action: impl FnOnce(&Context) -> T) -> T {
        let state = RefCell::new(StateStore::open(&self.root.join("state")).unwrap());
        let ctx = Context::new(self.package_system, &self.shell, &self.home, io, &state);
        action(&ctx)
    }

    pub(crate) fn git_config(&self, key: &str) -> Option<String> {
        self.with(&Answers::new(None), |ctx| {
            Setting::GitConfig {
                key: key.to_owned(),
            }
            .read(ctx)
            .unwrap()
        })
    }
}

/// Writes a shell script at `path` and makes it executable. A child process writes it,
/// so this process never holds the script open for writing. A program that another test
/// thread starts meanwhile would inherit such a descriptor, and running the script would
/// then fail with "Text file busy".
pub(crate) fn program(path: &Path, script: &str) {
    let status = Command::new("/bin/sh")
        .args(["-c", r#"printf '%s\n' "$1" > "$2" && chmod 755 "$2""#, "sh"])
        .arg(format!("#!/bin/sh\n{script}"))
        .arg(path)
        .status()
        .unwrap();
    assert!(status.success(), "could not write {}", path.display());
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

/// Installs a missing package, keeps an existing one, and changes a setting and a file.
struct Sample;

impl Module for Sample {
    fn name(&self) -> &'static str {
        "sample"
    }

    fn needs(&self) -> &'static [Need] {
        &[]
    }
    fn footprint(&self, _ctx: &Context) -> Footprint {
        Footprint {
            packages: vec![Package::Zsh, Package::Git],
            settings: Vec::new(),
        }
    }
    fn install(&self, ctx: &Context) -> ModuleResult {
        ctx.install_packages(&[Package::Zsh, Package::Git])?;
        ctx.set(
            Setting::GitConfig {
                key: "core.symlinks".into(),
            },
            "true",
        )?;
        ctx.write_file(&ctx.home.join(".config/sample/settings"), b"new\n", false)
    }
    fn verify(&self, _ctx: &Context) -> ModuleResult {
        Ok(())
    }
    fn remove(&self, _ctx: &Context) -> ModuleResult {
        Ok(())
    }
}

/// Another module in the profile that needs zsh, and records a file of its own.
struct NeedsZsh;

impl Module for NeedsZsh {
    fn name(&self) -> &'static str {
        "needs-zsh"
    }

    fn needs(&self) -> &'static [Need] {
        &[]
    }
    fn footprint(&self, _ctx: &Context) -> Footprint {
        Footprint {
            packages: vec![Package::Zsh],
            settings: Vec::new(),
        }
    }
    fn install(&self, ctx: &Context) -> ModuleResult {
        ctx.write_file(&ctx.home.join(".config/needs-zsh"), b"on\n", false)
    }
    fn verify(&self, _ctx: &Context) -> ModuleResult {
        Ok(())
    }
    fn remove(&self, _ctx: &Context) -> ModuleResult {
        Ok(())
    }
}

fn prepare_sample(sandbox: &Sandbox) {
    fs::write(sandbox.root.join("installed/git"), "").unwrap();
    fs::create_dir_all(sandbox.home.join(".config/sample")).unwrap();
    fs::write(sandbox.home.join(".config/sample/settings"), "old\n").unwrap();
}

/// A module that notes its name when it is removed.
struct NotesRemoval(&'static str, &'static [Need]);

impl Module for NotesRemoval {
    fn name(&self) -> &'static str {
        self.0
    }
    fn needs(&self) -> &'static [Need] {
        self.1
    }
    fn footprint(&self, _ctx: &Context) -> Footprint {
        Footprint::default()
    }
    fn install(&self, _ctx: &Context) -> ModuleResult {
        Ok(())
    }
    fn verify(&self, _ctx: &Context) -> ModuleResult {
        Ok(())
    }
    fn remove(&self, ctx: &Context) -> ModuleResult {
        ctx.note(self.0);
        Ok(())
    }
}

#[test]
fn remove_finds_claude_in_its_install_folder_when_path_lacks_it() {
    // The sandbox PATH holds only its fake programs and the system folders, like the SSH
    // session the VM cycle runs remove from. Claude Code installs itself in ~/.local/bin.
    let sandbox = Sandbox::new("claude-mcp-undo");
    let bin = sandbox.home.join(".local/bin");
    fs::create_dir_all(&bin).unwrap();
    program(
        &bin.join("claude"),
        r#"printf 'claude %s\n' "$*" >> "$INAYOS_TEST_LOG"
case "$2" in
    add-json) printf '{"mcpServers":{"%s":%s}}' "$5" "$6" > "$HOME/.claude.json" ;;
    remove) printf '{}' > "$HOME/.claude.json" ;;
esac"#,
    );
    let setting = || Setting::ClaudeMcpServer {
        name: "demo".to_owned(),
    };
    let io = Answers::new(None);
    sandbox.with(&io, |ctx| {
        ctx.state()
            .borrow_mut()
            .begin(crate::state::Action::Install)
            .unwrap();
        ctx.set_module("agent-config");
        ctx.set(setting(), r#"{"command":"demo"}"#).unwrap();
    });
    let module = NotesRemoval("agent-config", &[]);
    let profile: [&dyn Module; 1] = [&module];
    sandbox
        .with(&io, |ctx| runner::remove(ctx, &profile, &profile, false))
        .unwrap();
    assert_eq!(sandbox.calls("claude mcp remove --scope user demo"), 1);
    assert_eq!(sandbox.with(&io, |ctx| setting().read(ctx).unwrap()), None);
}

#[test]
fn remove_takes_a_module_away_before_the_modules_it_needs() {
    // runtimes undoes its Rust toolchain setting with rustup, which base installed, so base
    // must still be there when runtimes goes.
    let sandbox = Sandbox::new("remove-order");
    let base = NotesRemoval("base", &[]);
    let runtimes = NotesRemoval("runtimes", &[Need::Hard("base")]);
    let profile: [&dyn Module; 2] = [&base, &runtimes];
    let io = Answers::new(None);
    sandbox
        .with(&io, |ctx| runner::remove(ctx, &profile, &profile, false))
        .unwrap();
    assert_eq!(*io.notes.borrow(), ["runtimes", "base"]);
}

#[test]
fn remove_undoes_exactly_what_install_added() {
    let sandbox = Sandbox::new("round-trip");
    prepare_sample(&sandbox);
    let io = Answers::new(None);
    sandbox
        .with(&io, |ctx| runner::install(ctx, &[&Sample], &[&Sample]))
        .unwrap();
    assert!(sandbox.installed("zsh"));
    assert_eq!(sandbox.git_config("core.symlinks").as_deref(), Some("true"));
    assert_eq!(
        fs::read_to_string(sandbox.home.join(".config/sample/settings")).unwrap(),
        "new\n"
    );

    let report = sandbox
        .with(&io, |ctx| {
            runner::remove(ctx, &[&Sample], &[&Sample], false)
        })
        .unwrap();
    assert!(!report.unanswered);
    assert!(io.questions.borrow().is_empty());
    assert!(
        !sandbox.installed("zsh"),
        "the package install added is uninstalled"
    );
    assert!(
        sandbox.installed("git"),
        "a package that was already installed stays"
    );
    assert_eq!(sandbox.git_config("core.symlinks"), None);
    assert_eq!(
        fs::read_to_string(sandbox.home.join(".config/sample/settings")).unwrap(),
        "old\n"
    );
    assert!(sandbox.with(&io, |ctx| !runner::installed(ctx, &Sample)));
}

#[test]
fn removing_a_package_anyway_also_removes_the_packages_that_depend_on_it() {
    let sandbox = Sandbox::new("remove-with-dependents");
    prepare_sample(&sandbox);
    let io = Answers::new(Some(true));
    sandbox
        .with(&io, |ctx| runner::install(ctx, &[&Sample], &[&Sample]))
        .unwrap();
    // A package installed later, by hand, depends on zsh.
    fs::write(sandbox.root.join("installed/zsh-theme"), "").unwrap();
    fs::create_dir_all(sandbox.root.join("required-by")).unwrap();
    fs::write(sandbox.root.join("required-by/zsh"), "zsh-theme").unwrap();
    sandbox
        .with(&io, |ctx| {
            runner::remove(ctx, &[&Sample], &[&Sample], false)
        })
        .unwrap();
    assert!(io.questions.borrow().iter().any(|question| question
        == "package zsh is still needed by package zsh-theme. Removing it also removes package zsh-theme. Remove it anyway?"));
    assert_eq!(sandbox.calls("pacman -Rsc --noconfirm zsh\n"), 1);
    assert!(!sandbox.installed("zsh") && !sandbox.installed("zsh-theme"));
}

#[test]
fn remove_uninstalls_through_apt_only_what_install_added() {
    let sandbox = Sandbox::for_system("round-trip-apt", PackageSystem::Apt);
    prepare_sample(&sandbox);
    // zsh brings zsh-common, and old-lib was already unneeded before install.
    fs::create_dir_all(sandbox.root.join("depends")).unwrap();
    fs::write(sandbox.root.join("depends/zsh"), "zsh-common\n").unwrap();
    fs::create_dir_all(sandbox.root.join("auto")).unwrap();
    for marker in ["installed/old-lib", "auto/old-lib"] {
        fs::write(sandbox.root.join(marker), "").unwrap();
    }
    let io = Answers::new(None);
    sandbox
        .with(&io, |ctx| runner::install(ctx, &[&Sample], &[&Sample]))
        .unwrap();
    assert_eq!(sandbox.calls("apt-get install -y zsh\n"), 1);
    assert!(sandbox.installed("zsh") && sandbox.installed("zsh-common"));

    sandbox
        .with(&io, |ctx| {
            runner::remove(ctx, &[&Sample], &[&Sample], false)
        })
        .unwrap();
    assert!(io.questions.borrow().is_empty());
    assert_eq!(sandbox.calls("apt-get purge -y zsh\n"), 1);
    assert!(!sandbox.installed("zsh"));
    assert!(
        !sandbox.installed("zsh-common"),
        "the dependency that only zsh needed stays"
    );
    assert!(
        sandbox.installed("old-lib"),
        "a package that was unneeded before install was removed"
    );
    assert!(
        sandbox.installed("git"),
        "a package that was already installed stays"
    );
}

#[test]
fn an_unanswered_still_needed_package_is_kept_for_a_later_remove() {
    let sandbox = Sandbox::new("still-needed");
    prepare_sample(&sandbox);
    let profile: &[&dyn Module] = &[&Sample, &NeedsZsh];
    sandbox
        .with(&Answers::new(None), |ctx| {
            runner::install(ctx, profile, profile)
        })
        .unwrap();

    let unattended = Answers::new(None);
    let report = sandbox
        .with(&unattended, |ctx| {
            runner::remove(ctx, profile, &[&Sample], false)
        })
        .unwrap();
    assert!(
        report.unanswered,
        "the command line turns this into exit code 3"
    );
    assert!(unattended.questions.borrow()[0].contains("module needs-zsh"));
    assert!(sandbox.installed("zsh"));
    assert_eq!(
        sandbox.git_config("core.symlinks"),
        None,
        "the rest was still undone"
    );
    assert!(sandbox.with(&unattended, |ctx| runner::installed(ctx, &Sample)));

    let answered = Answers::new(Some(true));
    let report = sandbox
        .with(&answered, |ctx| {
            runner::remove(ctx, profile, &[&Sample], false)
        })
        .unwrap();
    assert!(!report.unanswered);
    assert!(!sandbox.installed("zsh"));
    assert!(sandbox.with(&answered, |ctx| !runner::installed(ctx, &Sample)));
}

#[test]
fn a_kept_package_passes_to_the_module_that_still_needs_it() {
    let sandbox = Sandbox::new("kept-passes");
    prepare_sample(&sandbox);
    let profile: &[&dyn Module] = &[&Sample, &NeedsZsh];
    sandbox
        .with(&Answers::new(None), |ctx| {
            runner::install(ctx, profile, profile)
        })
        .unwrap();

    let keep = Answers::new(Some(false));
    let report = sandbox
        .with(&keep, |ctx| runner::remove(ctx, profile, &[&Sample], false))
        .unwrap();
    assert!(!report.unanswered);
    assert!(keep.questions.borrow()[0].contains("module needs-zsh"));
    assert!(sandbox.installed("zsh"));
    assert!(sandbox.with(&keep, |ctx| !runner::installed(ctx, &Sample)));

    // needs-zsh now owns zsh, so removing it as well leaves the machine as it was.
    let unattended = Answers::new(None);
    let report = sandbox
        .with(&unattended, |ctx| {
            runner::remove(ctx, profile, &[&NeedsZsh], false)
        })
        .unwrap();
    assert!(!report.unanswered);
    assert!(unattended.questions.borrow().is_empty());
    assert!(!sandbox.installed("zsh"));
    assert!(sandbox.installed("git"));
}

#[test]
fn a_module_that_is_not_installed_does_not_count_as_needing_a_package() {
    let sandbox = Sandbox::new("needer-not-installed");
    prepare_sample(&sandbox);
    let profile: &[&dyn Module] = &[&Sample, &NeedsZsh];
    let io = Answers::new(None);
    sandbox
        .with(&io, |ctx| runner::install(ctx, profile, &[&Sample]))
        .unwrap();
    let report = sandbox
        .with(&io, |ctx| runner::remove(ctx, profile, &[&Sample], false))
        .unwrap();
    assert!(!report.unanswered);
    assert!(io.questions.borrow().is_empty());
    assert!(!sandbox.installed("zsh"));
}

/// Installs an official package and an AUR package in one call.
struct OfficialAndAur;

impl Module for OfficialAndAur {
    fn name(&self) -> &'static str {
        "official-and-aur"
    }

    fn needs(&self) -> &'static [Need] {
        &[]
    }
    fn footprint(&self, _ctx: &Context) -> Footprint {
        Footprint {
            packages: vec![Package::Zsh, Package::Kanata],
            settings: Vec::new(),
        }
    }
    fn install(&self, ctx: &Context) -> ModuleResult {
        ctx.install_packages(&[Package::Zsh, Package::Kanata])
    }
    fn verify(&self, _ctx: &Context) -> ModuleResult {
        Ok(())
    }
    fn remove(&self, _ctx: &Context) -> ModuleResult {
        Ok(())
    }
}

#[test]
fn remove_uninstalls_what_a_failed_install_left_behind() {
    let sandbox = Sandbox::new("partial-install");
    sandbox.fake("paru", "exit 1");
    let io = Answers::new(None);
    let error = sandbox
        .with(&io, |ctx| {
            runner::install(ctx, &[&OfficialAndAur], &[&OfficialAndAur])
        })
        .unwrap_err();
    assert!(error.to_string().contains("paru -S"), "{error}");
    assert!(
        sandbox.installed("zsh"),
        "pacman installed it before paru failed"
    );

    sandbox
        .with(&io, |ctx| {
            runner::remove(ctx, &[&OfficialAndAur], &[&OfficialAndAur], false)
        })
        .unwrap();
    assert!(!sandbox.installed("zsh"));
}

/// Sets one Git option to its own value.
struct SetsOption(&'static str, &'static str);

impl Module for SetsOption {
    fn name(&self) -> &'static str {
        self.0
    }

    fn needs(&self) -> &'static [Need] {
        &[]
    }
    fn footprint(&self, _ctx: &Context) -> Footprint {
        Footprint::default()
    }
    fn install(&self, ctx: &Context) -> ModuleResult {
        ctx.set(
            Setting::GitConfig {
                key: "core.editor".into(),
            },
            self.1,
        )
    }
    fn verify(&self, _ctx: &Context) -> ModuleResult {
        Ok(())
    }
    fn remove(&self, _ctx: &Context) -> ModuleResult {
        Ok(())
    }
}

#[test]
fn each_module_records_its_own_change_to_a_setting_in_one_run() {
    let sandbox = Sandbox::new("setting-per-module");
    let first = SetsOption("first", "vim");
    let second = SetsOption("second", "nano");
    let profile: &[&dyn Module] = &[&first, &second];
    let io = Answers::new(None);
    sandbox
        .with(&io, |ctx| runner::install(ctx, profile, profile))
        .unwrap();
    assert_eq!(sandbox.git_config("core.editor").as_deref(), Some("nano"));

    sandbox
        .with(&io, |ctx| runner::remove(ctx, profile, &[&second], false))
        .unwrap();
    assert_eq!(sandbox.git_config("core.editor").as_deref(), Some("vim"));
    sandbox
        .with(&io, |ctx| runner::remove(ctx, profile, &[&first], false))
        .unwrap();
    assert_eq!(sandbox.git_config("core.editor"), None);
}

/// Installs Python and the Python venv module, which Arch ships in one package.
struct PythonWithVenv;

impl Module for PythonWithVenv {
    fn name(&self) -> &'static str {
        "python-with-venv"
    }

    fn needs(&self) -> &'static [Need] {
        &[]
    }
    fn footprint(&self, _ctx: &Context) -> Footprint {
        Footprint {
            packages: vec![Package::Python, Package::PythonVenv],
            settings: Vec::new(),
        }
    }
    fn install(&self, ctx: &Context) -> ModuleResult {
        ctx.install_packages(&[Package::Python, Package::PythonVenv])
    }
    fn verify(&self, _ctx: &Context) -> ModuleResult {
        Ok(())
    }
    fn remove(&self, _ctx: &Context) -> ModuleResult {
        Ok(())
    }
}

/// Needs Python only, and records a file of its own.
struct NeedsPython;

impl Module for NeedsPython {
    fn name(&self) -> &'static str {
        "needs-python"
    }

    fn needs(&self) -> &'static [Need] {
        &[]
    }
    fn footprint(&self, _ctx: &Context) -> Footprint {
        Footprint {
            packages: vec![Package::Python],
            settings: Vec::new(),
        }
    }
    fn install(&self, ctx: &Context) -> ModuleResult {
        ctx.write_file(&ctx.home.join(".config/needs-python"), b"on\n", false)
    }
    fn verify(&self, _ctx: &Context) -> ModuleResult {
        Ok(())
    }
    fn remove(&self, _ctx: &Context) -> ModuleResult {
        Ok(())
    }
}

#[test]
fn a_package_needed_under_another_catalog_name_is_not_uninstalled_without_asking() {
    let sandbox = Sandbox::new("shared-package-name");
    let profile: &[&dyn Module] = &[&PythonWithVenv, &NeedsPython];
    let io = Answers::new(None);
    sandbox
        .with(&io, |ctx| runner::install(ctx, profile, profile))
        .unwrap();
    assert!(sandbox.installed("python"));

    let report = sandbox
        .with(&io, |ctx| {
            runner::remove(ctx, profile, &[&PythonWithVenv], false)
        })
        .unwrap();
    assert!(report.unanswered);
    assert!(io.questions.borrow()[0].contains("module needs-python"));
    assert!(sandbox.installed("python"));
}

/// Deploys the real `zsh` config package with GNU Stow.
struct ZshConfig;

impl Module for ZshConfig {
    fn name(&self) -> &'static str {
        "zsh-config"
    }

    fn needs(&self) -> &'static [Need] {
        &[]
    }
    fn footprint(&self, _ctx: &Context) -> Footprint {
        Footprint::default()
    }
    fn install(&self, ctx: &Context) -> ModuleResult {
        ctx.deploy_config("zsh")
    }
    fn verify(&self, ctx: &Context) -> ModuleResult {
        crate::deploy::verify_package(ctx, "zsh")
    }
    fn remove(&self, _ctx: &Context) -> ModuleResult {
        Ok(())
    }
}

#[test]
fn a_deployed_config_package_replaces_and_later_restores_your_file() {
    let sandbox = Sandbox::new("stow");
    let zshrc = sandbox.home.join(".zshrc");
    fs::write(&zshrc, "previous user config\n").unwrap();
    let io = Answers::new(None);
    sandbox
        .with(&io, |ctx| {
            runner::install(ctx, &[&ZshConfig], &[&ZshConfig])
        })
        .unwrap();
    assert_eq!(fs::read(&zshrc).unwrap(), ZSH._zshrc.content);
    assert_eq!(
        fs::canonicalize(&zshrc).unwrap(),
        fs::canonicalize(sandbox.home.join("dotfiles/zsh/.zshrc")).unwrap()
    );
    let backups = || -> Vec<PathBuf> {
        let mut backups: Vec<_> = fs::read_dir(&sandbox.home)
            .unwrap()
            .map(|entry| entry.unwrap().path().join("zsh"))
            .filter(|package| {
                package
                    .parent()
                    .and_then(Path::file_name)
                    .is_some_and(|name| name.to_string_lossy().starts_with("dotfiles.backup."))
            })
            .collect();
        backups.sort();
        backups
    };
    let reinstall = || {
        sandbox
            .with(&io, |ctx| {
                runner::install(ctx, &[&ZshConfig], &[&ZshConfig])
            })
            .unwrap();
    };
    // A package that holds only its own files is replaced without a backup.
    reinstall();
    assert!(backups().is_empty());
    // A program writes through a link into the deployed package, so reinstalling keeps
    // the replaced package in a backup folder.
    fs::write(sandbox.home.join("dotfiles/zsh/written"), "program data\n").unwrap();
    reinstall();
    assert_eq!(fs::read(&zshrc).unwrap(), ZSH._zshrc.content);
    assert_eq!(backups().len(), 1);
    assert_eq!(
        fs::read_to_string(backups()[0].join("written")).unwrap(),
        "program data\n"
    );

    sandbox
        .with(&io, |ctx| {
            runner::remove(ctx, &[&ZshConfig], &[&ZshConfig], false)
        })
        .unwrap();
    assert!(
        !fs::symlink_metadata(&zshrc)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert_eq!(
        fs::read_to_string(&zshrc).unwrap(),
        "previous user config\n"
    );
    // The reinstalled package holds only its own files, so remove deletes it.
    assert!(!sandbox.home.join("dotfiles").exists());
    assert_eq!(backups().len(), 1);
}

/// Deploys the real `kanata` config package, which shares `.config/systemd/user` with others.
struct KanataConfig;

impl Module for KanataConfig {
    fn name(&self) -> &'static str {
        "kanata-config"
    }

    fn needs(&self) -> &'static [Need] {
        &[]
    }
    fn footprint(&self, _ctx: &Context) -> Footprint {
        Footprint::default()
    }
    fn install(&self, ctx: &Context) -> ModuleResult {
        ctx.deploy_config("kanata")
    }
    fn verify(&self, ctx: &Context) -> ModuleResult {
        crate::deploy::verify_package(ctx, "kanata")
    }
    fn remove(&self, _ctx: &Context) -> ModuleResult {
        Ok(())
    }
}

#[test]
fn a_folder_shared_by_config_packages_stays_a_real_folder() {
    assert!(crate::deploy::shared_directories().contains(Path::new(".config/systemd/user")));
    let sandbox = Sandbox::new("shared-folders");
    let io = Answers::new(None);
    sandbox
        .with(&io, |ctx| {
            runner::install(ctx, &[&KanataConfig], &[&KanataConfig])
        })
        .unwrap();
    // A service enabled later writes here, so it must not be a link into ~/dotfiles/kanata.
    for folder in [".config", ".config/systemd", ".config/systemd/user"] {
        let metadata = fs::symlink_metadata(sandbox.home.join(folder)).unwrap();
        assert!(
            metadata.is_dir() && !metadata.file_type().is_symlink(),
            "{folder} was folded"
        );
    }
    sandbox
        .with(&io, |ctx| {
            runner::remove(ctx, &[&KanataConfig], &[&KanataConfig], false)
        })
        .unwrap();
    assert!(!sandbox.home.join(".config").exists());
}

#[test]
fn zsh_installs_once_and_sets_the_login_shell_once() {
    let sandbox = Sandbox::new("zsh");
    sandbox.fake(
        "curl",
        r#"printf 'curl %s\n' "$*" >> "$INAYOS_TEST_LOG"
printf 'mkdir -p "$HOME/.oh-my-zsh"\n'
# Like the Oh My Zsh installer, write a template ~/.zshrc when there is none.
printf '[ -e "$HOME/.zshrc" ] || echo "source \\$ZSH/oh-my-zsh.sh" > "$HOME/.zshrc"\n'"#,
    );
    sandbox.fake(
        "git",
        r#"printf 'git %s\n' "$*" >> "$INAYOS_TEST_LOG"
mkdir -p "$3""#,
    );
    sandbox.fake(
        "chsh",
        r#"printf 'chsh %s\n' "$*" >> "$INAYOS_TEST_LOG"
printf '%s' "$2" > "$INAYOS_TEST_ROOT/shell""#,
    );
    sandbox.fake(
        "getent",
        r#"shell=/bin/bash; [ -e "$INAYOS_TEST_ROOT/shell" ] && shell="$(cat "$INAYOS_TEST_ROOT/shell")"
printf 'tester:x:1000:1000::/home/tester:%s\n' "$shell""#,
    );
    sandbox.fake("id", r#"printf 'tester\n'"#);
    sandbox.fake("zsh", "exit 0");
    let zsh = Zsh {
        set_login_shell: true,
    };
    let io = Answers::new(None);
    sandbox
        .with(&io, |ctx| runner::install(ctx, &[&zsh], &[&zsh]))
        .unwrap();
    sandbox
        .with(&io, |ctx| runner::install(ctx, &[&zsh], &[&zsh]))
        .unwrap();
    assert_eq!(sandbox.calls("pacman -S --needed --noconfirm zsh"), 1);
    assert_eq!(sandbox.calls("curl -fsSL"), 1);
    assert_eq!(sandbox.calls("git clone"), 2);
    assert_eq!(sandbox.calls("chsh -s"), 1);

    sandbox
        .with(&io, |ctx| runner::remove(ctx, &[&zsh], &[&zsh], false))
        .unwrap();
    assert_eq!(
        fs::read_to_string(sandbox.root.join("shell")).unwrap(),
        "/bin/bash"
    );
    assert!(!sandbox.home.join(".oh-my-zsh").exists());
    // There was no ~/.zshrc before install, so none may source the deleted Oh My Zsh.
    assert!(fs::symlink_metadata(sandbox.home.join(".zshrc")).is_err());
    assert!(!sandbox.installed("zsh"));
}

#[test]
fn the_environment_inventory_keeps_an_existing_file() {
    let sandbox = Sandbox::new("inventory");
    let io = Answers::new(None);
    sandbox
        .with(&io, |ctx| {
            runner::install(
                ctx,
                &[&EnvironmentInventory::ARCH_WSL],
                &[&EnvironmentInventory::ARCH_WSL],
            )
        })
        .unwrap();
    let inventory = fs::read_to_string(sandbox.home.join("environment.md")).unwrap();
    assert!(inventory.starts_with(
        "# Environment\n\nThis file describes the capabilities installed for the Arch WSL development environment.\n"
    ));
    fs::write(
        sandbox.home.join("environment.md"),
        "Maintained by an agent.\n",
    )
    .unwrap();
    sandbox
        .with(&io, |ctx| {
            runner::install(
                ctx,
                &[&EnvironmentInventory::CACHYOS],
                &[&EnvironmentInventory::CACHYOS],
            )
        })
        .unwrap();
    assert_eq!(
        fs::read_to_string(sandbox.home.join("environment.md")).unwrap(),
        "Maintained by an agent.\n"
    );
}

/// Modules must go through `Context`, or their output breaks the screen and their
/// changes escape the recorded state.
#[test]
fn modules_run_commands_and_ask_questions_only_through_context() {
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut offenders = Vec::new();
    let mut files = vec![source];
    while let Some(path) = files.pop() {
        if path.is_dir() {
            files.extend(
                fs::read_dir(&path)
                    .unwrap()
                    .map(|entry| entry.unwrap().path()),
            );
            continue;
        }
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        // `Context` itself runs the commands, and the tests hold the patterns and fakes.
        if name == "context.rs" || path.components().any(|part| part.as_os_str() == "tests") {
            continue;
        }
        let text = fs::read_to_string(&path).unwrap();
        for (number, line) in text.lines().enumerate() {
            let forbidden = [
                ").run()",
                ").read()",
                ").output()",
                ".status()?",
                "/dev/tty",
            ]
            .iter()
            .any(|pattern| line.contains(pattern));
            let stdin = line.contains("stdin().read_line");
            if forbidden || stdin {
                offenders.push(format!(
                    "{}:{}: {}",
                    path.display(),
                    number + 1,
                    line.trim()
                ));
            }
        }
    }
    assert!(offenders.is_empty(), "{}", offenders.join("\n"));
}
