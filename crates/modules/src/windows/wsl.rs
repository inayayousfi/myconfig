use std::{
    error::Error,
    fs,
    io::{self, Write},
    path::Path,
};

use xshell::{Shell, cmd};

use super::powershell_7;
use crate::{ModuleContext, ModuleResult, Profile};

type Value<T> = Result<T, Box<dyn Error>>;

const ARCH_ASSET: &str = "myconfig-arch-wsl-x86_64-unknown-linux-musl";

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

fn distro_name(sh: &Shell) -> Value<String> {
    let distro = match sh
        .var_os("MYCONFIG_WSL_DISTRO")
        .filter(|value| !value.is_empty())
    {
        Some(value) => value
            .into_string()
            .map_err(|_| "WSL distro name is not UTF-8")?,
        None => {
            let pwsh = powershell_7(sh)?;
            let expression = "[Net.Dns]::GetHostName()";
            format!(
                "{}-subsystem",
                cmd!(sh, "{pwsh} -NoProfile -Command {expression}").read()?
            )
        }
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

fn windows_user(sh: &Shell) -> Value<String> {
    let pwsh = powershell_7(sh)?;
    let expression = "[Environment]::UserName";
    let user = cmd!(sh, "{pwsh} -NoProfile -Command {expression}").read()?;
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

fn confirm(message: &str) -> Value<bool> {
    print!("{message} Type 'yes' to continue: ");
    io::stdout().flush()?;
    let mut answer = String::new();
    io::stdin().read_line(&mut answer)?;
    Ok(answer.trim() == "yes")
}

fn root_command(sh: &Shell, distro: &str, program: &str, args: &[&str]) -> ModuleResult {
    sh.cmd("wsl.exe")
        .args(["-d", distro, "-u", "root", "--", program])
        .args(args)
        .run()?;
    Ok(())
}

fn configure_root(sh: &Shell, distro: &str) -> ModuleResult {
    cmd!(sh, "wsl.exe -d {distro} -u root -- chpasswd")
        .stdin("root:root\n")
        .run()?;
    root_command(sh, distro, "pacman-key", &["--init"])?;
    root_command(sh, distro, "pacman-key", &["--populate", "archlinux"])?;
    root_command(sh, distro, "pacman", &["-Syu", "--noconfirm"])?;
    root_command(
        sh,
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
    let config = format!(
        "[interop]\nenabled=true\n\n[network]\nhostname={distro}\n\n[boot]\nsystemd=true\n"
    );
    cmd!(sh, "wsl.exe -d {distro} -u root -- tee /etc/wsl.conf")
        .stdin(config)
        .ignore_stdout()
        .run()?;
    Ok(())
}

fn configure_user(sh: &Shell, distro: &str, user: &str) -> ModuleResult {
    if !cmd!(sh, "wsl.exe -d {distro} -u root -- id {user}")
        .quiet()
        .ignore_status()
        .output()?
        .status
        .success()
    {
        root_command(sh, distro, "useradd", &["-m", "-G", "wheel", user])?;
    }
    let credentials = format!("{user}:{user}\n");
    cmd!(sh, "wsl.exe -d {distro} -u root -- chpasswd")
        .stdin(credentials)
        .run()?;
    let sed_wheel = "s/^# *%wheel ALL=(ALL:ALL) ALL/%wheel ALL=(ALL:ALL) ALL/";
    cmd!(
        sh,
        "wsl.exe -d {distro} -u root -- sed -i {sed_wheel} /etc/sudoers"
    )
    .run()?;
    let sudo_line = format!("{user} ALL=(ALL) NOPASSWD:ALL");
    if !cmd!(
        sh,
        "wsl.exe -d {distro} -u root -- grep -Fqx {sudo_line} /etc/sudoers"
    )
    .quiet()
    .ignore_status()
    .output()?
    .status
    .success()
    {
        cmd!(sh, "wsl.exe -d {distro} -u root -- tee -a /etc/sudoers")
            .stdin(format!("{sudo_line}\n"))
            .ignore_stdout()
            .run()?;
    }
    if !cmd!(
        sh,
        "wsl.exe -d {distro} -u root -- loginctl enable-linger {user}"
    )
    .quiet()
    .ignore_status()
    .output()?
    .status
    .success()
    {
        let marker = format!("/var/lib/systemd/linger/{user}");
        cmd!(
            sh,
            "wsl.exe -d {distro} -u root -- install -Dm644 /dev/null {marker}"
        )
        .run()?;
    }
    let config = format!(
        "[interop]\nenabled=true\n\n[user]\ndefault={user}\n\n[network]\nhostname={distro}\n\n[boot]\nsystemd=true\n"
    );
    cmd!(sh, "wsl.exe -d {distro} -u root -- tee /etc/wsl.conf")
        .stdin(config)
        .ignore_stdout()
        .run()?;
    let locale = r"s/^#\s*en_US.UTF-8 UTF-8/en_US.UTF-8 UTF-8/";
    cmd!(
        sh,
        "wsl.exe -d {distro} -u root -- sed -i {locale} /etc/locale.gen"
    )
    .run()?;
    root_command(sh, distro, "locale-gen", &[])?;
    cmd!(sh, "wsl.exe -d {distro} -u root -- tee /etc/locale.conf")
        .stdin("LANG=en_US.UTF-8\n")
        .ignore_stdout()
        .run()?;
    Ok(())
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

fn download_and_run_arch(sh: &Shell, distro: &str, tag: &str) -> ModuleResult {
    let directory = cmd!(
        sh,
        "wsl.exe -d {distro} --exec mktemp -d /tmp/myconfig-arch.XXXXXXXX"
    )
    .read()?;
    if !directory.starts_with("/tmp/myconfig-arch.") || directory.contains(char::is_whitespace) {
        return Err("WSL returned an unexpected temporary directory".into());
    }
    let result = (|| -> ModuleResult {
        let binary = format!("{directory}/{ARCH_ASSET}");
        let checksum = format!("{binary}.sha256");
        let url =
            format!("https://github.com/inayayousfi/myconfig/releases/download/{tag}/{ARCH_ASSET}");
        let checksum_url = format!("{url}.sha256");
        cmd!(sh, "wsl.exe -d {distro} --exec curl --fail --location --show-error --silent --output {binary} {url}").run()?;
        cmd!(sh, "wsl.exe -d {distro} --exec curl --fail --location --show-error --silent --output {checksum} {checksum_url}").run()?;
        let expected = cmd!(sh, "wsl.exe -d {distro} --exec cat {checksum}").read()?;
        let mut fields = expected.split_whitespace();
        let hash = fields.next().ok_or("Arch WSL release checksum is empty")?;
        if hash.len() != 64
            || !hash.bytes().all(|byte| byte.is_ascii_hexdigit())
            || fields.next() != Some(ARCH_ASSET)
            || fields.next().is_some()
        {
            return Err("Arch WSL release checksum has an unexpected format or filename".into());
        }
        let actual = cmd!(sh, "wsl.exe -d {distro} --exec sha256sum {binary}").read()?;
        if actual.split_whitespace().next() != Some(hash) {
            return Err("Arch WSL binary does not match its release SHA-256".into());
        }
        cmd!(sh, "wsl.exe -d {distro} --exec chmod 0755 {binary}").run()?;

        let windows_root = sh.var_os("SystemRoot").ok_or("SystemRoot is unset")?;
        let ssh = Path::new(&windows_root).join("System32/OpenSSH/ssh.exe");
        let ssh = if ssh.is_file() {
            windows_path_in_wsl(&ssh)?
        } else {
            String::new()
        };
        let ssh_env = format!("MYCONFIG_WINDOWS_SSH={ssh}");
        cmd!(sh, "wsl.exe -d {distro} --exec env {ssh_env} {binary}").run()?;
        Ok(())
    })();
    let cleanup = cmd!(sh, "wsl.exe -d {distro} -u root -- rm -r -- {directory}").run();
    match (result, cleanup) {
        (Ok(()), Ok(())) => Ok(()),
        (Err(error), Ok(())) => Err(error),
        (Ok(()), Err(error)) => Err(error.into()),
        (Err(error), Err(cleanup)) => Err(format!(
            "{error}; could not remove WSL download directory {directory}: {cleanup}"
        )
        .into()),
    }
}

fn configure_shell(sh: &Shell, distro: &str, user: &str) -> ModuleResult {
    let command = "command -v zsh";
    let zsh = cmd!(sh, "wsl.exe -d {distro} -u root -- /bin/sh -c {command}").read()?;
    if zsh.is_empty() {
        return Err("zsh was not installed inside Arch WSL".into());
    }
    root_command(sh, distro, "usermod", &["--shell", &zsh, user])?;
    let account = cmd!(sh, "wsl.exe -d {distro} -u root -- getent passwd {user}").read()?;
    if account.rsplit(':').next() != Some(zsh.as_str()) {
        return Err(format!("could not verify {user}'s Arch WSL login shell").into());
    }
    Ok(())
}

fn configure_autostart(context: &ModuleContext<'_>, distro: &str) -> ModuleResult {
    let sh = context.shell;
    let path = context.home.join(".wslconfig");
    let config = "[general]\ninstanceIdleTimeout=-1\n\n[wsl2]\nvmIdleTimeout=-1\n";
    match fs::read(&path) {
        Ok(contents) if contents != config.as_bytes() => {
            if !confirm(&format!(
                "Replace {} with the following WSL idle settings?\n{config}",
                path.display()
            ))? {
                return Err("WSL idle settings were not replaced".into());
            }
        }
        Ok(_) => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    fs::write(&path, config)?;

    let pwsh = powershell_7(sh)?;
    let startup_command = "[Environment]::GetFolderPath('Startup')";
    let startup = cmd!(sh, "{pwsh} -NoProfile -Command {startup_command}").read()?;
    if startup.is_empty() {
        return Err("Windows Startup directory is unavailable".into());
    }
    let shortcut = Path::new(&startup).join("myconfig-wsl-autostart.lnk");
    let root = sh.var_os("SystemRoot").ok_or("SystemRoot is unset")?;
    let powershell = Path::new(&root).join("System32/WindowsPowerShell/v1.0/powershell.exe");
    if !powershell.is_file() {
        return Err("Windows PowerShell for WSL autostart is unavailable".into());
    }
    let script = r#"$ErrorActionPreference = 'Stop'; $shell = New-Object -ComObject WScript.Shell; $link = $shell.CreateShortcut($env:MYCONFIG_WSL_SHORTCUT); $arguments = '-NoLogo -NoProfile -WindowStyle Hidden -Command "wsl.exe -d ' + $env:MYCONFIG_WSL_DISTRO + ' --exec /bin/true"'; $link.TargetPath = $env:MYCONFIG_WSL_POWERSHELL; $link.Arguments = $arguments; $link.Description = 'Boot the ' + $env:MYCONFIG_WSL_DISTRO + ' WSL instance at logon so its SSH server stays available.'; $link.WindowStyle = 7; $link.Save(); $saved = $shell.CreateShortcut($env:MYCONFIG_WSL_SHORTCUT); if ($saved.TargetPath -ine $env:MYCONFIG_WSL_POWERSHELL -or $saved.Arguments -cne $arguments) { throw 'WSL startup shortcut does not match this distribution' }"#;
    cmd!(sh, "{pwsh} -NoProfile -Command {script}")
        .env("MYCONFIG_WSL_SHORTCUT", &shortcut)
        .env("MYCONFIG_WSL_POWERSHELL", &powershell)
        .env("MYCONFIG_WSL_DISTRO", distro)
        .run()?;
    if !shortcut.is_file() {
        return Err("WSL logon shortcut was not created".into());
    }
    Ok(())
}

pub(super) fn install(context: &ModuleContext<'_>) -> ModuleResult {
    if context.profile != Profile::Windows || !cfg!(windows) {
        return Err("Arch WSL provisioning requires Windows".into());
    }
    let sh = context.shell;
    let tag = release_tag()?;
    let distro = distro_name(sh)?;
    let user = windows_user(sh)?;
    crate::windows::powershell_7(sh)?;
    crate::windows::find_program(sh, "wsl.exe")?;

    if !confirm(
        "The setup will shut down all running WSL distributions to apply Arch WSL settings.",
    )? {
        return Err("Arch WSL setup cancelled before changing distributions".into());
    }
    let list = cmd!(sh, "wsl.exe --list --quiet")
        .ignore_status()
        .output()?;
    let names = decode_distro_list(&list.stdout)?;
    if names
        .lines()
        .map(|line| line.trim_matches(['\u{feff}', '\0', '\r', ' ']))
        .any(|line| line.eq_ignore_ascii_case(&distro))
    {
        if !confirm(&format!(
            "Unregister existing WSL distribution '{distro}' and delete all its contents?"
        ))? {
            return Err("existing Arch WSL distribution was preserved".into());
        }
        let _ = cmd!(sh, "wsl.exe --terminate {distro}")
            .quiet()
            .ignore_status()
            .run();
        cmd!(sh, "wsl.exe --unregister {distro}").run()?;
    }
    if !cmd!(
        sh,
        "wsl.exe --install archlinux --name {distro} --no-launch"
    )
    .ignore_status()
    .output()?
    .status
    .success()
    {
        cmd!(sh, "wsl.exe --install archlinux --name {distro}").run()?;
    }
    configure_root(sh, &distro)?;
    cmd!(sh, "wsl.exe --shutdown").run()?;
    cmd!(sh, "wsl.exe -s {distro}").run()?;
    configure_user(sh, &distro, &user)?;
    cmd!(sh, "wsl.exe --shutdown").run()?;
    download_and_run_arch(sh, &distro, &tag)?;
    configure_shell(sh, &distro, &user)?;
    configure_autostart(context, &distro)?;
    cmd!(sh, "wsl.exe --shutdown").run()?;
    cmd!(sh, "wsl.exe -d {distro} --exec /bin/true").run()?;
    Ok(())
}
