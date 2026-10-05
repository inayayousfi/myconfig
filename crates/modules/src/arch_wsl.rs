//! A fresh Arch Linux WSL distribution named after this computer, set up by the
//! Arch WSL installer from the release that matches this Windows installer.
use std::{
    fs, io,
    path::{Path, PathBuf},
};

use xshell::cmd;

use crate::{
    Context, Footprint, Module, ModuleResult, Package,
    state::Change,
    support::{powershell, powershell_7, startup_directory},
};

pub struct ArchWsl;

type Value<T> = ModuleResult<T>;

const ARCH_ASSET: &str = "myconfig-arch-wsl-x86_64-unknown-linux-musl";
const WSL_CONFIG: &str = "[general]\ninstanceIdleTimeout=-1\n\n[wsl2]\nvmIdleTimeout=-1\n";

fn release_tag() -> Value<String> {
    let tag = option_env!("MYCONFIG_RELEASE_TAG")
        .map(str::to_owned)
        .or_else(|| std::env::var("MYCONFIG_RELEASE_TAG").ok())
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
        .var_os("MYCONFIG_WSL_DISTRO")
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

fn distributions(ctx: &Context) -> Value<Vec<String>> {
    // wsl.exe fails when no distribution is registered, which is an empty list here.
    let bytes = ctx.read_bytes_unchecked(cmd!(ctx.shell, "wsl.exe --list --quiet"))?;
    Ok(decode_distro_list(&bytes)?
        .lines()
        .map(|line| line.trim_matches(['\u{feff}', '\0', '\r', ' ']).to_owned())
        .filter(|line| !line.is_empty())
        .collect())
}

fn confirm(ctx: &Context, question: &str, refused: &str) -> ModuleResult {
    match ctx.confirm(question) {
        Ok(true) => Ok(()),
        Ok(false) => Err(refused.into()),
        Err(unanswered) => Err(format!("{refused}: {unanswered}").into()),
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
    ctx.run_with_input(
        cmd!(ctx.shell, "wsl.exe -d {distro} -u root -- chpasswd"),
        b"root:root\n",
    )?;
    root(ctx, distro, "pacman-key", &["--init"])?;
    root(ctx, distro, "pacman-key", &["--populate", "archlinux"])?;
    root(ctx, distro, "pacman", &["-Syu", "--noconfirm"])?;
    root(
        ctx,
        distro,
        "pacman",
        &[
            "-S",
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
    if !ctx.succeeds(cmd!(sh, "wsl.exe -d {distro} -u root -- id {user}"))? {
        root(ctx, distro, "useradd", &["-m", "-G", "wheel", user])?;
    }
    ctx.run_with_input(
        cmd!(sh, "wsl.exe -d {distro} -u root -- chpasswd"),
        format!("{user}:{user}\n").as_bytes(),
    )?;
    let sed_wheel = "s/^# *%wheel ALL=(ALL:ALL) ALL/%wheel ALL=(ALL:ALL) ALL/";
    ctx.run(cmd!(
        sh,
        "wsl.exe -d {distro} -u root -- sed -i {sed_wheel} /etc/sudoers"
    ))?;
    let sudo_line = format!("{user} ALL=(ALL) NOPASSWD:ALL");
    if !ctx.succeeds(cmd!(
        sh,
        "wsl.exe -d {distro} -u root -- grep -Fqx {sudo_line} /etc/sudoers"
    ))? {
        root_write(ctx, distro, "/etc/sudoers", &format!("{sudo_line}\n"), true)?;
    }
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

fn download_and_run_arch(ctx: &Context, distro: &str, tag: &str) -> ModuleResult {
    let sh = ctx.shell;
    let directory = ctx.read(cmd!(
        sh,
        "wsl.exe -d {distro} --exec mktemp -d /tmp/myconfig-arch.XXXXXXXX"
    ))?;
    if !directory.starts_with("/tmp/myconfig-arch.") || directory.contains(char::is_whitespace) {
        return Err("WSL returned an unexpected temporary directory".into());
    }
    let result = (|| -> ModuleResult {
        let binary = format!("{directory}/{ARCH_ASSET}");
        let checksum = format!("{binary}.sha256");
        let url =
            format!("https://github.com/inayayousfi/myconfig/releases/download/{tag}/{ARCH_ASSET}");
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
        ctx.run(cmd!(sh, "wsl.exe -d {distro} --exec chmod 0755 {binary}"))?;
        let windows_root = sh.var_os("SystemRoot").ok_or("SystemRoot is unset")?;
        let ssh = Path::new(&windows_root).join("System32/OpenSSH/ssh.exe");
        let ssh = if ssh.is_file() {
            windows_path_in_wsl(&ssh)?
        } else {
            String::new()
        };
        let ssh_env = format!("MYCONFIG_WINDOWS_SSH={ssh}");
        // The Arch WSL installer runs non-interactively inside the distribution.
        ctx.run(cmd!(
            sh,
            "wsl.exe -d {distro} --exec env {ssh_env} {binary} install"
        ))
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

fn shortcut_path(ctx: &Context) -> ModuleResult<PathBuf> {
    Ok(startup_directory(ctx)?.join("myconfig-wsl-autostart.lnk"))
}

fn configure_autostart(ctx: &Context, distro: &str) -> ModuleResult {
    let sh = ctx.shell;
    let path = ctx.home.join(".wslconfig");
    match fs::read(&path) {
        Ok(contents) if contents != WSL_CONFIG.as_bytes() => confirm(
            ctx,
            &format!(
                "Replace {} with the following WSL idle settings?\n{WSL_CONFIG}",
                path.display()
            ),
            "WSL idle settings were not replaced",
        )?,
        Ok(_) => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    ctx.write_file(&path, WSL_CONFIG.as_bytes(), false)?;

    let shortcut = shortcut_path(ctx)?;
    if shortcut.exists() {
        ctx.record_path(&shortcut)?;
    } else {
        ctx.created(&shortcut, false)?;
    }
    let root = sh.var_os("SystemRoot").ok_or("SystemRoot is unset")?;
    let windows_powershell =
        Path::new(&root).join("System32/WindowsPowerShell/v1.0/powershell.exe");
    if !windows_powershell.is_file() {
        return Err("Windows PowerShell for WSL autostart is unavailable".into());
    }
    let pwsh = powershell_7(ctx)?;
    let script = r#"$ErrorActionPreference = 'Stop'; $shell = New-Object -ComObject WScript.Shell; $link = $shell.CreateShortcut($env:MYCONFIG_WSL_SHORTCUT); $arguments = '-NoLogo -NoProfile -WindowStyle Hidden -Command "wsl.exe -d ' + $env:MYCONFIG_WSL_DISTRO + ' --exec /bin/true"'; $link.TargetPath = $env:MYCONFIG_WSL_POWERSHELL; $link.Arguments = $arguments; $link.Description = 'Boot the ' + $env:MYCONFIG_WSL_DISTRO + ' WSL instance at logon so its SSH server stays available.'; $link.WindowStyle = 7; $link.Save(); $saved = $shell.CreateShortcut($env:MYCONFIG_WSL_SHORTCUT); if ($saved.TargetPath -ine $env:MYCONFIG_WSL_POWERSHELL -or $saved.Arguments -cne $arguments) { throw 'WSL startup shortcut does not match this distribution' }"#;
    ctx.run(
        cmd!(sh, "{pwsh} -NoProfile -Command {script}")
            .env("MYCONFIG_WSL_SHORTCUT", &shortcut)
            .env("MYCONFIG_WSL_POWERSHELL", &windows_powershell)
            .env("MYCONFIG_WSL_DISTRO", distro),
    )?;
    if !shortcut.is_file() {
        return Err("WSL logon shortcut was not created".into());
    }
    Ok(())
}

impl Module for ArchWsl {
    fn name(&self) -> &'static str {
        "arch-wsl"
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
        let tag = release_tag()?;
        let distro = distro_name(ctx)?;
        let user = windows_user(ctx)?;
        powershell_7(ctx)?;
        ctx.find_program("wsl.exe")?;
        confirm(
            ctx,
            "The setup will shut down all running WSL distributions to apply Arch WSL settings. Continue?",
            "Arch WSL setup cancelled before changing distributions",
        )?;
        if distributions(ctx)?
            .iter()
            .any(|name| name.eq_ignore_ascii_case(&distro))
        {
            confirm(
                ctx,
                &format!(
                    "Unregister the existing WSL distribution '{distro}' and delete all its contents?"
                ),
                "the existing Arch WSL distribution was preserved",
            )?;
            let _ = ctx.succeeds(cmd!(sh, "wsl.exe --terminate {distro}"));
            ctx.run(cmd!(sh, "wsl.exe --unregister {distro}"))?;
        }
        ctx.record(Change::WslDistribution {
            name: distro.clone(),
        })?;
        if !ctx.succeeds(cmd!(
            sh,
            "wsl.exe --install archlinux --name {distro} --no-launch"
        ))? {
            ctx.run(cmd!(sh, "wsl.exe --install archlinux --name {distro}"))?;
        }
        configure_root(ctx, &distro)?;
        ctx.run(cmd!(sh, "wsl.exe --shutdown"))?;
        ctx.run(cmd!(sh, "wsl.exe -s {distro}"))?;
        configure_user(ctx, &distro, &user)?;
        ctx.run(cmd!(sh, "wsl.exe --shutdown"))?;
        download_and_run_arch(ctx, &distro, &tag)?;
        configure_shell(ctx, &distro, &user)?;
        configure_autostart(ctx, &distro)?;
        ctx.run(cmd!(sh, "wsl.exe --shutdown"))?;
        ctx.run(cmd!(sh, "wsl.exe -d {distro} --exec /bin/true"))
    }

    fn verify(&self, ctx: &Context) -> ModuleResult {
        let distro = distro_name(ctx)?;
        if !distributions(ctx)?
            .iter()
            .any(|name| name.eq_ignore_ascii_case(&distro))
        {
            return Err(format!("the WSL distribution {distro} is not registered").into());
        }
        if fs::read(ctx.home.join(".wslconfig"))? != WSL_CONFIG.as_bytes() {
            return Err("~/.wslconfig differs from the WSL idle settings".into());
        }
        if !shortcut_path(ctx)?.is_file() {
            return Err("the WSL logon shortcut is missing".into());
        }
        Ok(())
    }

    fn remove(&self, _ctx: &Context) -> ModuleResult {
        Ok(())
    }
}
