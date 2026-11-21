//! An Arch Linux WSL distribution named after this computer, created when it is not
//! registered, and set up by the Arch WSL installer from the release that matches this
//! Windows installer. A registered distribution of that name that this repository did
//! not create is replaced only after a confirmation.
use std::{
    fs, io,
    path::{Path, PathBuf},
};

use xshell::cmd;

use crate::{
    Context, Footprint, Module, ModuleResult, Need, Package, Setting,
    state::Change,
    support::{
        StartupShortcut, powershell, powershell_7, startup_directory, write_startup_shortcut,
    },
};

pub struct ArchWsl;

impl ArchWsl {
    pub const NAME: &'static str = "arch-wsl";
}

type Value<T> = ModuleResult<T>;

const ARCH_ASSET: &str = "inayos-arch-wsl-x86_64-unknown-linux-musl";
const WSL_CONFIG: &str = "[general]\ninstanceIdleTimeout=-1\n\n[wsl2]\nvmIdleTimeout=-1\n";
const SHORTCUT: &str = "inayos-wsl-autostart.lnk";
const SHUTDOWN_QUESTION: &str =
    "The setup will shut down all running WSL distributions to apply Arch WSL settings. Continue?";

/// Where the Linux installer that runs inside the distribution comes from.
enum ArchInstaller {
    /// The URL of this release's asset, checked against its published SHA-256.
    Release(String),
    /// A file already on this computer, as a path inside WSL. `INAYOS_ARCH_WSL_BINARY`
    /// names it, so a test can run an installer built from a working tree.
    Local(String),
}

fn arch_installer(ctx: &Context) -> Value<ArchInstaller> {
    let Some(path) = ctx
        .shell
        .var_os("INAYOS_ARCH_WSL_BINARY")
        .filter(|value| !value.is_empty())
    else {
        return Ok(ArchInstaller::Release(format!(
            "{}/{}/{ARCH_ASSET}",
            release_base(ctx)?,
            release_tag(ctx)?
        )));
    };
    let path = PathBuf::from(path);
    if !path.is_file() {
        return Err(format!("INAYOS_ARCH_WSL_BINARY names no file: {}", path.display()).into());
    }
    Ok(ArchInstaller::Local(windows_path_in_wsl(&path)?))
}

/// Where the releases of this repository publish their assets, one folder per tag.
/// release.yml embeds it in a published binary, and `INAYOS_RELEASE_BASE` replaces it
/// at run time.
fn release_base(ctx: &Context) -> Value<String> {
    let base = ctx
        .shell
        .var("INAYOS_RELEASE_BASE")
        .ok()
        .filter(|base| !base.is_empty())
        .or_else(|| option_env!("INAYOS_RELEASE_BASE").map(str::to_owned))
        .ok_or(
            "release location is unavailable; Arch WSL setup needs its matching Linux release",
        )?;
    if !base.starts_with("https://") || base.contains(char::is_whitespace) {
        return Err(format!("invalid release location: {base}").into());
    }
    Ok(base.trim_end_matches('/').to_owned())
}

fn release_tag(ctx: &Context) -> Value<String> {
    let tag = option_env!("INAYOS_RELEASE_TAG")
        .map(str::to_owned)
        .or_else(|| ctx.shell.var("INAYOS_RELEASE_TAG").ok())
        .ok_or("release tag is unavailable; Arch WSL setup needs its matching Linux release")?;
    if !tag.starts_with('v')
        || tag.len() < 2
        || !tag
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b".-_".contains(&byte))
    {
        return Err(format!("invalid release tag: {tag}").into());
    }
    Ok(tag)
}

fn decode_distro_list(bytes: &[u8]) -> Value<String> {
    if bytes.starts_with(&[0xff, 0xfe]) || bytes.get(1) == Some(&0) {
        let bytes = bytes.strip_prefix(&[0xff, 0xfe]).unwrap_or(bytes);
        let (pairs, remainder) = bytes.as_chunks::<2>();
        if !remainder.is_empty() {
            return Err("truncated UTF-16 WSL list".into());
        }
        let code_units = pairs.iter().map(|pair| u16::from_le_bytes(*pair));
        Ok(String::from_utf16(&code_units.collect::<Vec<_>>())?)
    } else {
        Ok(String::from_utf8(bytes.to_vec())?)
    }
}

fn windows_path_in_wsl(path: &Path) -> Value<String> {
    let path = path.to_str().ok_or("Windows SSH path is not UTF-8")?;
    let bytes = path.as_bytes();
    if bytes.len() < 3 || !bytes[0].is_ascii_alphabetic() || bytes[1] != b':' || bytes[2] != b'\\' {
        return Err(format!("Windows SSH path is not on a drive: {path}").into());
    }
    Ok(format!(
        "/mnt/{}{}",
        (bytes[0] as char).to_ascii_lowercase(),
        path[2..].replace('\\', "/")
    ))
}

fn distro_name(ctx: &Context) -> Value<String> {
    let distro = match ctx
        .shell
        .var_os("INAYOS_WSL_DISTRO")
        .filter(|value| !value.is_empty())
    {
        Some(value) => value
            .into_string()
            .map_err(|_| "WSL distro name is not UTF-8")?,
        None => format!("{}-subsystem", powershell(ctx, "[Net.Dns]::GetHostName()")?),
    }
    .to_ascii_lowercase();
    let bytes = distro.as_bytes();
    if bytes.is_empty()
        || bytes.len() > 63
        || !bytes[0].is_ascii_alphanumeric()
        || !bytes[bytes.len() - 1].is_ascii_alphanumeric()
        || !bytes
            .iter()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || *byte == b'-')
    {
        return Err(format!("WSL distro name is not a valid lowercase hostname: {distro}").into());
    }
    Ok(distro)
}

fn windows_user(ctx: &Context) -> Value<String> {
    // Linux user names are lowercase, so "Inaya" on Windows becomes "inaya" in Arch.
    let user = powershell(ctx, "[Environment]::UserName")?.to_ascii_lowercase();
    let bytes = user.as_bytes();
    if bytes.is_empty()
        || !matches!(bytes[0], b'a'..=b'z' | b'_')
        || !bytes.iter().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || *byte == b'_' || *byte == b'-'
        })
    {
        return Err(format!("Windows user name cannot be created as an Arch user: {user}").into());
    }
    Ok(user)
}

pub(crate) fn distributions(ctx: &Context) -> Value<Vec<String>> {
    // wsl.exe fails when no distribution is registered, which is an empty list here.
    let bytes = ctx.read_bytes_unchecked(cmd!(ctx.shell, "wsl.exe --list --quiet"))?;
    Ok(decode_distro_list(&bytes)?
        .lines()
        .map(|line| line.trim_matches(['\u{feff}', '\0', '\r', ' ']).to_owned())
        .filter(|line| !line.is_empty())
        .collect())
}

fn registered(ctx: &Context, distro: &str) -> Value<bool> {
    Ok(distributions(ctx)?
        .iter()
        .any(|name| name.eq_ignore_ascii_case(distro)))
}

/// Whether a run that was not removed yet recorded creating `distro`.
fn distribution_recorded(ctx: &Context, distro: &str) -> bool {
    ctx.state()
        .borrow()
        .pending(ArchWsl::NAME)
        .iter()
        .any(|entry| matches!(&entry.change, Change::WslDistribution { name } if name.eq_ignore_ascii_case(distro)))
}

/// The registry value where WSL keeps the default distribution, which `wsl.exe -s` sets.
fn default_distribution() -> Setting {
    Setting::RegistryValue {
        key: r"HKEY_CURRENT_USER\Software\Microsoft\Windows\CurrentVersion\Lxss".to_owned(),
        name: "DefaultDistribution".to_owned(),
    }
}

fn root(ctx: &Context, distro: &str, program: &str, args: &[&str]) -> ModuleResult {
    ctx.run(
        ctx.shell
            .cmd("wsl.exe")
            .args(["-d", distro, "-u", "root", "--", program])
            .args(args),
    )
}

fn root_write(
    ctx: &Context,
    distro: &str,
    path: &str,
    contents: &str,
    append: bool,
) -> ModuleResult {
    let flag: &[&str] = if append { &["-a"] } else { &[] };
    ctx.run_with_input(
        cmd!(
            ctx.shell,
            "wsl.exe -d {distro} -u root -- tee {flag...} {path}"
        )
        .ignore_stdout(),
        contents.as_bytes(),
    )
}

fn configure_root(ctx: &Context, distro: &str) -> ModuleResult {
    root(ctx, distro, "pacman-key", &["--init"])?;
    root(ctx, distro, "pacman-key", &["--populate", "archlinux"])?;
    root(ctx, distro, "pacman", &["-Syu", "--noconfirm"])?;
    root(
        ctx,
        distro,
        "pacman",
        &[
            "-S",
            "--needed",
            "--noconfirm",
            "sudo",
            "git",
            "base-devel",
            "wget",
            "curl",
            "unzip",
            "zip",
            "man-db",
            "man-pages",
            "vi",
            "rustup",
            "polkit",
            "zsh",
        ],
    )?;
    root_write(
        ctx,
        distro,
        "/etc/wsl.conf",
        &format!(
            "[interop]\nenabled=true\n\n[network]\nhostname={distro}\n\n[boot]\nsystemd=true\n"
        ),
        false,
    )
}

fn configure_user(ctx: &Context, distro: &str, user: &str) -> ModuleResult {
    let sh = ctx.shell;
    // The first password only; a later run keeps the one the person chose.
    if !ctx.succeeds(cmd!(sh, "wsl.exe -d {distro} -u root -- id {user}"))? {
        root(ctx, distro, "useradd", &["-m", "-G", "wheel", user])?;
        ctx.run_with_input(
            cmd!(sh, "wsl.exe -d {distro} -u root -- chpasswd"),
            format!("{user}:{user}\n").as_bytes(),
        )?;
    }
    let sed_wheel = "s/^# *%wheel ALL=(ALL:ALL) ALL/%wheel ALL=(ALL:ALL) ALL/";
    ctx.run(cmd!(
        sh,
        "wsl.exe -d {distro} -u root -- sed -i {sed_wheel} /etc/sudoers"
    ))?;
    // The wheel rule has no NOPASSWD, and sudo -v by default wants every matching rule
    // to be password-free. The installer starts with sudo -v, so without verifypw=any it
    // asks for a password that the NOPASSWD rule was meant to make unnecessary.
    for sudo_line in [
        format!("{user} ALL=(ALL) NOPASSWD:ALL"),
        format!("Defaults:{user} verifypw=any"),
    ] {
        if !ctx.succeeds(cmd!(
            sh,
            "wsl.exe -d {distro} -u root -- grep -Fqx {sudo_line} /etc/sudoers"
        ))? {
            root_write(ctx, distro, "/etc/sudoers", &format!("{sudo_line}\n"), true)?;
        }
    }
    root(ctx, distro, "visudo", &["-c"])?;
    if !ctx.succeeds(cmd!(
        sh,
        "wsl.exe -d {distro} -u root -- loginctl enable-linger {user}"
    ))? {
        let marker = format!("/var/lib/systemd/linger/{user}");
        ctx.run(cmd!(
            sh,
            "wsl.exe -d {distro} -u root -- install -Dm644 /dev/null {marker}"
        ))?;
    }
    root_write(
        ctx,
        distro,
        "/etc/wsl.conf",
        &format!(
            "[interop]\nenabled=true\n\n[user]\ndefault={user}\n\n[network]\nhostname={distro}\n\n[boot]\nsystemd=true\n"
        ),
        false,
    )?;
    let locale = r"s/^#\s*en_US.UTF-8 UTF-8/en_US.UTF-8 UTF-8/";
    ctx.run(cmd!(
        sh,
        "wsl.exe -d {distro} -u root -- sed -i {locale} /etc/locale.gen"
    ))?;
    root(ctx, distro, "locale-gen", &[])?;
    root_write(ctx, distro, "/etc/locale.conf", "LANG=en_US.UTF-8\n", false)
}

fn download_and_run_arch(ctx: &Context, distro: &str, installer: &ArchInstaller) -> ModuleResult {
    let sh = ctx.shell;
    let directory = ctx.read(cmd!(
        sh,
        "wsl.exe -d {distro} --exec mktemp -d /tmp/inayos-arch.XXXXXXXX"
    ))?;
    if !directory.starts_with("/tmp/inayos-arch.") || directory.contains(char::is_whitespace) {
        return Err("WSL returned an unexpected temporary directory".into());
    }
    let result = (|| -> ModuleResult {
        let binary = format!("{directory}/{ARCH_ASSET}");
        let url = match installer {
            ArchInstaller::Release(url) => url,
            ArchInstaller::Local(path) => {
                ctx.run(cmd!(sh, "wsl.exe -d {distro} --exec cp {path} {binary}"))?;
                return run_arch(ctx, distro, &binary);
            }
        };
        let checksum = format!("{binary}.sha256");
        let checksum_url = format!("{url}.sha256");
        ctx.run(cmd!(sh, "wsl.exe -d {distro} --exec curl --fail --location --show-error --silent --output {binary} {url}"))?;
        ctx.run(cmd!(sh, "wsl.exe -d {distro} --exec curl --fail --location --show-error --silent --output {checksum} {checksum_url}"))?;
        let expected = ctx.read(cmd!(sh, "wsl.exe -d {distro} --exec cat {checksum}"))?;
        let mut fields = expected.split_whitespace();
        let hash = fields.next().ok_or("Arch WSL release checksum is empty")?;
        if hash.len() != 64
            || !hash.bytes().all(|byte| byte.is_ascii_hexdigit())
            || fields.next() != Some(ARCH_ASSET)
            || fields.next().is_some()
        {
            return Err("Arch WSL release checksum has an unexpected format or filename".into());
        }
        let actual = ctx.read(cmd!(sh, "wsl.exe -d {distro} --exec sha256sum {binary}"))?;
        if actual.split_whitespace().next() != Some(hash) {
            return Err("Arch WSL binary does not match its release SHA-256".into());
        }
        run_arch(ctx, distro, &binary)
    })();
    let cleanup = ctx.run(cmd!(
        sh,
        "wsl.exe -d {distro} -u root -- rm -r -- {directory}"
    ));
    match (result, cleanup) {
        (Ok(()), Ok(())) => Ok(()),
        (Err(error), Ok(())) => Err(error),
        (Ok(()), Err(error)) => Err(error),
        (Err(error), Err(cleanup)) => Err(format!(
            "{error}; could not remove WSL download directory {directory}: {cleanup}"
        )
        .into()),
    }
}

/// Runs the Linux installer at `binary`, a path inside the distribution.
fn run_arch(ctx: &Context, distro: &str, binary: &str) -> ModuleResult {
    let sh = ctx.shell;
    ctx.run(cmd!(sh, "wsl.exe -d {distro} --exec chmod 0755 {binary}"))?;
    let windows_root = sh.var_os("SystemRoot").ok_or("SystemRoot is unset")?;
    let ssh = Path::new(&windows_root).join("System32/OpenSSH/ssh.exe");
    let ssh = if ssh.is_file() {
        windows_path_in_wsl(&ssh)?
    } else {
        String::new()
    };
    let ssh_env = format!("INAYOS_WINDOWS_SSH={ssh}");
    // With the console attached, the installer inside can ask whether to turn on its SSH
    // server and to log in to the GitHub CLI and Tailscale.
    let mut command = std::process::Command::new(ctx.find_program("wsl.exe")?);
    command.args(["-d", distro, "--exec", "env", &ssh_env, binary, "install"]);
    ctx.with_terminal(command)
}

fn configure_shell(ctx: &Context, distro: &str, user: &str) -> ModuleResult {
    let command = "command -v zsh";
    let zsh = ctx.read(cmd!(
        ctx.shell,
        "wsl.exe -d {distro} -u root -- /bin/sh -c {command}"
    ))?;
    if zsh.is_empty() {
        return Err("zsh was not installed inside Arch WSL".into());
    }
    root(ctx, distro, "usermod", &["--shell", &zsh, user])?;
    let account = ctx.read(cmd!(
        ctx.shell,
        "wsl.exe -d {distro} -u root -- getent passwd {user}"
    ))?;
    if account.rsplit(':').next() != Some(zsh.as_str()) {
        return Err(format!("could not verify {user}'s Arch WSL login shell").into());
    }
    Ok(())
}

fn configure_autostart(ctx: &Context, distro: &str) -> ModuleResult {
    ctx.write_file(&ctx.home.join(".wslconfig"), WSL_CONFIG.as_bytes(), false)?;
    let root = ctx
        .shell
        .var_os("SystemRoot")
        .ok_or("SystemRoot is unset")?;
    let windows_powershell =
        Path::new(&root).join("System32/WindowsPowerShell/v1.0/powershell.exe");
    if !windows_powershell.is_file() {
        return Err("Windows PowerShell for WSL autostart is unavailable".into());
    }
    write_startup_shortcut(
        ctx,
        &StartupShortcut {
            name: SHORTCUT,
            target: &windows_powershell,
            arguments: &format!(
                "-NoLogo -NoProfile -WindowStyle Hidden -Command \"wsl.exe -d {distro} --exec /bin/true\""
            ),
            working_directory: ctx.home,
            description: &format!(
                "Boot the {distro} WSL instance at logon so its SSH server stays available."
            ),
            minimized: true,
        },
    )
}

/// Whether `~/.wslconfig` holds other settings, which the install replaces.
fn other_wsl_config(ctx: &Context) -> Value<bool> {
    match fs::read(ctx.home.join(".wslconfig")) {
        Ok(contents) => Ok(contents != WSL_CONFIG.as_bytes()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error.into()),
    }
}

impl Module for ArchWsl {
    fn name(&self) -> &'static str {
        Self::NAME
    }

    fn needs(&self) -> &'static [Need] {
        &[]
    }

    fn footprint(&self, _ctx: &Context) -> Footprint {
        Footprint {
            packages: vec![Package::Wsl, Package::Powershell],
            settings: Vec::new(),
        }
    }

    fn install(&self, ctx: &Context) -> ModuleResult {
        if !cfg!(windows) {
            return Err("Arch WSL provisioning requires Windows".into());
        }
        let sh = ctx.shell;
        let installer = arch_installer(ctx)?;
        let distro = distro_name(ctx)?;
        let user = windows_user(ctx)?;
        powershell_7(ctx)?;
        ctx.find_program("wsl.exe")?;
        // A registered distribution that no pending record names was not created by this
        // repository. `remove` cannot undo changes inside it, so install never configures
        // it and replaces it only when the person agrees.
        let foreign = registered(ctx, &distro)? && !distribution_recorded(ctx, &distro);
        // Every question comes before the first change, so a no leaves nothing half done.
        let path = ctx.home.join(".wslconfig");
        let accepted = ctx.confirm(SHUTDOWN_QUESTION)?
            && (!other_wsl_config(ctx)?
                || ctx.confirm(&format!(
                    "Replace {} with the following WSL idle settings?\n{WSL_CONFIG}",
                    path.display()
                ))?)
            && (!foreign
                || ctx.confirm(&format!(
                    "The WSL distribution {distro} exists, and InayOS did not create it. Unregister it, delete everything inside it, and create a new one? Remove cannot bring it back."
                ))?);
        if !accepted {
            ctx.note("Arch WSL not set up; the next install asks again");
            return ctx.record_declined();
        }
        ctx.record_accepted()?;
        if foreign {
            ctx.run(cmd!(sh, "wsl.exe --unregister {distro}"))?;
        }
        // A distribution that an earlier run created is configured as it is.
        if !registered(ctx, &distro)? {
            // The undo counts a missing distribution as done, so one record covers every
            // run that creates it again.
            if !distribution_recorded(ctx, &distro) {
                ctx.record(Change::WslDistribution {
                    name: distro.clone(),
                })?;
            }
            if !ctx.succeeds(cmd!(
                sh,
                "wsl.exe --install archlinux --name {distro} --no-launch"
            ))? {
                ctx.run(cmd!(sh, "wsl.exe --install archlinux --name {distro}"))?;
            }
            ctx.run_with_input(
                cmd!(sh, "wsl.exe -d {distro} -u root -- chpasswd"),
                b"root:root\n",
            )?;
        }
        configure_root(ctx, &distro)?;
        ctx.run(cmd!(sh, "wsl.exe --shutdown"))?;
        ctx.record_setting(default_distribution())?;
        ctx.run(cmd!(sh, "wsl.exe -s {distro}"))?;
        configure_user(ctx, &distro, &user)?;
        // Every change this module makes inside the distribution comes before the Arch WSL
        // installer, so that installer's remove brings the distribution back to this point.
        configure_shell(ctx, &distro, &user)?;
        ctx.run(cmd!(sh, "wsl.exe --shutdown"))?;
        download_and_run_arch(ctx, &distro, &installer)?;
        configure_autostart(ctx, &distro)?;
        ctx.run(cmd!(sh, "wsl.exe --shutdown"))?;
        ctx.run(cmd!(sh, "wsl.exe -d {distro} --exec /bin/true"))
    }

    fn verify(&self, ctx: &Context) -> ModuleResult {
        if ctx.declined() {
            return Ok(());
        }
        let distro = distro_name(ctx)?;
        if !registered(ctx, &distro)? {
            return Err(format!("the WSL distribution {distro} is not registered").into());
        }
        if fs::read(ctx.home.join(".wslconfig"))? != WSL_CONFIG.as_bytes() {
            return Err("~/.wslconfig differs from the WSL idle settings".into());
        }
        if !startup_directory(ctx)?.join(SHORTCUT).is_file() {
            return Err("the WSL logon shortcut is missing".into());
        }
        Ok(())
    }

    fn remove(&self, _ctx: &Context) -> ModuleResult {
        Ok(())
    }
}
