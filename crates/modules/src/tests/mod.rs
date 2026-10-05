//! Behavior tests through the runner, with fake system programs in a temporary home.
mod modules;
use std::{
    cell::RefCell,
    fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::{Command, ExitStatus},
};

use embedded_dotfiles::DOTFILES;
use xshell::Shell;

use crate::{
    Context, EmacsCopied, EmacsOptions, EnvironmentInventory, Event, Footprint, Interaction,
    Module, ModuleResult, Package, PackageSystem, Setting, Unanswered, Zsh, runner,
    state::StateStore,
};

/// Answers every question the same way, or not at all when `reply` is `None`.
pub(crate) struct Answers {
    reply: Option<bool>,
    pub(crate) questions: RefCell<Vec<String>>,
}

impl Answers {
    pub(crate) fn new(reply: Option<bool>) -> Self {
        Self {
            reply,
            questions: RefCell::new(Vec::new()),
        }
    }
}

impl Interaction for Answers {
    fn output(&self, _line: &str) {}
    fn note(&self, _message: &str) {}
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
    fn event(&self, _event: Event) {}
}

/// A temporary home with fake programs first on PATH. Fake `sudo` runs its command
/// directly, and fake `pacman` keeps installed packages as files under `installed/`.
pub(crate) struct Sandbox {
    pub(crate) root: PathBuf,
    pub(crate) home: PathBuf,
    pub(crate) shell: Shell,
}

impl Sandbox {
    pub(crate) fn new(name: &str) -> Self {
        let root =
            std::env::temp_dir().join(format!("myconfig-behavior-{name}-{}", std::process::id()));
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
        shell.set_var("MYCONFIG_TEST_ROOT", &root);
        shell.set_var("MYCONFIG_TEST_LOG", root.join("calls"));
        let sandbox = Self { root, home, shell };
        sandbox.fake("sudo", r#"exec "$@""#);
        // Never reach the real paru or the AUR: paru counts as installed, and installs
        // the same way as the fake pacman.
        fs::write(sandbox.root.join("installed/paru"), "").unwrap();
        sandbox.fake(
            "paru",
            r#"printf 'paru %s\n' "$*" >> "$MYCONFIG_TEST_LOG"
for name in "$@"; do case "$name" in -*) ;; *) : > "$MYCONFIG_TEST_ROOT/installed/$name" ;; esac; done"#,
        );
        sandbox.fake(
            "pacman",
            r#"printf 'pacman %s\n' "$*" >> "$MYCONFIG_TEST_LOG"
installed="$MYCONFIG_TEST_ROOT/installed"
case "$1" in
    -Q) [ "$2" = plasma-workspace ] && { printf 'plasma-workspace 6.7.4-1\n'; exit 0; }
        [ -e "$installed/$2" ] ;;
    -Si) [ -e "$MYCONFIG_TEST_ROOT/repository/$2" ] ;;
    -U) shift; for file in "$@"; do case "$file" in -*) ;; *) printf 'local %s\n' "$file" >> "$MYCONFIG_TEST_LOG" ;; esac; done ;;
    -Qi) printf 'Name            : %s\nRequired By     : None\n' "$2" ;;
    -Qqo) exit 1 ;;
    -Syu) ;;
    -S) shift; for name in "$@"; do case "$name" in -*) ;; *) : > "$installed/$name" ;; esac; done ;;
    -R) shift; for name in "$@"; do case "$name" in -*) ;; *) rm -f "$installed/$name" ;; esac; done ;;
esac"#,
        );
        sandbox
    }

    pub(crate) fn fake(&self, name: &str, script: &str) {
        let path = self.root.join("bin").join(name);
        fs::write(&path, format!("#!/bin/sh\n{script}\n")).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
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
        let ctx = Context::new(PackageSystem::Arch, &self.shell, &self.home, io, &state);
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

/// Another module in the profile that needs zsh.
struct NeedsZsh;

impl Module for NeedsZsh {
    fn name(&self) -> &'static str {
        "needs-zsh"
    }
    fn footprint(&self, _ctx: &Context) -> Footprint {
        Footprint {
            packages: vec![Package::Zsh],
            settings: Vec::new(),
        }
    }
    fn install(&self, _ctx: &Context) -> ModuleResult {
        Ok(())
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

#[test]
fn remove_undoes_exactly_what_install_added() {
    let sandbox = Sandbox::new("round-trip");
    prepare_sample(&sandbox);
    let io = Answers::new(None);
    sandbox
        .with(&io, |ctx| runner::install(ctx, &[&Sample]))
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
fn an_unanswered_still_needed_package_is_kept_for_a_later_remove() {
    let sandbox = Sandbox::new("still-needed");
    prepare_sample(&sandbox);
    let profile: &[&dyn Module] = &[&Sample, &NeedsZsh];
    sandbox
        .with(&Answers::new(None), |ctx| runner::install(ctx, profile))
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

/// Deploys the real `zsh` config package with GNU Stow.
struct ZshConfig;

impl Module for ZshConfig {
    fn name(&self) -> &'static str {
        "zsh-config"
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
        .with(&io, |ctx| runner::install(ctx, &[&ZshConfig]))
        .unwrap();
    assert_eq!(fs::read(&zshrc).unwrap(), DOTFILES.zsh._zshrc.content);
    assert_eq!(
        fs::canonicalize(&zshrc).unwrap(),
        fs::canonicalize(sandbox.home.join("dotfiles/zsh/.zshrc")).unwrap()
    );
    // A second install leaves the deployed package in place.
    sandbox
        .with(&io, |ctx| runner::install(ctx, &[&ZshConfig]))
        .unwrap();
    assert_eq!(fs::read(&zshrc).unwrap(), DOTFILES.zsh._zshrc.content);

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
    assert!(!sandbox.home.join("dotfiles").exists());
}

/// Deploys the real `kanata` config package, which shares `.config/systemd/user` with others.
struct KanataConfig;

impl Module for KanataConfig {
    fn name(&self) -> &'static str {
        "kanata-config"
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
        .with(&io, |ctx| runner::install(ctx, &[&KanataConfig]))
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
        r#"printf 'curl %s\n' "$*" >> "$MYCONFIG_TEST_LOG"
printf 'mkdir -p "$HOME/.oh-my-zsh"\n'"#,
    );
    sandbox.fake(
        "git",
        r#"printf 'git %s\n' "$*" >> "$MYCONFIG_TEST_LOG"
mkdir -p "$3""#,
    );
    sandbox.fake(
        "chsh",
        r#"printf 'chsh %s\n' "$*" >> "$MYCONFIG_TEST_LOG"
printf '%s' "$2" > "$MYCONFIG_TEST_ROOT/shell""#,
    );
    sandbox.fake(
        "getent",
        r#"shell=/bin/bash; [ -e "$MYCONFIG_TEST_ROOT/shell" ] && shell="$(cat "$MYCONFIG_TEST_ROOT/shell")"
printf 'tester:x:1000:1000::/home/tester:%s\n' "$shell""#,
    );
    sandbox.fake("id", r#"printf 'tester\n'"#);
    sandbox.fake("zsh", "exit 0");
    let zsh = Zsh {
        set_login_shell: true,
    };
    let io = Answers::new(None);
    sandbox
        .with(&io, |ctx| runner::install(ctx, &[&zsh]))
        .unwrap();
    sandbox
        .with(&io, |ctx| runner::install(ctx, &[&zsh]))
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
    assert!(!sandbox.installed("zsh"));
}

#[test]
fn the_windows_emacs_copy_keeps_a_loader_you_wrote() {
    let sandbox = Sandbox::new("emacs");
    sandbox.fake("emacs.exe", r#"printf '%s\n' "$HOME""#);
    let emacs = EmacsCopied(EmacsOptions {
        browser_terminal_firewall: false,
    });
    let io = Answers::new(None);
    sandbox
        .with(&io, |ctx| runner::install(ctx, &[&emacs]))
        .unwrap();
    assert_eq!(
        fs::read(sandbox.home.join(".config/emacs/init.el")).unwrap(),
        DOTFILES.emacs._config.emacs.init_el.content
    );
    assert!(
        fs::read_to_string(sandbox.home.join(".emacs"))
            .unwrap()
            .contains(".config/emacs/init.el")
    );
    fs::write(sandbox.home.join(".emacs"), "user loader\n").unwrap();
    sandbox
        .with(&io, |ctx| runner::install(ctx, &[&emacs]))
        .unwrap();
    assert_eq!(
        fs::read_to_string(sandbox.home.join(".emacs")).unwrap(),
        "user loader\n"
    );
}

#[test]
fn the_environment_inventory_keeps_an_existing_file() {
    let sandbox = Sandbox::new("inventory");
    let io = Answers::new(None);
    sandbox
        .with(&io, |ctx| {
            runner::install(ctx, &[&EnvironmentInventory::ARCH_WSL])
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
            runner::install(ctx, &[&EnvironmentInventory::CACHYOS])
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
            // The elevated shared desktop step runs in its own console window.
            let stdin = line.contains("stdin().read_line") && name != "shared_desktop.rs";
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
