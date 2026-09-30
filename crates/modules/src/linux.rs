//! Contracts for the module calls in the three current Linux profiles.
use crate::{ModuleContext, ModuleResult, Profile};
use embedded_dotfiles::DOTFILES;
use myconfig_utils::{
    ExistingFilePolicy, PackageSystem, find_program, install_embedded_file,
    install_embedded_file_with_policy, install_packages, remove_arch_packages,
};
use package_catalog::Package;
use std::{
    fs,
    path::{Path, PathBuf},
};
use typed_fs_rs::EmbeddedDirectory;
use xshell::cmd;

mod agents;
pub use agents::{
    ArchWslEnvironmentInventory, CachyosEnvironmentInventory, EnvironmentInventoryModule,
};

pub trait KdePlasmaValidateModule {
    fn install(&self, context: &ModuleContext<'_>) -> ModuleResult;
}
pub struct CachyosKdePlasmaValidate;
impl KdePlasmaValidateModule for CachyosKdePlasmaValidate {
    fn install(&self, context: &ModuleContext<'_>) -> ModuleResult {
        require_cachyos(context)?;
        find_program(context.shell, "plasmashell")?;
        find_program(context.shell, "pacman")?;
        let output = cmd!(context.shell, "pacman -Q plasma-workspace").read()?;
        let version = output
            .split_whitespace()
            .nth(1)
            .ok_or("could not read the Plasma version")?;
        let no_epoch = version.rsplit(':').next().unwrap_or(version);
        let mut components = no_epoch.split('.');
        let major: u32 = components
            .next()
            .ok_or("Plasma has no major version")?
            .parse()?;
        let minor: u32 = components
            .next()
            .ok_or("Plasma has no minor version")?
            .parse()?;
        if major != 6 || minor < 7 {
            return Err(format!("KDE Plasma 6.7 through 6.x is required; found {version}").into());
        }
        Ok(())
    }
}

pub trait BaseModule {
    fn install(&self, context: &ModuleContext<'_>) -> ModuleResult;
}

pub struct CachyosBase;
pub struct ArchWslBase;
pub struct UbuntuServerBase;

impl BaseModule for CachyosBase {
    fn install(&self, context: &ModuleContext<'_>) -> ModuleResult {
        require_cachyos(context)?;
        remove_arch_packages(context.shell, &[Package::CachyUpdate])?;
        install_arch_base(context)
    }
}

impl BaseModule for ArchWslBase {
    fn install(&self, context: &ModuleContext<'_>) -> ModuleResult {
        if context.profile != Profile::ArchWsl {
            return Err("ArchWslBase requires Arch WSL".into());
        }
        install_arch_base(context)
    }
}

fn install_arch_base(context: &ModuleContext<'_>) -> ModuleResult {
    require_arch(context)?;
    install_packages(
        context.shell,
        context.package_system,
        &[
            Package::CaCertificates,
            Package::Sudo,
            Package::Git,
            Package::Curl,
            Package::Wget,
            Package::Rsync,
            Package::Stow,
            Package::Tar,
            Package::Unzip,
            Package::Zip,
            Package::Xz,
            Package::File,
            Package::ManDb,
            Package::ManPages,
            Package::BaseDevel,
            Package::Rustup,
            Package::Polkit,
        ],
    )?;
    Ok(())
}

impl BaseModule for UbuntuServerBase {
    fn install(&self, context: &ModuleContext<'_>) -> ModuleResult {
        if context.profile != Profile::UbuntuServer || context.package_system != PackageSystem::Apt
        {
            return Err("UbuntuServerBase requires the Ubuntu Server profile and APT".into());
        }
        install_packages(
            context.shell,
            context.package_system,
            &[
                Package::CaCertificates,
                Package::Curl,
                Package::Git,
                Package::Rsync,
                Package::Stow,
            ],
        )?;
        Ok(())
    }
}
pub trait CachyosModule {
    fn install(&self, context: &ModuleContext<'_>) -> ModuleResult;
}
pub struct CachyosSetup;
impl CachyosModule for CachyosSetup {
    fn install(&self, context: &ModuleContext<'_>) -> ModuleResult {
        if context.profile != Profile::Cachyos || context.package_system != PackageSystem::Arch {
            return Err("CachyosSetup requires CachyOS".into());
        }
        install_packages(
            context.shell,
            context.package_system,
            &[Package::CachyosKernelManager, Package::LinuxCachyos],
        )?;
        remove_arch_packages(
            context.shell,
            &[
                Package::Konsole,
                Package::Alacritty,
                Package::CachyosHello,
                Package::CachyosZshConfig,
                Package::Vim,
                Package::Fish,
                Package::CachyosFishConfig,
                Package::FishAutopair,
                Package::FishPurePrompt,
                Package::Fisher,
            ],
        )?;
        Ok(())
    }
}
pub trait SshModule {
    fn install(&self, context: &ModuleContext<'_>) -> ModuleResult;
}
pub struct CachyosSsh;
pub struct ArchWslSsh;
impl SshModule for CachyosSsh {
    fn install(&self, context: &ModuleContext<'_>) -> ModuleResult {
        install_ssh(context)
    }
}
impl SshModule for ArchWslSsh {
    fn install(&self, context: &ModuleContext<'_>) -> ModuleResult {
        install_ssh(context)
    }
}

fn install_ssh(context: &ModuleContext<'_>) -> ModuleResult {
    require_arch(context)?;
    if !Path::new("/run/systemd/system").is_dir() {
        return Err("systemd is not running".into());
    }
    let sh = context.shell;
    install_packages(sh, context.package_system, &[Package::Openssh])?;
    let user = cmd!(sh, "id -un").read()?;
    if user.is_empty() || user.chars().any(char::is_whitespace) {
        return Err("could not determine a safe OpenSSH user name".into());
    }
    let temporary = std::env::temp_dir().join(format!(
        "myconfig-sshd-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_nanos()
    ));
    fs::create_dir(&temporary)?;
    let result = (|| -> ModuleResult {
        let proposed = temporary.join("10-myconfig.conf");
        fs::write(
            &proposed,
            format!("ListenAddress 0.0.0.0\nListenAddress ::\nAllowUsers {user}\n"),
        )?;
        cmd!(sh, "sudo ssh-keygen -A").run()?;
        cmd!(sh, "sudo sshd -t -f {proposed}").run()?;

        let config = Path::new("/etc/ssh/sshd_config.d/10-myconfig.conf");
        let legacy = Path::new("/etc/ssh/sshd_config.d/10-local-only.conf");
        let old_config = temporary.join("previous-myconfig.conf");
        let old_legacy = temporary.join("previous-local-only.conf");
        let had_config = exists_or_link(config);
        let had_legacy = exists_or_link(legacy);
        if had_config {
            fs::copy(config, &old_config)?;
        }
        if had_legacy {
            fs::copy(legacy, &old_legacy)?;
        }

        let apply = (|| -> ModuleResult {
            cmd!(sh, "sudo install -Dm644 {proposed} {config}").run()?;
            cmd!(sh, "sudo rm -f {legacy}").run()?;
            cmd!(sh, "sudo sshd -t").run()?;
            cmd!(sh, "sudo systemctl enable sshd.service").run()?;
            cmd!(sh, "sudo systemctl restart sshd.service").run()?;
            Ok(())
        })();
        if let Err(failure) = apply {
            let restored = (|| -> ModuleResult {
                cmd!(sh, "sudo rm -f {config} {legacy}").run()?;
                if had_config {
                    cmd!(sh, "sudo install -Dm644 {old_config} {config}").run()?;
                }
                if had_legacy {
                    cmd!(sh, "sudo install -Dm644 {old_legacy} {legacy}").run()?;
                }
                cmd!(sh, "sudo sshd -t").run()?;
                cmd!(sh, "sudo systemctl restart sshd.service").run()?;
                Ok(())
            })();
            return match restored {
                Ok(()) => Err(format!("OpenSSH setup failed, previous configuration restored: {failure}").into()),
                Err(restore_error) => Err(format!("OpenSSH setup failed: {failure}; previous configuration could not be restored: {restore_error}").into()),
            };
        }
        Ok(())
    })();
    let cleanup = fs::remove_dir_all(&temporary);
    match (result, cleanup) {
        (Ok(()), Ok(())) => Ok(()),
        (Err(error), Ok(())) => Err(error),
        (Ok(()), Err(error)) => Err(error.into()),
        (Err(error), Err(cleanup)) => Err(format!(
            "{error}; could not remove {}: {cleanup}",
            temporary.display()
        )
        .into()),
    }
}
pub trait CliModule {
    fn install(&self, context: &ModuleContext<'_>) -> ModuleResult;
}
pub struct CachyosCli;
pub struct ArchWslCli;

fn install_cli(context: &ModuleContext<'_>) -> ModuleResult {
    require_arch(context)?;
    install_packages(
        context.shell,
        context.package_system,
        &[
            Package::Ripgrep,
            Package::Fd,
            Package::Fzf,
            Package::Zoxide,
            Package::Eza,
            Package::Bat,
            Package::Jq,
            Package::Fastfetch,
            Package::Btop,
            Package::Tokei,
            Package::GithubCli,
        ],
    )?;
    Ok(())
}

impl CliModule for CachyosCli {
    fn install(&self, context: &ModuleContext<'_>) -> ModuleResult {
        install_cli(context)
    }
}
impl CliModule for ArchWslCli {
    fn install(&self, context: &ModuleContext<'_>) -> ModuleResult {
        install_cli(context)
    }
}

fn require_arch(context: &ModuleContext<'_>) -> ModuleResult {
    if context.package_system != PackageSystem::Arch
        || !matches!(context.profile, Profile::Cachyos | Profile::ArchWsl)
    {
        return Err("this module requires CachyOS or Arch WSL with the Arch package system".into());
    }
    Ok(())
}

pub trait RuntimesModule {
    fn install(&self, context: &ModuleContext<'_>) -> ModuleResult;
}
pub struct CachyosRuntimes;
pub struct ArchWslRuntimes;

fn install_runtimes(context: &ModuleContext<'_>) -> ModuleResult {
    require_arch(context)?;
    install_packages(
        context.shell,
        context.package_system,
        &[
            Package::Go,
            Package::Bun,
            Package::Python,
            Package::Jdk,
            Package::Maven,
            Package::Llvm,
            Package::Make,
            Package::Cmake,
            Package::Nodejs,
            Package::Npm,
            Package::NodeGyp,
        ],
    )?;
    cmd!(context.shell, "rustup default stable").run()?;
    Ok(())
}

impl RuntimesModule for CachyosRuntimes {
    fn install(&self, context: &ModuleContext<'_>) -> ModuleResult {
        install_runtimes(context)
    }
}
impl RuntimesModule for ArchWslRuntimes {
    fn install(&self, context: &ModuleContext<'_>) -> ModuleResult {
        install_runtimes(context)
    }
}
pub trait ZshModule {
    fn install(&self, context: &ModuleContext<'_>) -> ModuleResult;
}

pub struct CachyosZsh;
pub struct ArchWslZsh;
pub struct UbuntuServerZsh;

impl ZshModule for CachyosZsh {
    fn install(&self, context: &ModuleContext<'_>) -> ModuleResult {
        require_cachyos(context)?;
        install_zsh(context)
    }
}

impl ZshModule for ArchWslZsh {
    fn install(&self, context: &ModuleContext<'_>) -> ModuleResult {
        if context.profile != Profile::ArchWsl {
            return Err("ArchWslZsh requires Arch WSL".into());
        }
        install_zsh(context)
    }
}

impl ZshModule for UbuntuServerZsh {
    fn install(&self, context: &ModuleContext<'_>) -> ModuleResult {
        if context.profile != Profile::UbuntuServer || context.package_system != PackageSystem::Apt
        {
            return Err("UbuntuServerZsh requires the Ubuntu Server profile and APT".into());
        }
        install_zsh(context)
    }
}

fn install_zsh(context: &ModuleContext<'_>) -> ModuleResult {
    let sh = context.shell;
    install_packages(sh, context.package_system, &[Package::Zsh])?;

    if !context.home.join(".oh-my-zsh").is_dir() {
        let url = "https://raw.githubusercontent.com/ohmyzsh/ohmyzsh/master/tools/install.sh";
        let script = cmd!(sh, "curl -fsSL {url}").output()?.stdout;
        cmd!(sh, "sh -s")
            .env("HOME", context.home)
            .env("RUNZSH", "no")
            .env("CHSH", "no")
            .env("KEEP_ZSHRC", "yes")
            .stdin(script)
            .run()?;
    }

    let custom = sh
        .var_os("ZSH_CUSTOM")
        .filter(|value| !value.is_empty())
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| context.home.join(".oh-my-zsh/custom"));
    let plugins = custom.join("plugins");
    sh.create_dir(&plugins)?;
    for (name, url) in [
        (
            "zsh-autosuggestions",
            "https://github.com/zsh-users/zsh-autosuggestions",
        ),
        (
            "zsh-syntax-highlighting",
            "https://github.com/zsh-users/zsh-syntax-highlighting.git",
        ),
    ] {
        let destination = plugins.join(name);
        if !destination.is_dir() {
            cmd!(sh, "git clone {url} {destination}").run()?;
        }
    }

    if context.profile != Profile::ArchWsl {
        let zsh = find_program(sh, "zsh")?;
        let user = sh.var("USER")?;
        let account = cmd!(sh, "getent passwd {user}").read()?;
        let current_shell = account
            .rsplit(':')
            .next()
            .ok_or("getent returned no shell")?;
        if current_shell != zsh.to_string_lossy() {
            cmd!(sh, "sudo chsh -s {zsh} {user}").run()?;
            let updated = cmd!(sh, "getent passwd {user}").read()?;
            if updated.rsplit(':').next() != Some(zsh.to_string_lossy().as_ref()) {
                return Err(format!("could not verify the login shell for {user}").into());
            }
        }
    }
    Ok(())
}
pub trait NeovimModule {
    fn install(&self, context: &ModuleContext<'_>) -> ModuleResult;
}
pub struct ArchWslNeovim;
impl NeovimModule for ArchWslNeovim {
    fn install(&self, context: &ModuleContext<'_>) -> ModuleResult {
        if context.profile != Profile::ArchWsl {
            return Err("Neovim is selected by Arch WSL only".into());
        }
        require_arch(context)?;
        install_packages(context.shell, context.package_system, &[Package::Neovim])?;
        Ok(())
    }
}

pub trait TerminalToolsModule {
    fn install(&self, context: &ModuleContext<'_>) -> ModuleResult;
}
pub struct CachyosTerminalTools;
pub struct ArchWslTerminalTools;

fn install_terminal_tools(context: &ModuleContext<'_>) -> ModuleResult {
    require_arch(context)?;
    install_packages(
        context.shell,
        context.package_system,
        &[
            Package::Yazi,
            Package::Ffmpeg,
            Package::SevenZip,
            Package::Poppler,
            Package::Resvg,
            Package::Imagemagick,
        ],
    )?;
    Ok(())
}

impl TerminalToolsModule for CachyosTerminalTools {
    fn install(&self, context: &ModuleContext<'_>) -> ModuleResult {
        install_terminal_tools(context)
    }
}
impl TerminalToolsModule for ArchWslTerminalTools {
    fn install(&self, context: &ModuleContext<'_>) -> ModuleResult {
        install_terminal_tools(context)
    }
}
pub trait AxidevOskModule {
    fn install(&self, context: &ModuleContext<'_>) -> ModuleResult;
}
pub struct CachyosAxidevOsk;

fn run_axidev_greeter(sh: &xshell::Shell, app: &Path, tty: &fs::File) -> ModuleResult {
    use std::process::{Command, Stdio};
    let status = Command::new("sudo")
        .arg(app)
        .args(["linux", "setup-greeter"])
        .env("PATH", sh.var_os("PATH").ok_or("PATH is unset")?)
        .stdin(Stdio::from(tty.try_clone()?))
        .stdout(Stdio::from(tty.try_clone()?))
        .stderr(Stdio::from(tty.try_clone()?))
        .status()?;
    if !status.success() {
        return Err(format!("Axidev OSK greeter setup failed: {status}").into());
    }
    Ok(())
}

impl AxidevOskModule for CachyosAxidevOsk {
    fn install(&self, context: &ModuleContext<'_>) -> ModuleResult {
        require_cachyos(context)?;
        let sh = context.shell;
        install_packages(
            sh,
            context.package_system,
            &[
                Package::Python,
                Package::Pyside6,
                Package::Qt6Wayland,
                Package::LayerShellQt,
                Package::Libinput,
                Package::Systemd,
                Package::Libxkbcommon,
            ],
        )?;
        for program in ["curl", "sudo"] {
            find_program(sh, program)?;
        }
        let tty_path = sh
            .var_os("MYCONFIG_TTY_PATH")
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("/dev/tty"));
        let tty = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(&tty_path)
            .map_err(|error| {
                format!(
                    "Axidev OSK greeter setup requires a terminal ({}): {error}",
                    tty_path.display()
                )
            })?;
        let user = cmd!(sh, "id -un").read()?;
        let lifecycle = Path::new("/usr/local/sbin/axidev-osk-install");
        let app = Path::new("/usr/local/bin/axidev-osk");
        if require_executable(lifecycle).is_ok() {
            cmd!(sh, "sudo {lifecycle} upgrade --user {user}").run()?;
        } else {
            let directory = std::env::temp_dir().join(format!(
                "myconfig-axidev-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)?
                    .as_nanos()
            ));
            fs::create_dir(&directory)?;
            let result = (|| -> ModuleResult {
                let installer = directory.join("axidev-osk-install");
                let url = "https://github.com/axide-dev/axidev-osk/releases/latest/download/axidev-osk-install";
                cmd!(
                    sh,
                    "curl --fail --location --show-error --silent --output {installer} {url}"
                )
                .run()?;
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    fs::set_permissions(&installer, fs::Permissions::from_mode(0o755))?;
                }
                cmd!(sh, "sudo {installer} install --user {user}").run()?;
                Ok(())
            })();
            let cleanup = fs::remove_dir_all(&directory);
            match (result, cleanup) {
                (Ok(()), Ok(())) => {}
                (Err(error), Ok(())) => return Err(error),
                (Ok(()), Err(error)) => return Err(error.into()),
                (Err(error), Err(cleanup)) => {
                    return Err(format!(
                        "{error}; could not remove {}: {cleanup}",
                        directory.display()
                    )
                    .into());
                }
            }
        }
        require_executable(app)?;
        cmd!(sh, "sudo {app} linux setup-permissions --user {user}").run()?;
        cmd!(sh, "{app} linux setup-autostart --user {user}").run()?;
        run_axidev_greeter(sh, app, &tty)?;
        cmd!(sh, "sudo {app} linux status-permissions --user {user}").run()?;
        cmd!(sh, "{app} linux status-autostart --user {user}").run()?;
        cmd!(sh, "sudo {app} linux status-greeter").run()?;
        Ok(())
    }
}
pub trait TailscaleModule {
    fn install(&self, context: &ModuleContext<'_>) -> ModuleResult;
}
pub struct CachyosTailscale;
pub struct ArchWslTailscale;

fn install_tailscale(context: &ModuleContext<'_>) -> ModuleResult {
    require_arch(context)?;
    install_packages(context.shell, context.package_system, &[Package::Tailscale])?;
    cmd!(context.shell, "sudo systemctl enable --now tailscaled").run()?;
    let status = cmd!(context.shell, "systemctl is-active tailscaled").read()?;
    if status != "active" {
        return Err(format!("tailscaled is not active: {status}").into());
    }
    Ok(())
}

impl TailscaleModule for CachyosTailscale {
    fn install(&self, context: &ModuleContext<'_>) -> ModuleResult {
        install_tailscale(context)
    }
}
impl TailscaleModule for ArchWslTailscale {
    fn install(&self, context: &ModuleContext<'_>) -> ModuleResult {
        install_tailscale(context)
    }
}
pub trait AgentsPackagesModule {
    fn install(&self, context: &ModuleContext<'_>) -> ModuleResult;
}
pub struct CachyosAgentsPackages;
pub struct ArchWslAgentsPackages;

fn install_agents_packages(context: &ModuleContext<'_>) -> ModuleResult {
    require_arch(context)?;
    let sh = context.shell;
    let mut packages = vec![
        Package::Opencode,
        Package::FxAgent,
        Package::Lsof,
        Package::AtSpi2Core,
        Package::Libxcomposite,
        Package::Libxdamage,
        Package::Libxrandr,
        Package::Libxkbcommon,
    ];
    if context.profile == Profile::Cachyos {
        packages.push(Package::Ydotool);
    }
    install_packages(sh, context.package_system, &packages)?;
    if context.profile == Profile::ArchWsl {
        install_packages(sh, context.package_system, &[Package::WslSshAgent])?;
    }

    let bun_install = context.home.join(".bun");
    let path = std::env::join_paths(
        [context.home.join(".local/bin"), bun_install.join("bin")]
            .into_iter()
            .chain(std::env::split_paths(
                &sh.var_os("PATH").ok_or("PATH is unset")?,
            )),
    )?;
    let _bun_install = sh.push_env("BUN_INSTALL", &bun_install);
    let _path = sh.push_env("PATH", path);
    cmd!(sh, "bun add --global @playwright/mcp@latest").run()?;

    let playwright_cli = bun_install.join("install/global/node_modules/.bin/playwright");
    require_executable(&playwright_cli)?;
    cmd!(sh, "{playwright_cli} install --only-shell chromium").run()?;
    let playwright_module = bun_install.join("install/global/node_modules/playwright");
    let script = "const { chromium } = require(process.env.MYCONFIG_PLAYWRIGHT_MODULE); const browser = await chromium.launch({ headless: true }); await browser.close();";
    cmd!(sh, "bun -e {script}")
        .env("MYCONFIG_PLAYWRIGHT_MODULE", playwright_module)
        .run()?;
    Ok(())
}

impl AgentsPackagesModule for CachyosAgentsPackages {
    fn install(&self, context: &ModuleContext<'_>) -> ModuleResult {
        require_cachyos(context)?;
        install_agents_packages(context)
    }
}
impl AgentsPackagesModule for ArchWslAgentsPackages {
    fn install(&self, context: &ModuleContext<'_>) -> ModuleResult {
        if context.profile != Profile::ArchWsl {
            return Err("ArchWslAgentsPackages requires Arch WSL".into());
        }
        install_agents_packages(context)
    }
}
pub trait DotfilesModule {
    fn install(&self, context: &ModuleContext<'_>) -> ModuleResult;
}

pub struct CachyosDotfiles;
pub struct ArchWslDotfiles;
pub struct UbuntuServerDotfiles;

impl DotfilesModule for CachyosDotfiles {
    fn install(&self, context: &ModuleContext<'_>) -> ModuleResult {
        require_cachyos(context)?;
        install_selected_dotfiles(
            context,
            &[
                "zsh",
                "yazi",
                "ai",
                "kanata",
                "kanata-kde",
                "handy",
                "kde-plasma",
                "emacs",
                "phone",
                "pipewire",
            ],
            &["ghostty", "hunk", "lazygit", "nvim", "tmux", "zed"],
        )
    }
}

impl DotfilesModule for ArchWslDotfiles {
    fn install(&self, context: &ModuleContext<'_>) -> ModuleResult {
        if context.profile != Profile::ArchWsl {
            return Err("ArchWslDotfiles requires Arch WSL".into());
        }
        install_selected_dotfiles(
            context,
            &["zsh", "yazi", "ai"],
            &["hunk", "lazygit", "nvim", "tmux"],
        )
    }
}

fn exists_or_link(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok()
}

fn backup_path(path: &Path, stamp: &str) -> PathBuf {
    let base = format!("{}.backup.{stamp}", path.display());
    let mut candidate = PathBuf::from(&base);
    let mut suffix = 1;
    while exists_or_link(&candidate) {
        candidate = PathBuf::from(format!("{base}.{suffix}"));
        suffix += 1;
    }
    candidate
}

fn backup_conflict(
    path: &Path,
    relative: &Path,
    home: &Path,
    stamp: &str,
    backup_root: &mut Option<PathBuf>,
) -> ModuleResult {
    if backup_root.is_none() {
        let base = home.join(".dotfiles-conflicts");
        let root = backup_path(&base, stamp);
        fs::create_dir(&root)?;
        *backup_root = Some(root);
    }
    let backup = backup_root.as_ref().unwrap().join(relative);
    if let Some(parent) = backup.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::rename(path, backup)?;
    Ok(())
}

impl DotfilesModule for UbuntuServerDotfiles {
    fn install(&self, context: &ModuleContext<'_>) -> ModuleResult {
        if context.profile != Profile::UbuntuServer {
            return Err("UbuntuServerDotfiles requires the Ubuntu Server profile".into());
        }
        install_selected_dotfiles(context, &["zsh"], &[])
    }
}

fn install_selected_dotfiles(
    context: &ModuleContext<'_>,
    packages: &[&str],
    retired: &[&str],
) -> ModuleResult {
    let home = context.home;
    let stamp = cmd!(context.shell, "date +%Y%m%d_%H%M%S").read()?;
    let dotfiles = home.join("dotfiles");
    let staging = home.join(format!(".dotfiles-stage.{}-{stamp}", std::process::id()));
    fs::create_dir(&staging)?;
    let result = (|| -> ModuleResult {
        let files: Vec<_> = DOTFILES
            .files()
            .into_iter()
            .filter(|file| {
                packages
                    .iter()
                    .any(|package| Path::new(file.path_from_root).starts_with(package))
            })
            .collect();
        for package in packages {
            if !files
                .iter()
                .any(|file| Path::new(file.path_from_root).starts_with(package))
            {
                return Err(format!("embedded dotfile package has no files: {package}").into());
            }
            fs::create_dir(staging.join(package))?;
        }
        for file in &files {
            let destination = staging.join(file.path_from_root);
            install_embedded_file(file, &destination)?;
            if fs::read(destination)? != file.content {
                return Err(format!("staged file differs: {}", file.path_from_root).into());
            }
        }

        if exists_or_link(&dotfiles) {
            for package in retired {
                if dotfiles.join(package).is_dir() {
                    cmd!(
                        context.shell,
                        "stow --dir {dotfiles} --target {home} --delete {package}"
                    )
                    .run()?;
                }
            }
            let backup = backup_path(&dotfiles, &stamp);
            fs::rename(&dotfiles, backup)?;
        }
        fs::rename(&staging, &dotfiles)?;

        let mut conflict_backup = None;
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
                let expected_ancestor = dotfiles
                    .join(source.components().next().ok_or("empty embedded path")?)
                    .join(ancestor.strip_prefix(home)?);
                let current = fs::canonicalize(&ancestor);
                let expected = fs::canonicalize(&expected_ancestor);
                if is_link
                    && matches!((&current, &expected), (Ok(current), Ok(expected)) if current == expected)
                {
                    continue;
                }
                if metadata.is_ok() && (is_link || !ancestor.is_dir()) {
                    let path = ancestor.strip_prefix(home)?;
                    backup_conflict(&ancestor, path, home, &stamp, &mut conflict_backup)?;
                    fs::create_dir(&ancestor)?;
                }
            }
            let target = home.join(&relative);
            if exists_or_link(&target) {
                let expected = fs::canonicalize(dotfiles.join(file.path_from_root));
                let actual = fs::canonicalize(&target);
                if let (Ok(expected), Ok(actual)) = (expected, actual)
                    && expected == actual
                {
                    continue;
                }
                backup_conflict(&target, &relative, home, &stamp, &mut conflict_backup)?;
            }
        }

        cmd!(
            context.shell,
            "stow --dir {dotfiles} --target {home} --restow {packages...}"
        )
        .run()?;
        for file in &files {
            let relative: PathBuf = Path::new(file.path_from_root)
                .components()
                .skip(1)
                .collect();
            if fs::read(home.join(relative))? != file.content {
                return Err(format!("installed file differs: {}", file.path_from_root).into());
            }
        }
        if packages.contains(&"ai") {
            link_agent_config(home)?;
        }
        Ok(())
    })();
    if exists_or_link(&staging) {
        fs::remove_dir_all(&staging)?;
    }
    result
}
pub trait AndroidPhoneModule {
    fn install(&self, context: &ModuleContext<'_>) -> ModuleResult;
}
pub struct CachyosAndroidPhone;
impl AndroidPhoneModule for CachyosAndroidPhone {
    fn install(&self, context: &ModuleContext<'_>) -> ModuleResult {
        require_cachyos(context)?;
        install_packages(
            context.shell,
            context.package_system,
            &[Package::AndroidSdkPlatformTools, Package::Scrcpy],
        )?;
        for program in ["adb", "scrcpy", "timeout"] {
            find_program(context.shell, program)?;
        }
        require_executable(&context.home.join(".local/bin/phone"))?;
        Ok(())
    }
}

fn require_cachyos(context: &ModuleContext<'_>) -> ModuleResult {
    if context.profile != Profile::Cachyos || context.package_system != PackageSystem::Arch {
        return Err("this module requires the CachyOS profile".into());
    }
    Ok(())
}

fn require_file(path: &Path) -> ModuleResult {
    if !fs::metadata(path)?.is_file() {
        return Err(format!("required file is missing: {}", path.display()).into());
    }
    Ok(())
}

fn require_executable(path: &Path) -> ModuleResult {
    require_file(path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if fs::metadata(path)?.permissions().mode() & 0o111 == 0 {
            return Err(format!("required file is not executable: {}", path.display()).into());
        }
    }
    Ok(())
}

pub trait EmacsModule {
    fn install(&self, context: &ModuleContext<'_>) -> ModuleResult;
}
pub struct CachyosEmacs;
impl EmacsModule for CachyosEmacs {
    fn install(&self, context: &ModuleContext<'_>) -> ModuleResult {
        require_cachyos(context)?;
        let sh = context.shell;
        install_packages(
            sh,
            context.package_system,
            &[
                Package::EmacsWayland,
                Package::Sshfs,
                Package::IosevkaFont,
                Package::Ufw,
            ],
        )?;
        let config = context.home.join(".config/emacs");
        require_file(&config.join("early-init.el"))?;
        require_file(&config.join("init.el"))?;
        if !config.join("lisp").is_dir() {
            return Err("Emacs Lisp modules were not installed".into());
        }

        let legacy = context.home.join(".emacs.d");
        if exists_or_link(&legacy) {
            let stamp = cmd!(sh, "date +%Y%m%d_%H%M%S").read()?;
            fs::rename(&legacy, backup_path(&legacy, &stamp))?;
        }
        let comment = "myconfig Emacs browser terminal";
        for subnet in ["10.0.0.0/8", "172.16.0.0/12", "192.168.0.0/16"] {
            cmd!(sh, "sudo ufw allow in proto tcp from {subnet} to any port 18080,18081 comment {comment}").run()?;
        }
        cmd!(sh, "sudo ufw --force enable").run()?;
        Ok(())
    }
}
pub trait CursorThemeModule {
    fn install(&self, context: &ModuleContext<'_>) -> ModuleResult;
}
pub struct CachyosCursorTheme;

fn remove_unselected_entries(
    root: &Path,
    current: &Path,
    selected: &std::collections::HashSet<PathBuf>,
) -> ModuleResult {
    for entry in fs::read_dir(current)? {
        let entry = entry?;
        let path = entry.path();
        let metadata = fs::symlink_metadata(&path)?;
        if metadata.is_dir() {
            remove_unselected_entries(root, &path, selected)?;
            if fs::read_dir(&path)?.next().is_none() {
                fs::remove_dir(&path)?;
            }
        } else {
            let relative = path.strip_prefix(root)?;
            if !selected.contains(relative) {
                fs::remove_file(&path)?;
            }
        }
    }
    Ok(())
}

impl CursorThemeModule for CachyosCursorTheme {
    fn install(&self, context: &ModuleContext<'_>) -> ModuleResult {
        require_cachyos(context)?;
        let source = Path::new("assets/cursor-theme/cursors/blacknpink-crosshair");
        let files: Vec<_> = DOTFILES
            .files()
            .into_iter()
            .filter(|file| Path::new(file.path_from_root).starts_with(source))
            .collect();
        for required in ["index.theme", "cursors/default", "cursors/crosshair"] {
            if !files.iter().any(|file| {
                Path::new(file.path_from_root).strip_prefix(source).ok()
                    == Some(Path::new(required))
            }) {
                return Err(format!("cursor theme is missing {required}").into());
            }
        }
        let destination = context.home.join(".local/share/icons/blacknpink-crosshair");
        if fs::symlink_metadata(&destination)
            .is_ok_and(|metadata| metadata.file_type().is_symlink())
        {
            return Err(format!(
                "cursor theme destination is a symbolic link: {}",
                destination.display()
            )
            .into());
        }
        fs::create_dir_all(&destination)?;
        let mut selected = std::collections::HashSet::new();
        for file in files {
            let relative = Path::new(file.path_from_root).strip_prefix(source)?;
            selected.insert(relative.to_path_buf());
            let target = destination.join(relative);
            let mut ancestor = destination.clone();
            if let Some(parent) = relative.parent() {
                for component in parent.components() {
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
            }
            if fs::symlink_metadata(&target).is_ok_and(|metadata| metadata.file_type().is_symlink())
            {
                fs::remove_file(&target)?;
            }
            install_embedded_file_with_policy(file, &target, ExistingFilePolicy::Replace)?;
        }
        remove_unselected_entries(&destination, &destination, &selected)?;
        Ok(())
    }
}
pub trait RefindModule {
    fn install(&self, context: &ModuleContext<'_>) -> ModuleResult;
}
pub struct CachyosRefind;

fn refind_config(sh: &xshell::Shell) -> ModuleResultPath {
    for candidate in [
        "/boot/EFI/refind/refind.conf",
        "/boot/efi/EFI/refind/refind.conf",
        "/efi/EFI/refind/refind.conf",
    ] {
        if cmd!(sh, "sudo test -f {candidate}")
            .quiet()
            .ignore_status()
            .output()?
            .status
            .success()
        {
            return Ok(Some(PathBuf::from(candidate)));
        }
    }
    Ok(None)
}

type ModuleResultPath = Result<Option<PathBuf>, Box<dyn std::error::Error>>;

fn generate_refind_images(sh: &xshell::Shell, directory: &Path) -> ModuleResult {
    let tool = find_program(sh, "magick").or_else(|_| find_program(sh, "convert"))?;
    let pink = "#ff4ead";
    let banner = directory.join("banner.png");
    let bottom = "rectangle 0,1076 1920,1080";
    cmd!(
        sh,
        "{tool} -size 1920x1080 xc:#000000 -fill {pink} -draw {bottom} {banner}"
    )
    .run()?;
    for (name, size, rect, stroke) in [
        (
            "selection_big.png",
            "144x144",
            "roundrectangle 2,2 142,142 12,12",
            "3",
        ),
        (
            "selection_small.png",
            "64x64",
            "roundrectangle 1,1 63,63 6,6",
            "2",
        ),
    ] {
        let output = directory.join(name);
        let transparent = "xc:none";
        let pink_fill = "rgba(255,78,173,0.16)";
        cmd!(sh, "{tool} -size {size} {transparent} -fill {pink_fill} -draw {rect} -stroke {pink} -strokewidth {stroke} -fill none -draw {rect} {output}").run()?;
    }
    Ok(())
}

fn updated_refind_config(original: &str) -> String {
    let mut new = String::new();
    let mut managed = false;
    for line in original.lines() {
        match line {
            "# BEGIN MYCONFIG BLACKNPINK" | "# BEGIN MYCONFIG REFIND" => managed = true,
            "# END MYCONFIG BLACKNPINK" | "# END MYCONFIG REFIND" => managed = false,
            _ if !managed => {
                new.push_str(line);
                new.push('\n');
            }
            _ => {}
        }
    }
    new.push_str("\n# BEGIN MYCONFIG REFIND\ninclude managed.conf\ninclude themes/black-pink/theme.conf\n# END MYCONFIG REFIND\n");
    new
}

impl RefindModule for CachyosRefind {
    fn install(&self, context: &ModuleContext<'_>) -> ModuleResult {
        require_cachyos(context)?;
        let sh = context.shell;
        let Some(config) = refind_config(sh)? else {
            eprintln!("Skipping the rEFInd theme because no installation was found");
            return Ok(());
        };
        let directory = config
            .parent()
            .ok_or("rEFInd configuration has no parent directory")?;
        let theme = directory.join("themes/black-pink");
        let backup = PathBuf::from(format!("{}.pre-blacknpink", config.display()));
        let temporary = std::env::temp_dir().join(format!(
            "myconfig-refind-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)?
                .as_nanos()
        ));
        fs::create_dir(&temporary)?;
        let result = (|| -> ModuleResult {
            generate_refind_images(sh, &temporary)?;
            let theme_config = temporary.join("theme.conf");
            let global_config = temporary.join("global.conf");
            fs::write(
                &theme_config,
                DOTFILES
                    .assets
                    .refind
                    .refind
                    .themes
                    .black_pink
                    .theme_conf
                    .content,
            )?;
            fs::write(
                &global_config,
                DOTFILES.assets.refind.refind.global_conf.content,
            )?;
            let legacy = directory.join("themes/blacknpink");
            cmd!(sh, "sudo rm -rf -- {legacy}").run()?;
            cmd!(sh, "sudo install -d {theme}").run()?;
            let managed = directory.join("managed.conf");
            for (source, destination) in [
                (&theme_config, theme.join("theme.conf")),
                (&temporary.join("banner.png"), theme.join("banner.png")),
                (
                    &temporary.join("selection_big.png"),
                    theme.join("selection_big.png"),
                ),
                (
                    &temporary.join("selection_small.png"),
                    theme.join("selection_small.png"),
                ),
                (&global_config, managed),
            ] {
                cmd!(sh, "sudo install -m 0644 {source} {destination}").run()?;
            }
            let original = cmd!(sh, "sudo cat {config}").read()?;
            let proposed = temporary.join("refind.conf");
            fs::write(&proposed, updated_refind_config(&original))?;
            if !cmd!(sh, "sudo cmp -s {proposed} {config}")
                .quiet()
                .ignore_status()
                .output()?
                .status
                .success()
            {
                if !cmd!(sh, "sudo test -e {backup}")
                    .quiet()
                    .ignore_status()
                    .output()?
                    .status
                    .success()
                {
                    cmd!(sh, "sudo cp -a {config} {backup}").run()?;
                }
                cmd!(sh, "sudo cp {proposed} {config}").run()?;
            }
            Ok(())
        })();
        match (result, fs::remove_dir_all(&temporary)) {
            (Ok(()), Ok(())) => Ok(()),
            (Err(error), Ok(())) => Err(error),
            (Ok(()), Err(error)) => Err(error.into()),
            (Err(error), Err(cleanup)) => Err(format!(
                "{error}; could not remove {}: {cleanup}",
                temporary.display()
            )
            .into()),
        }
    }
}
fn active_group(sh: &xshell::Shell, wanted: &str) -> ModuleResult {
    let groups = cmd!(sh, "id -Gn").read()?;
    if groups.split_whitespace().any(|group| group == wanted) {
        Ok(())
    } else {
        Err(format!("the {wanted} group is not active in this session").into())
    }
}

fn configure_input_access(sh: &xshell::Shell, owner: &str) -> ModuleResult {
    if owner.is_empty()
        || !owner
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
    {
        return Err(format!("invalid input access owner: {owner}").into());
    }
    for program in ["id", "sudo", "udevadm"] {
        find_program(sh, program)?;
    }
    let user = cmd!(sh, "id -un").read()?;
    cmd!(sh, "sudo groupadd --system --force input").run()?;
    cmd!(sh, "sudo groupadd --system --force uinput").run()?;
    cmd!(sh, "sudo usermod -aG input,uinput {user}").run()?;
    cmd!(sh, "sudo modprobe uinput").run()?;
    cmd!(sh, "sudo install -d /etc/modules-load.d /etc/udev/rules.d").run()?;
    let module_file = format!("/etc/modules-load.d/myconfig-{owner}.conf");
    cmd!(sh, "sudo tee {module_file}")
        .quiet()
        .ignore_stdout()
        .stdin("uinput\n")
        .run()?;
    let rule_file = format!("/etc/udev/rules.d/99-myconfig-{owner}.rules");
    let rules = "KERNEL==\"uinput\", MODE=\"0660\", GROUP=\"uinput\", OPTIONS+=\"static_node=uinput\"\nSUBSYSTEM==\"input\", KERNEL==\"event*\", MODE=\"0660\", GROUP=\"input\"\n";
    cmd!(sh, "sudo tee {rule_file}")
        .quiet()
        .ignore_stdout()
        .stdin(rules)
        .run()?;
    cmd!(sh, "sudo udevadm control --reload-rules").run()?;
    cmd!(
        sh,
        "sudo udevadm trigger --subsystem-match=misc --sysname-match=uinput"
    )
    .run()?;
    cmd!(sh, "sudo udevadm trigger --subsystem-match=input").run()?;
    Ok(())
}

pub trait KanataModule {
    fn install(&self, context: &ModuleContext<'_>) -> ModuleResult;
}
pub struct CachyosKanata;
impl KanataModule for CachyosKanata {
    fn install(&self, context: &ModuleContext<'_>) -> ModuleResult {
        require_cachyos(context)?;
        let sh = context.shell;
        install_packages(sh, context.package_system, &[Package::Kanata])?;
        find_program(sh, "kanata")?;
        find_program(sh, "systemctl")?;
        let config = context.home.join(".config/kanata/config.kbd");
        require_file(&config)?;
        require_file(
            &context
                .home
                .join(".config/systemd/user/myconfig-kanata.service"),
        )?;
        cmd!(sh, "kanata --check --cfg {config}").run()?;
        configure_input_access(sh, "kanata")?;
        cmd!(sh, "systemctl --user daemon-reload").run()?;
        cmd!(sh, "systemctl --user enable myconfig-kanata.service").run()?;
        if active_group(sh, "input").is_ok() && active_group(sh, "uinput").is_ok() {
            cmd!(sh, "systemctl --user restart myconfig-kanata.service").run()?;
            cmd!(
                sh,
                "systemctl --user --quiet is-active myconfig-kanata.service"
            )
            .run()?;
        } else {
            eprintln!("Log out and back in before Kanata can access input devices");
        }
        Ok(())
    }
}
fn glass_package_name() -> ModuleResultString {
    let content = std::str::from_utf8(DOTFILES.assets.kde_plasma.kde_glass.PKGBUILD.content)?;
    let value = |key: &str| -> Result<&str, Box<dyn std::error::Error>> {
        content
            .lines()
            .find_map(|line| line.strip_prefix(key))
            .filter(|value| {
                !value.is_empty()
                    && value
                        .bytes()
                        .all(|byte| byte.is_ascii_alphanumeric() || b".-_".contains(&byte))
            })
            .ok_or_else(|| format!("Glass PKGBUILD has no simple {key} value").into())
    };
    Ok(format!(
        "{} {}-{}",
        value("pkgname=")?,
        value("pkgver=")?,
        value("pkgrel=")?
    ))
}

type ModuleResultString = Result<String, Box<dyn std::error::Error>>;

fn glass_effect_id() -> ModuleResultString {
    let effect = fs::read_to_string("/usr/share/myconfig/kde-glass/effect-id")?;
    let effect = effect.trim();
    if !effect.starts_with("myconfig_glass_")
        || !effect
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
    {
        return Err(format!("invalid Glass effect identifier: {effect}").into());
    }
    Ok(effect.to_owned())
}

fn install_kde_glass(context: &ModuleContext<'_>) -> ModuleResult {
    let sh = context.shell;
    let data = Path::new("/usr/share/myconfig/kde-glass");
    let previous = fs::read_to_string(data.join("effect-id")).unwrap_or_default();
    let built_for = fs::read_to_string(data.join("kwin-version")).unwrap_or_default();
    let kwin = cmd!(sh, "pacman -Q kwin").read()?;
    let expected = glass_package_name()?;
    let installed = cmd!(sh, "pacman -Q myconfig-kde-glass")
        .quiet()
        .read()
        .unwrap_or_default();
    if installed != expected || built_for.trim() != kwin {
        find_program(sh, "makepkg")?;
        let build = std::env::temp_dir().join(format!(
            "myconfig-glass-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)?
                .as_nanos()
        ));
        fs::create_dir(&build)?;
        let result = (|| -> ModuleResult {
            for file in DOTFILES.assets.kde_plasma.kde_glass.files() {
                let name = Path::new(file.path_from_root)
                    .file_name()
                    .ok_or("Glass resource has no name")?;
                install_embedded_file(file, &build.join(name))?;
            }
            let _cwd = sh.push_dir(&build);
            cmd!(sh, "makepkg --syncdeps --noconfirm").run()?;
            let mut packages = fs::read_dir(&build)?
                .filter_map(Result::ok)
                .map(|entry| entry.path())
                .filter(|path| {
                    path.file_name().is_some_and(|name| {
                        let name = name.to_string_lossy();
                        name.starts_with("myconfig-kde-glass-") && name.ends_with(".pkg.tar.zst")
                    })
                });
            let package = packages.next().ok_or("Glass build produced no package")?;
            if packages.next().is_some() {
                return Err("Glass build produced multiple matching packages".into());
            }
            let elevate = sh
                .var_os("MYCONFIG_GLASS_ELEVATE")
                .filter(|value| !value.is_empty())
                .unwrap_or_else(|| "sudo".into());
            cmd!(sh, "{elevate} pacman -U --noconfirm {package}").run()?;
            Ok(())
        })();
        if let Err(error) = result {
            return Err(format!(
                "Glass build failed; build files retained in {}: {error}",
                build.display()
            )
            .into());
        }
        fs::remove_dir_all(build)?;
    }
    let effect = glass_effect_id()?;
    let config = std::str::from_utf8(
        DOTFILES
            .kde_plasma
            ._local
            .share
            .myconfig
            .kde_plasma
            .glass_conf
            .content,
    )?;
    for line in config.lines() {
        if line.is_empty() || line.starts_with('[') {
            continue;
        }
        let (key, value) = line.split_once('=').ok_or("invalid Glass setting")?;
        cmd!(
            sh,
            "kwriteconfig6 --file kwinrc --group Effect-blurplus --key {key} {value}"
        )
        .run()?;
    }
    for old in ["blur", "glass", "myconfig_glass", previous.trim()] {
        if old.is_empty() || old == effect {
            continue;
        }
        let key = format!("{old}Enabled");
        cmd!(
            sh,
            "kwriteconfig6 --file kwinrc --group Plugins --key {key} false"
        )
        .run()?;
    }
    let key = format!("{effect}Enabled");
    cmd!(
        sh,
        "kwriteconfig6 --file kwinrc --group Plugins --key {key} true"
    )
    .run()?;
    Ok(())
}

fn activate_kde_glass(sh: &xshell::Shell) -> ModuleResult {
    if !cmd!(sh, "qdbus6 org.kde.KWin /KWin")
        .quiet()
        .ignore_status()
        .output()?
        .status
        .success()
    {
        eprintln!("Glass will load at the next KDE Plasma login");
        return Ok(());
    }
    let effect = glass_effect_id()?;
    let loaded = cmd!(
        sh,
        "qdbus6 org.kde.KWin /Effects org.kde.kwin.Effects.loadedEffects"
    )
    .read()?;
    for old in loaded.lines().filter(|name| {
        matches!(*name, "blur" | "glass" | "myconfig_glass") || name.starts_with("myconfig_glass_")
    }) {
        cmd!(
            sh,
            "qdbus6 org.kde.KWin /Effects org.kde.kwin.Effects.unloadEffect {old}"
        )
        .run()?;
        if old != effect {
            let key = format!("{old}Enabled");
            cmd!(
                sh,
                "kwriteconfig6 --file kwinrc --group Plugins --key {key} false"
            )
            .run()?;
        }
    }
    let loaded = cmd!(
        sh,
        "qdbus6 org.kde.KWin /Effects org.kde.kwin.Effects.loadEffect {effect}"
    )
    .read();
    let ready = if matches!(loaded.as_deref(), Ok("true")) {
        cmd!(
            sh,
            "qdbus6 org.kde.KWin /Effects org.kde.kwin.Effects.reconfigureEffect {effect}"
        )
        .run()
        .is_ok()
            && cmd!(
                sh,
                "qdbus6 org.kde.KWin /Effects org.kde.kwin.Effects.debug {effect} ''"
            )
            .read()
            .is_ok_and(|result| result.starts_with("valid=1 shaders=1 "))
    } else {
        false
    };
    if !ready {
        let _ = cmd!(
            sh,
            "qdbus6 org.kde.KWin /Effects org.kde.kwin.Effects.unloadEffect {effect}"
        )
        .quiet()
        .run();
        let key = format!("{effect}Enabled");
        cmd!(
            sh,
            "kwriteconfig6 --file kwinrc --group Plugins --key {key} false"
        )
        .run()?;
        if matches!(
            cmd!(
                sh,
                "qdbus6 org.kde.KWin /Effects org.kde.kwin.Effects.loadEffect blur"
            )
            .read()
            .as_deref(),
            Ok("true")
        ) {
            cmd!(
                sh,
                "kwriteconfig6 --file kwinrc --group Plugins --key blurEnabled true"
            )
            .run()?;
        }
        return Err(format!("KWin could not initialize Glass: {effect}").into());
    }
    Ok(())
}

pub trait KdePlasmaModule {
    fn install(&self, context: &ModuleContext<'_>) -> ModuleResult;
}
pub struct CachyosKdePlasma;

fn kde_setting(
    sh: &xshell::Shell,
    file: &str,
    groups: &[&str],
    key: &str,
    value: &str,
) -> ModuleResult {
    let group_args: Vec<_> = groups
        .iter()
        .flat_map(|group| ["--group", *group])
        .collect();
    cmd!(
        sh,
        "kwriteconfig6 --file {file} {group_args...} --key {key} {value}"
    )
    .run()?;
    Ok(())
}

fn configure_kde_appearance(context: &ModuleContext<'_>) -> ModuleResult {
    let sh = context.shell;
    let font = "Iosevka Nerd Font,12,-1,5,50,0,0,0,0,0";
    let small = "Iosevka Nerd Font,10,-1,5,50,0,0,0,0,0";
    for (group, key, value) in [
        ("General", "font", font),
        ("General", "fixed", font),
        ("General", "menuFont", font),
        ("General", "toolBarFont", font),
        ("General", "smallestReadableFont", small),
        ("WM", "activeFont", font),
    ] {
        kde_setting(sh, "kdeglobals", &[group], key, value)?;
    }
    for (group, key, value) in [
        ("Windows", "PerOutputVirtualDesktops", "true"),
        ("Windows", "ElectricBorderPushbackPixels", "0"),
        ("EdgeBarrier", "CornerBarrier", "false"),
        ("EdgeBarrier", "EdgeBarrier", "0"),
        ("Effect-overview", "BorderActivate", "9"),
    ] {
        kde_setting(sh, "kwinrc", &[group], key, value)?;
    }
    for (device, key, value) in [
        ("Pointer", "PointerAcceleration", "1.000"),
        ("Pointer", "PointerAccelerationProfile", "1"),
        ("Touchpad", "PointerAcceleration", "1.000"),
        ("Touchpad", "PointerAccelerationProfile", "1"),
        ("Touchpad", "NaturalScroll", "true"),
        ("Touchpad", "TapDragLock", "true"),
        ("Touchpad", "ClickMethod", "2"),
    ] {
        kde_setting(
            sh,
            "kcminputrc",
            &["Libinput", "Defaults", device],
            key,
            value,
        )?;
    }
    let theme = "blacknpink-crosshair";
    let size = "40";
    require_file(
        &context
            .home
            .join(".local/share/icons/blacknpink-crosshair/cursors/default"),
    )?;
    kde_setting(sh, "kcminputrc", &["Mouse"], "cursorSize", size)?;
    cmd!(sh, "plasma-apply-cursortheme --size {size} {theme}")
        .env("QT_QPA_PLATFORM", "offscreen")
        .run()?;
    let gtk_expression = r"s/^(gtk-cursor-theme-size=)[0-9]+$/\140/";
    for relative in [
        ".gtkrc-2.0",
        ".config/gtk-3.0/settings.ini",
        ".config/gtk-4.0/settings.ini",
    ] {
        let file = context.home.join(relative);
        if file.is_file() {
            cmd!(sh, "sed -i -E {gtk_expression} {file}").run()?;
        }
    }
    let xsettings = context.home.join(".config/xsettingsd/xsettingsd.conf");
    if xsettings.is_file() {
        let expression = r"s/^(Gtk\/CursorThemeSize )[0-9]+$/\140/";
        cmd!(sh, "sed -i -E {expression} {xsettings}").run()?;
    }
    if find_program(sh, "gsettings").is_ok()
        && cmd!(sh, "gsettings list-keys org.gnome.desktop.interface")
            .quiet()
            .read()
            .is_ok_and(|keys| keys.lines().any(|key| key == "cursor-size"))
    {
        cmd!(
            sh,
            "gsettings set org.gnome.desktop.interface cursor-size {size}"
        )
        .run()?;
    }
    kde_setting(
        sh,
        "kwinrc",
        &["Plugins"],
        "myconfig-plasma-panelsEnabled",
        "true",
    )?;
    Ok(())
}

impl KdePlasmaModule for CachyosKdePlasma {
    fn install(&self, context: &ModuleContext<'_>) -> ModuleResult {
        CachyosKdePlasmaValidate.install(context)?;
        let sh = context.shell;
        install_packages(
            sh,
            context.package_system,
            &[
                Package::IosevkaFont,
                Package::DesktopFileUtils,
                Package::Libinput,
            ],
        )?;
        for program in [
            "plasma-apply-cursortheme",
            "plasma-apply-lookandfeel",
            "kwriteconfig6",
            "qdbus6",
            "fc-match",
            "desktop-file-validate",
            "systemctl",
            "sudo",
        ] {
            find_program(sh, program)?;
        }
        install_kde_glass(context)?;
        let pointer = &DOTFILES
            .assets
            .kde_plasma
            .libinput
            ._90_myconfig_pointer_sensitivity_lua;
        let staged = std::env::temp_dir().join(format!(
            "myconfig-pointer-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)?
                .as_nanos()
        ));
        fs::write(&staged, pointer.content)?;
        let destination = "/etc/libinput/plugins/90-myconfig-pointer-sensitivity.lua";
        let install = cmd!(sh, "sudo install -Dm644 {staged} {destination}").run();
        let cleanup = fs::remove_file(&staged);
        match (install, cleanup) {
            (Ok(()), Ok(())) => {}
            (Err(error), Ok(())) => return Err(error.into()),
            (Ok(()), Err(error)) => return Err(error.into()),
            (Err(error), Err(cleanup)) => {
                return Err(format!(
                    "could not install pointer plugin: {error}; could not remove {}: {cleanup}",
                    staged.display()
                )
                .into());
            }
        }

        let home = context.home;
        require_executable(&home.join(".local/bin/myconfig-kde-plasma-layout"))?;
        for relative in [
            ".local/bin/myconfig-kde-plasma-glass-repair",
            ".local/share/color-schemes/BlackPink.colors",
            ".local/share/plasma/desktoptheme/blacknpink/metadata.json",
            ".local/share/plasma/desktoptheme/blacknpink/widgets/panel-background.svg",
            ".local/share/plasma/desktoptheme/blacknpink/dialogs/background.svg",
            ".local/share/plasma/desktoptheme/blacknpink/solid/dialogs/background.svg",
            ".local/share/plasma/look-and-feel/org.myconfig.blacknpink.desktop/metadata.json",
            ".local/share/plasma/look-and-feel/org.myconfig.blacknpink.desktop/contents/defaults",
            ".local/share/kwin/scripts/myconfig-plasma-panels/metadata.json",
            ".local/share/kwin/scripts/myconfig-plasma-panels/contents/code/main.js",
            ".config/systemd/user/myconfig-kde-plasma-layout.service",
            ".config/systemd/user/myconfig-kde-plasma-glass.service",
        ] {
            require_file(&home.join(relative))?;
        }
        for widget in ["overview", "session", "power", "island"] {
            for relative in ["metadata.json", "contents/ui/main.qml"] {
                require_file(&home.join(format!(
                    ".local/share/plasma/plasmoids/myconfig.{widget}/{relative}"
                )))?;
            }
        }
        let desktop = home.join(".config/autostart/myconfig-kde-plasma-layout.desktop");
        cmd!(sh, "desktop-file-validate {desktop}").run()?;
        let font_name = "Iosevka Nerd Font";
        if !cmd!(sh, "fc-match {font_name}").read()?.contains("Iosevka") {
            return Err("Iosevka Nerd Font is not available after installation".into());
        }
        let look = "org.myconfig.blacknpink.desktop";
        cmd!(sh, "plasma-apply-lookandfeel --apply {look}")
            .env("QT_QPA_PLATFORM", "offscreen")
            .run()?;
        configure_kde_appearance(context)?;

        let runtime = match sh.var_os("XDG_RUNTIME_DIR") {
            Some(value) if !value.is_empty() => PathBuf::from(value),
            _ => PathBuf::from(format!("/run/user/{}", cmd!(sh, "id -u").read()?)),
        };
        let bus = sh
            .var_os("DBUS_SESSION_BUS_ADDRESS")
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| format!("unix:path={}/bus", runtime.display()).into());
        let _runtime = sh.push_env("XDG_RUNTIME_DIR", &runtime);
        let _bus = sh.push_env("DBUS_SESSION_BUS_ADDRESS", bus);
        cmd!(sh, "systemctl --user daemon-reload").run()?;
        cmd!(
            sh,
            "systemctl --user enable myconfig-kde-plasma-glass.service"
        )
        .run()?;
        if cmd!(sh, "qdbus6 org.kde.KWin /KWin")
            .quiet()
            .ignore_status()
            .output()?
            .status
            .success()
        {
            activate_kde_glass(sh)?;
            cmd!(sh, "qdbus6 org.kde.KWin /KWin org.kde.KWin.reconfigure").run()?;
        }
        let layout = home.join(".local/bin/myconfig-kde-plasma-layout");
        let status = cmd!(sh, "{layout}").ignore_status().output()?.status;
        match status.code() {
            Some(0) => {
                cmd!(
                    sh,
                    "systemctl --user try-restart plasma-plasmashell.service"
                )
                .run()?;
            }
            Some(75) => eprintln!(
                "KDE Plasma is not active; the layout will apply at the next KDE Plasma login"
            ),
            _ => return Err(format!("KDE Plasma layout failed: {status}").into()),
        }
        Ok(())
    }
}
pub trait KanataKdeModule {
    fn install(&self, context: &ModuleContext<'_>) -> ModuleResult;
}
pub struct CachyosKanataKde;
impl KanataKdeModule for CachyosKanataKde {
    fn install(&self, context: &ModuleContext<'_>) -> ModuleResult {
        CachyosKdePlasmaValidate.install(context)?;
        let sh = context.shell;
        install_packages(
            sh,
            context.package_system,
            &[Package::Python, Package::Pyside6],
        )?;
        for program in ["kwriteconfig6", "python", "systemctl"] {
            find_program(sh, program)?;
        }
        let tray = context.home.join(".local/bin/myconfig-kanata-tray");
        require_executable(&tray)?;
        require_file(
            &context
                .home
                .join(".config/systemd/user/myconfig-kanata-tray.service"),
        )?;
        cmd!(sh, "python -m py_compile {tray}").run()?;
        let overview = "Meta+W,Meta+W,Toggle Overview";
        cmd!(
            sh,
            "kwriteconfig6 --file kglobalshortcutsrc --group kwin --key Overview {overview}"
        )
        .run()?;
        let old_enablement = context
            .home
            .join(".config/systemd/user/default.target.wants/myconfig-kanata.service");
        if exists_or_link(&old_enablement) {
            fs::remove_file(old_enablement)?;
        }
        cmd!(sh, "systemctl --user enable myconfig-kanata-tray.service").run()?;
        cmd!(sh, "systemctl --user daemon-reload").run()?;
        if active_group(sh, "input").is_err() || active_group(sh, "uinput").is_err() {
            cmd!(sh, "systemctl --user stop myconfig-kanata.service").run()?;
            eprintln!("Log out and back in before Kanata and its KDE tray start together");
        } else if cmd!(
            sh,
            "systemctl --user --quiet is-active graphical-session.target"
        )
        .quiet()
        .ignore_status()
        .output()?
        .status
        .success()
        {
            cmd!(sh, "systemctl --user restart myconfig-kanata-tray.service").run()?;
            cmd!(
                sh,
                "systemctl --user --quiet is-active myconfig-kanata-tray.service"
            )
            .run()?;
        } else {
            cmd!(sh, "systemctl --user stop myconfig-kanata.service").run()?;
            eprintln!("Kanata and its KDE tray are enabled for the next Plasma login");
        }
        Ok(())
    }
}
pub trait HandyModule {
    fn install(&self, context: &ModuleContext<'_>) -> ModuleResult;
}
pub struct CachyosHandy;
impl HandyModule for CachyosHandy {
    fn install(&self, context: &ModuleContext<'_>) -> ModuleResult {
        require_cachyos(context)?;
        let sh = context.shell;
        install_packages(sh, context.package_system, &[Package::Handy])?;
        for program in ["handy", "jq", "systemctl"] {
            find_program(sh, program)?;
        }
        let configure = context.home.join(".local/bin/myconfig-handy-configure");
        require_executable(&configure)?;
        require_file(
            &context
                .home
                .join(".config/systemd/user/myconfig-handy.service"),
        )?;
        configure_input_access(sh, "handy")?;
        cmd!(sh, "systemctl --user daemon-reload").run()?;
        cmd!(sh, "systemctl --user stop myconfig-handy.service").run()?;
        cmd!(sh, "{configure}").run()?;
        cmd!(sh, "systemctl --user enable myconfig-handy.service").run()?;
        if active_group(sh, "input").is_err() || active_group(sh, "uinput").is_err() {
            eprintln!("Log out and back in before Handy can read keyboard input");
        } else if cmd!(
            sh,
            "systemctl --user --quiet is-active graphical-session.target"
        )
        .quiet()
        .ignore_status()
        .output()?
        .status
        .success()
        {
            cmd!(sh, "systemctl --user restart myconfig-handy.service").run()?;
            cmd!(
                sh,
                "systemctl --user --quiet is-active myconfig-handy.service"
            )
            .run()?;
        } else {
            eprintln!("Handy is enabled for the next KDE Plasma login");
        }
        Ok(())
    }
}
pub trait PipewireModule {
    fn install(&self, context: &ModuleContext<'_>) -> ModuleResult;
}
pub struct CachyosPipewire;
impl PipewireModule for CachyosPipewire {
    fn install(&self, context: &ModuleContext<'_>) -> ModuleResult {
        require_cachyos(context)?;
        let sh = context.shell;
        install_packages(
            sh,
            context.package_system,
            &[Package::Python, Package::Pyside6],
        )?;
        find_program(sh, "python")?;
        find_program(sh, "systemctl")?;
        let tray = context.home.join(".local/bin/myconfig-pipewire-tray");
        require_executable(&tray)?;
        require_file(
            &context
                .home
                .join(".config/systemd/user/myconfig-pipewire-tray.service"),
        )?;
        cmd!(sh, "python -m py_compile {tray}").run()?;
        cmd!(sh, "systemctl --user enable myconfig-pipewire-tray.service").run()?;
        cmd!(sh, "systemctl --user daemon-reload").run()?;
        if cmd!(
            sh,
            "systemctl --user --quiet is-active graphical-session.target"
        )
        .quiet()
        .ignore_status()
        .output()?
        .status
        .success()
        {
            cmd!(
                sh,
                "systemctl --user restart myconfig-pipewire-tray.service"
            )
            .run()?;
            cmd!(
                sh,
                "systemctl --user --quiet is-active myconfig-pipewire-tray.service"
            )
            .run()?;
        } else {
            cmd!(sh, "systemctl --user stop myconfig-pipewire-tray.service").run()?;
        }
        Ok(())
    }
}
pub trait DockerModule {
    fn install(&self, context: &ModuleContext<'_>) -> ModuleResult;
}
pub struct CachyosDocker;
impl DockerModule for CachyosDocker {
    fn install(&self, context: &ModuleContext<'_>) -> ModuleResult {
        if context.profile != Profile::Cachyos || context.package_system != PackageSystem::Arch {
            return Err("Docker setup is supported only on CachyOS".into());
        }
        let sh = context.shell;
        let user = cmd!(sh, "id -un").read()?;
        for groups in [
            cmd!(sh, "id -Gn").read()?,
            cmd!(sh, "id -nG {user}").read()?,
        ] {
            if groups.split_whitespace().any(|group| group == "docker") {
                return Err(
                    "current user is already in the docker group; refusing Docker setup".into(),
                );
            }
        }
        install_packages(
            sh,
            context.package_system,
            &[
                Package::Docker,
                Package::DockerBuildx,
                Package::DockerCompose,
            ],
        )?;
        cmd!(sh, "sudo systemctl enable --now docker.service").run()?;
        let status = cmd!(sh, "systemctl is-active docker.service").read()?;
        if status != "active" {
            return Err(format!("docker.service is not active: {status}").into());
        }
        Ok(())
    }
}
#[cfg(unix)]
fn remove_existing_path(path: &Path) -> ModuleResult {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_dir() => fs::remove_dir_all(path)?,
        Ok(_) => fs::remove_file(path)?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    Ok(())
}

#[cfg(unix)]
fn link_agent_config(home: &Path) -> ModuleResult {
    use std::os::unix::fs::symlink;
    let skills = home.join(".agents/skills");
    let instructions = home.join(".agents/AGENTS.md");
    if !skills.is_dir() || !instructions.is_file() {
        return Err("agent skills or global AGENTS.md were not stowed".into());
    }
    let claude_skills = home.join(".claude/skills");
    fs::create_dir_all(&claude_skills)?;
    fs::create_dir_all(home.join(".config/opencode"))?;
    fs::create_dir_all(home.join(".fx"))?;

    for entry in fs::read_dir(&claude_skills)? {
        let entry = entry?;
        let path = entry.path();
        if fs::symlink_metadata(&path)?.file_type().is_symlink() {
            let target = fs::read_link(&path)?;
            if target.starts_with("../../.agents/skills")
                && !skills.join(entry.file_name()).is_dir()
            {
                fs::remove_file(path)?;
            }
        }
    }
    for entry in fs::read_dir(&skills)? {
        let entry = entry?;
        if !entry.path().is_dir() {
            continue;
        }
        let destination = claude_skills.join(entry.file_name());
        remove_existing_path(&destination)?;
        symlink(
            Path::new("../../.agents/skills").join(entry.file_name()),
            &destination,
        )?;
    }
    for destination in [
        home.join(".config/opencode/AGENTS.md"),
        home.join(".fx/AGENTS.md"),
    ] {
        remove_existing_path(&destination)?;
        symlink(&instructions, &destination)?;
        if fs::read_link(&destination)? != instructions {
            return Err(format!(
                "agent instructions link was not installed: {}",
                destination.display()
            )
            .into());
        }
    }
    Ok(())
}

#[cfg(not(unix))]
fn link_agent_config(_home: &Path) -> ModuleResult {
    Err("Linux agent links require Unix symbolic links".into())
}

fn configure_fx_playwright(context: &ModuleContext<'_>) -> ModuleResult {
    let sh = context.shell;
    find_program(sh, "jq")?;
    let directory = context.home.join(".fx");
    fs::create_dir_all(&directory)?;
    let config = directory.join("mcp.json");
    let contents = match fs::read(&config) {
        Ok(contents) => contents,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => b"{}".to_vec(),
        Err(error) => return Err(error.into()),
    };
    let program = context.home.join(".bun/bin/playwright-mcp");
    let expression = ".mcp = ((.mcpServers // {}) + (.mcp // {})) | .mcp.playwright = {\"type\": \"stdio\", \"command\": [$command, \"--headless\"], \"enabled\": true} | del(.mcpServers)";
    let output = cmd!(sh, "jq --arg command {program} {expression}")
        .stdin(contents)
        .output()?
        .stdout;
    let temporary = directory.join(format!(
        "mcp.json.{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_nanos()
    ));
    let result = (|| -> ModuleResult {
        use std::io::Write;
        let mut file = fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temporary)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&temporary, fs::Permissions::from_mode(0o600))?;
        }
        file.write_all(&output)?;
        file.sync_all()?;
        fs::rename(&temporary, &config)?;
        Ok(())
    })();
    if result.is_err() && fs::symlink_metadata(&temporary).is_ok() {
        fs::remove_file(&temporary)?;
    }
    result
}

pub trait AgentsConfigureModule {
    fn install(&self, context: &ModuleContext<'_>) -> ModuleResult;
}
pub struct CachyosAgentsConfigure;
pub struct ArchWslAgentsConfigure;

fn configure_agents(context: &ModuleContext<'_>) -> ModuleResult {
    let sh = context.shell;
    let bun = context.home.join(".bun");
    let path = std::env::join_paths(
        [context.home.join(".local/bin"), bun.join("bin")]
            .into_iter()
            .chain(std::env::split_paths(
                &sh.var_os("PATH").ok_or("PATH is unset")?,
            )),
    )?;
    let _bun = sh.push_env("BUN_INSTALL", &bun);
    let _path = sh.push_env("PATH", path);
    cmd!(sh, "opencode debug config").ignore_stdout().run()?;
    link_agent_config(context.home)?;
    configure_fx_playwright(context)?;
    if context.profile == Profile::Cachyos {
        for program in ["ydotool", "systemctl"] {
            find_program(sh, program)?;
        }
        configure_input_access(sh, "ydotool")?;
        cmd!(sh, "systemctl --user daemon-reload").run()?;
        cmd!(sh, "systemctl --user enable ydotool.service").run()?;
        if active_group(sh, "input").is_ok() && active_group(sh, "uinput").is_ok() {
            cmd!(sh, "systemctl --user restart ydotool.service").run()?;
            cmd!(sh, "systemctl --user --quiet is-active ydotool.service").run()?;
            #[cfg(unix)]
            let runtime = match sh.var_os("XDG_RUNTIME_DIR") {
                Some(value) if !value.is_empty() => PathBuf::from(value),
                _ => PathBuf::from(format!("/run/user/{}", cmd!(sh, "id -u").read()?)),
            };
            #[cfg(unix)]
            let socket = sh
                .var_os("YDOTOOL_SOCKET")
                .filter(|value| !value.is_empty())
                .map(PathBuf::from)
                .unwrap_or_else(|| runtime.join(".ydotool_socket"));
            #[cfg(unix)]
            {
                use std::os::unix::fs::FileTypeExt;
                let mut ready = false;
                for _ in 0..20 {
                    if fs::metadata(&socket).is_ok_and(|metadata| metadata.file_type().is_socket())
                    {
                        ready = true;
                        break;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(100));
                }
                if !ready {
                    return Err(
                        format!("ydotool did not create its socket: {}", socket.display()).into(),
                    );
                }
            }
            #[cfg(not(unix))]
            return Err("ydotool requires a Unix socket".into());
        } else {
            eprintln!("Log out and back in before ydotool can access uinput");
        }
    }
    Ok(())
}

impl AgentsConfigureModule for CachyosAgentsConfigure {
    fn install(&self, context: &ModuleContext<'_>) -> ModuleResult {
        require_cachyos(context)?;
        configure_agents(context)
    }
}
impl AgentsConfigureModule for ArchWslAgentsConfigure {
    fn install(&self, context: &ModuleContext<'_>) -> ModuleResult {
        if context.profile != Profile::ArchWsl {
            return Err("ArchWslAgentsConfigure requires Arch WSL".into());
        }
        require_arch(context)?;
        configure_agents(context)
    }
}
fn offer_authentication(
    name: &str,
    command: &str,
    program: &str,
    args: &[&str],
    sh: &xshell::Shell,
) -> ModuleResult {
    use std::io::{BufRead, Write};
    use std::process::{Command, Stdio};

    let mut terminal = match fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open("/dev/tty")
    {
        Ok(terminal) => terminal,
        Err(_) => {
            eprintln!("{name} is not authenticated. Run: {command}");
            return Ok(());
        }
    };
    write!(terminal, "{name} is not authenticated. Log in now? [y/N] ")?;
    terminal.flush()?;
    let mut answer = String::new();
    std::io::BufReader::new(terminal.try_clone()?).read_line(&mut answer)?;
    if !matches!(answer.trim(), "y" | "Y" | "yes" | "YES" | "Yes") {
        eprintln!("Skipped {name} authentication. Run: {command}");
        return Ok(());
    }
    let input = Stdio::from(terminal.try_clone()?);
    let output = Stdio::from(terminal.try_clone()?);
    let status = Command::new(program)
        .args(args)
        .env("PATH", sh.var_os("PATH").ok_or("PATH is unset")?)
        .stdin(input)
        .stdout(output)
        .stderr(Stdio::from(terminal))
        .status()?;
    if !status.success() {
        return Err(format!("{name} authentication failed: {status}").into());
    }
    Ok(())
}

pub trait AuthenticationModule {
    fn install(&self, context: &ModuleContext<'_>) -> ModuleResult;
}
pub struct CachyosAuthentication;
pub struct ArchWslAuthentication;

fn configure_authentication(context: &ModuleContext<'_>) -> ModuleResult {
    require_arch(context)?;
    let sh = context.shell;
    cmd!(sh, "git config --global core.symlinks true").run()?;
    if context.profile == Profile::ArchWsl
        && let Some(ssh) = sh
            .var_os("MYCONFIG_WINDOWS_SSH")
            .filter(|value| !value.is_empty())
    {
        let ssh = PathBuf::from(ssh);
        require_executable(&ssh)?;
        cmd!(sh, "git config --global core.sshCommand {ssh}").run()?;
    }
    if !cmd!(sh, "gh auth status")
        .quiet()
        .ignore_status()
        .output()?
        .status
        .success()
    {
        offer_authentication("GitHub CLI", "gh auth login", "gh", &["auth", "login"], sh)?;
    }
    if !cmd!(sh, "tailscale status")
        .quiet()
        .ignore_status()
        .output()?
        .status
        .success()
    {
        offer_authentication(
            "Tailscale",
            "sudo tailscale up",
            "sudo",
            &["tailscale", "up"],
            sh,
        )?;
    }
    Ok(())
}

impl AuthenticationModule for CachyosAuthentication {
    fn install(&self, context: &ModuleContext<'_>) -> ModuleResult {
        require_cachyos(context)?;
        configure_authentication(context)
    }
}
impl AuthenticationModule for ArchWslAuthentication {
    fn install(&self, context: &ModuleContext<'_>) -> ModuleResult {
        if context.profile != Profile::ArchWsl {
            return Err("ArchWslAuthentication requires Arch WSL".into());
        }
        configure_authentication(context)
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::{fs, os::unix::fs::PermissionsExt, path::Path};
    use xshell::Shell;

    fn fake_program(root: &Path, name: &str, script: &str) {
        let executable = root.join(name);
        fs::write(&executable, format!("#!/bin/sh\n{script}\n")).unwrap();
        fs::set_permissions(&executable, fs::Permissions::from_mode(0o755)).unwrap();
    }

    #[test]
    fn ubuntu_base_runs_the_five_apt_installations_via_xshell() {
        let root =
            std::env::temp_dir().join(format!("myconfig-ubuntu-base-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        fake_program(
            &root,
            "sudo",
            "printf '%s\\n' \"$*\" >> \"$MYCONFIG_TEST_LOG\"",
        );

        let sh = Shell::new().unwrap();
        sh.set_var("PATH", &root);
        sh.set_var("MYCONFIG_TEST_LOG", root.join("calls"));
        let context = ModuleContext {
            profile: Profile::UbuntuServer,
            package_system: PackageSystem::Apt,
            shell: &sh,
            home: Path::new("/not-used"),
        };
        UbuntuServerBase.install(&context).unwrap();
        assert_eq!(
            fs::read_to_string(root.join("calls")).unwrap(),
            "apt-get install -y ca-certificates curl git rsync stow\n"
        );

        let wrong_context = ModuleContext {
            profile: Profile::Cachyos,
            ..context
        };
        assert!(UbuntuServerBase.install(&wrong_context).is_err());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn ubuntu_zsh_installs_once_and_checks_the_login_shell() {
        let root = std::env::temp_dir().join(format!("myconfig-ubuntu-zsh-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        fake_program(
            &root,
            "sudo",
            "printf 'sudo %s\\n' \"$*\" >> \"$MYCONFIG_TEST_LOG\"\nif [ \"$1\" = chsh ]; then : > \"$MYCONFIG_TEST_ROOT/shell-changed\"; fi",
        );
        fake_program(
            &root,
            "curl",
            "printf 'curl %s\\n' \"$*\" >> \"$MYCONFIG_TEST_LOG\"\nprintf '#!/bin/sh\\nmkdir -p \"$HOME/.oh-my-zsh\"\\n'",
        );
        fake_program(
            &root,
            "git",
            "printf 'git %s\\n' \"$*\" >> \"$MYCONFIG_TEST_LOG\"\nmkdir -p \"$3\"",
        );
        fake_program(
            &root,
            "getent",
            "if [ -e \"$MYCONFIG_TEST_ROOT/shell-changed\" ]; then printf 'tester:x:1000:1000::/home/tester:%s\\n' \"$MYCONFIG_TEST_ROOT/zsh\"; else printf 'tester:x:1000:1000::/home/tester:/bin/bash\\n'; fi",
        );
        fake_program(&root, "zsh", "exit 0");

        let sh = Shell::new().unwrap();
        sh.set_var("PATH", format!("{}:/usr/bin:/bin", root.display()));
        sh.set_var("MYCONFIG_TEST_ROOT", &root);
        sh.set_var("MYCONFIG_TEST_LOG", root.join("calls"));
        sh.set_var("HOME", root.join("home"));
        sh.set_var("USER", "tester");
        let context = ModuleContext {
            profile: Profile::UbuntuServer,
            package_system: PackageSystem::Apt,
            shell: &sh,
            home: &root.join("home"),
        };

        UbuntuServerZsh.install(&context).unwrap();
        UbuntuServerZsh.install(&context).unwrap();
        let calls = fs::read_to_string(root.join("calls")).unwrap();
        assert_eq!(calls.matches("sudo apt-get install -y zsh").count(), 2);
        assert_eq!(calls.matches("curl -fsSL").count(), 1);
        assert_eq!(calls.matches("git clone").count(), 2);
        assert_eq!(calls.matches("sudo chsh -s").count(), 1);
        assert!(root.join("shell-changed").exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn ubuntu_dotfiles_stows_only_the_embedded_zsh_package() {
        let home =
            std::env::temp_dir().join(format!("myconfig-ubuntu-dotfiles-{}", std::process::id()));
        fs::create_dir_all(&home).unwrap();
        fs::write(home.join(".zshrc"), b"previous user config\n").unwrap();
        fs::create_dir_all(home.join(".oh-my-zsh")).unwrap();
        let sh = Shell::new().unwrap();
        let context = ModuleContext {
            profile: Profile::UbuntuServer,
            package_system: PackageSystem::Apt,
            shell: &sh,
            home: &home,
        };
        UbuntuServerDotfiles.install(&context).unwrap();
        assert_eq!(
            fs::read(home.join(".zshrc")).unwrap(),
            DOTFILES.zsh._zshrc.content
        );
        assert!(!home.join("dotfiles/assets").exists());
        let backups: Vec<_> = fs::read_dir(&home)
            .unwrap()
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| {
                path.file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with(".dotfiles-conflicts.backup.")
            })
            .collect();
        assert_eq!(backups.len(), 1);
        assert_eq!(
            fs::read(backups[0].join(".zshrc")).unwrap(),
            b"previous user config\n"
        );
        UbuntuServerDotfiles.install(&context).unwrap();
        assert_eq!(
            fs::read(home.join(".zshrc")).unwrap(),
            DOTFILES.zsh._zshrc.content
        );
        fs::remove_dir_all(home).unwrap();
    }
}
