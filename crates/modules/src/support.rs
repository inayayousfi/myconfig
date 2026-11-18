//! Helpers shared by several modules and by the runner.
use std::{
    fs,
    path::{Path, PathBuf},
};

use xshell::cmd;

use crate::{Context, ModuleResult, Setting, context::temporary_path};

pub(crate) fn require_file(path: &Path) -> ModuleResult {
    if !fs::metadata(path).is_ok_and(|metadata| metadata.is_file()) {
        return Err(format!("required file is missing: {}", path.display()).into());
    }
    Ok(())
}

pub(crate) fn require_executable(path: &Path) -> ModuleResult {
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

pub(crate) fn verify_packages(ctx: &Context, packages: &[crate::Package]) -> ModuleResult {
    for (package, spec) in ctx.specs(packages)? {
        if !ctx.packages().installed(ctx, spec)? {
            return Err(format!("package {} ({package:?}) is not installed", spec.name).into());
        }
    }
    Ok(())
}

pub(crate) fn verify_absent(ctx: &Context, packages: &[crate::Package]) -> ModuleResult {
    for (package, spec) in ctx.specs(packages)? {
        if ctx.packages().installed(ctx, spec)? {
            return Err(format!(
                "package {} ({package:?}) should not be installed",
                spec.name
            )
            .into());
        }
    }
    Ok(())
}

pub(crate) fn require_programs(ctx: &Context, programs: &[&str]) -> ModuleResult {
    for program in programs {
        ctx.find_program(program)?;
    }
    Ok(())
}

/// Runs `action` with a new temporary directory, then deletes it.
pub(crate) fn with_temporary_directory<T>(
    purpose: &str,
    action: impl FnOnce(&Path) -> ModuleResult<T>,
) -> ModuleResult<T> {
    let directory = temporary_path(purpose);
    fs::create_dir(&directory)?;
    let result = action(&directory);
    match (result, fs::remove_dir_all(&directory)) {
        (Ok(value), Ok(())) => Ok(value),
        (Err(error), Ok(())) => Err(error),
        (Ok(_), Err(error)) => Err(error.into()),
        (Err(error), Err(cleanup)) => Err(format!(
            "{error}; could not remove {}: {cleanup}",
            directory.display()
        )
        .into()),
    }
}

pub(crate) fn current_user(ctx: &Context) -> ModuleResult<String> {
    let user = ctx.read(cmd!(ctx.shell, "id -un"))?;
    if user.is_empty() || user.chars().any(char::is_whitespace) {
        return Err("could not determine a safe user name".into());
    }
    Ok(user)
}

/// Whether the user's systemd manager, which runs the user services, has `wanted` among its
/// groups. A login made after the group was added has it, but a manager that started earlier
/// keeps its old groups until every session of the user ends.
pub(crate) fn active_group(ctx: &Context, wanted: &str) -> ModuleResult<bool> {
    let sh = ctx.shell;
    let manager = format!("user@{}.service", ctx.read(cmd!(sh, "id -u"))?);
    let pid = ctx.read(cmd!(
        sh,
        "systemctl show --property=MainPID --value {manager}"
    ))?;
    if pid.is_empty() || pid == "0" {
        return Ok(false);
    }
    let groups = ctx.read(cmd!(sh, "ps -o supgrp= -p {pid}"))?;
    Ok(groups.split(',').any(|group| group.trim() == wanted))
}

pub(crate) fn graphical_session(ctx: &Context) -> ModuleResult<bool> {
    ctx.succeeds(cmd!(
        ctx.shell,
        "systemctl --user --quiet is-active graphical-session.target"
    ))
}

/// The settings that give a program access to keyboard and virtual input devices.
pub(crate) fn input_access_settings(ctx: &Context) -> Vec<Setting> {
    let user = ctx.shell.var("USER").unwrap_or_default();
    ["input", "uinput"]
        .into_iter()
        .flat_map(|group| {
            [
                Setting::GroupExists {
                    group: group.to_owned(),
                },
                Setting::GroupMember {
                    group: group.to_owned(),
                    user: user.clone(),
                },
            ]
        })
        .collect()
}

pub(crate) fn configure_input_access(ctx: &Context, owner: &str) -> ModuleResult {
    if owner.is_empty()
        || !owner
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
    {
        return Err(format!("invalid input access owner: {owner}").into());
    }
    require_programs(ctx, &["id", "sudo", "udevadm"])?;
    let sh = ctx.shell;
    let user = current_user(ctx)?;
    for group in ["input", "uinput"] {
        ctx.add(Setting::GroupExists {
            group: group.to_owned(),
        })?;
        ctx.add(Setting::GroupMember {
            group: group.to_owned(),
            user: user.clone(),
        })?;
    }
    ctx.run(cmd!(sh, "sudo modprobe uinput"))?;
    ctx.write_system_file(
        &PathBuf::from(format!("/etc/modules-load.d/inayos-{owner}.conf")),
        b"uinput\n",
        "0644",
    )?;
    ctx.write_system_file(
        &PathBuf::from(format!("/etc/udev/rules.d/99-inayos-{owner}.rules")),
        b"KERNEL==\"uinput\", MODE=\"0660\", GROUP=\"uinput\", OPTIONS+=\"static_node=uinput\"\nSUBSYSTEM==\"input\", KERNEL==\"event*\", MODE=\"0660\", GROUP=\"input\"\n",
        "0644",
    )?;
    ctx.run(cmd!(sh, "sudo udevadm control --reload-rules"))?;
    ctx.run(cmd!(
        sh,
        "sudo udevadm trigger --subsystem-match=misc --sysname-match=uinput"
    ))?;
    ctx.run(cmd!(sh, "sudo udevadm trigger --subsystem-match=input"))?;
    Ok(())
}

pub(crate) fn verify_input_access(ctx: &Context, owner: &str) -> ModuleResult {
    for setting in input_access_settings(ctx) {
        if setting.read(ctx)?.is_none() {
            return Err(format!("missing {}", setting.describe()).into());
        }
    }
    for path in [
        format!("/etc/modules-load.d/inayos-{owner}.conf"),
        format!("/etc/udev/rules.d/99-inayos-{owner}.rules"),
    ] {
        if !ctx.succeeds(cmd!(ctx.shell, "sudo test -f {path}"))? {
            return Err(format!("{path} is missing").into());
        }
    }
    Ok(())
}

/// Extends PATH with `~/.local/bin` and Bun's global directory for agent tools.
pub(crate) fn agent_tool_path(ctx: &Context) -> ModuleResult<std::ffi::OsString> {
    let bun = ctx.home.join(".bun");
    Ok(std::env::join_paths(
        [ctx.home.join(".local/bin"), bun.join("bin")]
            .into_iter()
            .chain(std::env::split_paths(
                &ctx.shell.var_os("PATH").ok_or("PATH is unset")?,
            )),
    )?)
}

pub(crate) fn user_runtime_directory(ctx: &Context) -> ModuleResult<PathBuf> {
    Ok(match ctx.shell.var_os("XDG_RUNTIME_DIR") {
        Some(value) if !value.is_empty() => PathBuf::from(value),
        _ => PathBuf::from(format!("/run/user/{}", ctx.read(cmd!(ctx.shell, "id -u"))?)),
    })
}

/// The folder that holds the user's configuration files, as KDE and GTK programs find it.
pub(crate) fn config_home(ctx: &Context) -> PathBuf {
    ctx.shell
        .var_os("XDG_CONFIG_HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| ctx.home.join(".config"))
}

pub(crate) fn powershell_7(ctx: &Context) -> ModuleResult<PathBuf> {
    if let Ok(program) = ctx.find_program("pwsh.exe") {
        return Ok(program);
    }
    let program_files = ctx
        .shell
        .var_os("ProgramFiles")
        .ok_or("PowerShell 7 is not on PATH and ProgramFiles is unset")?;
    let program = Path::new(&program_files).join("PowerShell/7/pwsh.exe");
    if !program.is_file() {
        return Err("PowerShell 7 was not found after installation".into());
    }
    Ok(program)
}

/// Runs a PowerShell 7 command and returns its output.
pub(crate) fn powershell(ctx: &Context, command: &str) -> ModuleResult<String> {
    let pwsh = powershell_7(ctx)?;
    ctx.read(cmd!(ctx.shell, "{pwsh} -NoProfile -Command {command}"))
}

/// `text` as a single-quoted PowerShell string, which expands nothing.
pub(crate) fn powershell_quote(text: &str) -> String {
    format!("'{}'", text.replace('\'', "''"))
}

/// Runs a PowerShell 7 script as administrator through the system's elevation prompt,
/// waits for it, and fails when it exits nonzero. An elevated process does not inherit
/// this one's environment, so the script carries its data inside, quoted with
/// `powershell_quote`. It travels encoded, so no quoting of the command line applies.
pub(crate) fn run_elevated(ctx: &Context, script: &str) -> ModuleResult {
    let pwsh = powershell_7(ctx)?;
    let elevate = "$encoded = [Convert]::ToBase64String([Text.Encoding]::Unicode.GetBytes(\"`$ErrorActionPreference = 'Stop'`n\" + $env:INAYOS_ELEVATED_SCRIPT)); $process = Start-Process -FilePath $env:INAYOS_PWSH -Verb RunAs -ArgumentList '-NoLogo', '-NoProfile', '-EncodedCommand', $encoded -Wait -PassThru -ErrorAction Stop; if ($process.ExitCode -ne 0) { throw \"The elevated step exited with code $($process.ExitCode)\" }";
    ctx.run(
        cmd!(ctx.shell, "{pwsh} -NoProfile -Command {elevate}")
            .env("INAYOS_ELEVATED_SCRIPT", script)
            .env("INAYOS_PWSH", &pwsh),
    )
}

pub(crate) fn startup_directory(ctx: &Context) -> ModuleResult<PathBuf> {
    let startup = powershell(ctx, "[Environment]::GetFolderPath('Startup')")?;
    if startup.is_empty() {
        return Err("Windows Startup directory is unavailable".into());
    }
    Ok(PathBuf::from(startup))
}

/// A shortcut in the Startup folder, which Windows opens at logon.
pub(crate) struct StartupShortcut<'a> {
    /// The file name in the Startup folder, ending in `.lnk`.
    pub name: &'a str,
    pub target: &'a Path,
    pub arguments: &'a str,
    pub working_directory: &'a Path,
    pub description: &'a str,
    /// Opens the target's window minimized.
    pub minimized: bool,
}

/// Writes `shortcut` after recording what its path held, and checks that Windows saved
/// its target and arguments.
pub(crate) fn write_startup_shortcut(ctx: &Context, shortcut: &StartupShortcut) -> ModuleResult {
    let path = startup_directory(ctx)?.join(shortcut.name);
    ctx.record_path(&path)?;
    let pwsh = powershell_7(ctx)?;
    let script = "$ErrorActionPreference = 'Stop'; $shell = New-Object -ComObject WScript.Shell; $link = $shell.CreateShortcut($env:INAYOS_SHORTCUT); $link.TargetPath = $env:INAYOS_SHORTCUT_TARGET; $link.Arguments = $env:INAYOS_SHORTCUT_ARGUMENTS; $link.WorkingDirectory = $env:INAYOS_SHORTCUT_DIRECTORY; $link.Description = $env:INAYOS_SHORTCUT_DESCRIPTION; $link.WindowStyle = [int]$env:INAYOS_SHORTCUT_WINDOW; $link.Save(); $saved = $shell.CreateShortcut($env:INAYOS_SHORTCUT); if ($saved.TargetPath -ine $env:INAYOS_SHORTCUT_TARGET -or $saved.Arguments -cne $env:INAYOS_SHORTCUT_ARGUMENTS) { throw 'The saved shortcut does not match its target' }";
    // WScript.Shell window styles: 1 opens normally, 7 opens minimized.
    let window = if shortcut.minimized { "7" } else { "1" };
    ctx.run(
        cmd!(ctx.shell, "{pwsh} -NoProfile -Command {script}")
            .env("INAYOS_SHORTCUT", &path)
            .env("INAYOS_SHORTCUT_TARGET", shortcut.target)
            .env("INAYOS_SHORTCUT_ARGUMENTS", shortcut.arguments)
            .env("INAYOS_SHORTCUT_DIRECTORY", shortcut.working_directory)
            .env("INAYOS_SHORTCUT_DESCRIPTION", shortcut.description)
            .env("INAYOS_SHORTCUT_WINDOW", window),
    )?;
    if !path.is_file() {
        return Err(format!("the startup shortcut {} was not created", path.display()).into());
    }
    Ok(())
}

/// Writes the files of an embedded directory under `destination`, recording each one.
pub(crate) fn copy_embedded(
    ctx: &Context,
    files: Vec<&typed_fs_rs::EmbeddedFile>,
    source: &Path,
    destination: &Path,
) -> ModuleResult<usize> {
    let mut copied = 0;
    for file in files {
        let relative = Path::new(file.path_from_root).strip_prefix(source)?;
        ctx.write_file(&destination.join(relative), file.content, file.executable)?;
        copied += 1;
    }
    Ok(copied)
}

/// Checks that each embedded file under `source` has the same contents under `destination`.
pub(crate) fn verify_embedded(
    files: Vec<&typed_fs_rs::EmbeddedFile>,
    source: &Path,
    destination: &Path,
) -> ModuleResult {
    for file in files {
        let path = destination.join(Path::new(file.path_from_root).strip_prefix(source)?);
        if fs::read(&path)? != file.content {
            return Err(format!("{} differs from the repository", path.display()).into());
        }
    }
    Ok(())
}

/// The file types a desktop entry declares, from the first directory of `XDG_DATA_DIRS`
/// that has it.
pub(crate) fn desktop_entry_types(ctx: &Context, entry: &str) -> ModuleResult<Vec<String>> {
    let directories = ctx
        .shell
        .var("XDG_DATA_DIRS")
        .ok()
        .filter(|directories| !directories.is_empty())
        .unwrap_or_else(|| "/usr/local/share:/usr/share".to_owned());
    let path = directories
        .split(':')
        .map(|directory| Path::new(directory).join("applications").join(entry))
        .find(|path| path.is_file())
        .ok_or_else(|| format!("desktop entry not found: {entry}"))?;
    let contents = fs::read_to_string(&path)?;
    Ok(contents
        .lines()
        .find_map(|line| line.strip_prefix("MimeType="))
        .unwrap_or_default()
        .split(';')
        .filter(|kind| !kind.is_empty())
        .map(str::to_owned)
        .collect())
}

/// The settings that make `entry` the default application for each of `types`.
/// Defaults for other file types stay.
pub(crate) fn default_application_settings(types: &[String]) -> Vec<Setting> {
    types
        .iter()
        .map(|kind| Setting::kde("mimeapps.list", &["Default Applications"], kind))
        .collect()
}

pub(crate) fn set_default_application(
    ctx: &Context,
    entry: &str,
    types: &[String],
) -> ModuleResult {
    for setting in default_application_settings(types) {
        ctx.set(setting, entry)?;
    }
    Ok(())
}

/// A root-owned file's contents, or `None` when it does not exist.
pub(crate) fn read_system_file(ctx: &Context, path: &str) -> ModuleResult<Option<String>> {
    read_system_bytes(ctx, path)?
        .map(|bytes| Ok(String::from_utf8(bytes)?))
        .transpose()
}

/// Reads a root-owned file that may hold binary data, or `None` when it is missing.
pub(crate) fn read_system_bytes(ctx: &Context, path: &str) -> ModuleResult<Option<Vec<u8>>> {
    if !ctx.succeeds(cmd!(ctx.shell, "sudo test -f {path}"))? {
        return Ok(None);
    }
    Ok(Some(ctx.read_bytes(cmd!(ctx.shell, "sudo cat {path}"))?))
}

/// Replaces a root-owned file with `sudo install`, without recording it. The callers
/// record the change themselves or undo a recorded one.
pub(crate) fn install_system_file(
    ctx: &Context,
    path: &Path,
    contents: &[u8],
    mode: &str,
) -> ModuleResult {
    let staged = temporary_path("system-file");
    fs::write(&staged, contents)?;
    let result = ctx.run(cmd!(ctx.shell, "sudo install -m {mode} {staged} {path}"));
    let cleanup = fs::remove_file(&staged);
    result?;
    Ok(cleanup?)
}

/// Replaces a root-owned file when `edit` changes it, and reports whether it did.
pub(crate) fn edit_system_file(
    ctx: &Context,
    path: &str,
    edit: impl Fn(&str) -> String,
) -> ModuleResult<Option<bool>> {
    let Some(current) = read_system_file(ctx, path)? else {
        return Ok(None);
    };
    let edited = edit(&current);
    if edited == current {
        return Ok(Some(false));
    }
    ctx.write_system_file(Path::new(path), edited.as_bytes(), "0644")?;
    Ok(Some(true))
}

/// Reloads PATH from the stored machine and user values, which a winget install changes
/// without changing this session's copy.
pub(crate) fn refresh_windows_path(ctx: &Context) -> ModuleResult {
    let path = powershell(
        ctx,
        "[Environment]::GetEnvironmentVariable('Path', 'Machine') + ';' + [Environment]::GetEnvironmentVariable('Path', 'User')",
    )?;
    ctx.shell.set_var("PATH", path);
    Ok(())
}
