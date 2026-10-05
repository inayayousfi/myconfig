//! Helpers shared by several modules and by the runner.
use std::{
    fs,
    path::{Path, PathBuf},
};

use myconfig_utils::{PackageSpec, PackageTool};
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
        if !ctx.package_installed(spec)? {
            return Err(format!("package {} ({package:?}) is not installed", spec.name).into());
        }
    }
    Ok(())
}

pub(crate) fn verify_absent(ctx: &Context, packages: &[crate::Package]) -> ModuleResult {
    for (package, spec) in ctx.specs(packages)? {
        if ctx.package_installed(spec)? {
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

pub(crate) fn active_group(ctx: &Context, wanted: &str) -> ModuleResult<bool> {
    let groups = ctx.read(cmd!(ctx.shell, "id -Gn"))?;
    Ok(groups.split_whitespace().any(|group| group == wanted))
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
        ctx.set(
            Setting::GroupExists {
                group: group.to_owned(),
            },
            "present",
        )?;
        ctx.set(
            Setting::GroupMember {
                group: group.to_owned(),
                user: user.clone(),
            },
            "member",
        )?;
    }
    ctx.run(cmd!(sh, "sudo modprobe uinput"))?;
    ctx.write_system_file(
        &PathBuf::from(format!("/etc/modules-load.d/myconfig-{owner}.conf")),
        b"uinput\n",
        "0644",
    )?;
    ctx.write_system_file(
        &PathBuf::from(format!("/etc/udev/rules.d/99-myconfig-{owner}.rules")),
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
        format!("/etc/modules-load.d/myconfig-{owner}.conf"),
        format!("/etc/udev/rules.d/99-myconfig-{owner}.rules"),
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

/// Installed packages that depend on `spec`, as the package manager reports them.
pub(crate) fn dependents(ctx: &Context, spec: PackageSpec) -> ModuleResult<Vec<String>> {
    let sh = ctx.shell;
    let name = spec.name;
    Ok(match spec.tool {
        PackageTool::Pacman | PackageTool::Paru => {
            let info = ctx.read(cmd!(sh, "pacman -Qi {name}"))?;
            info.lines()
                .find_map(|line| line.strip_prefix("Required By"))
                .and_then(|line| line.split_once(':'))
                .map(|(_, names)| {
                    names
                        .split_whitespace()
                        .filter(|name| *name != "None")
                        .map(str::to_owned)
                        .collect()
                })
                .unwrap_or_default()
        }
        PackageTool::Apt => {
            let output = ctx.read(cmd!(sh, "apt-cache rdepends --installed {name}"))?;
            output
                .lines()
                .skip_while(|line| !line.starts_with("Reverse Depends:"))
                .skip(1)
                .map(|line| line.trim().trim_start_matches('|').to_owned())
                .filter(|line| !line.is_empty() && line != name)
                .collect()
        }
        PackageTool::Winget => Vec::new(),
    })
}

/// The package that owns the running kernel, so `remove` never uninstalls it.
pub(crate) fn running_kernel_package(ctx: &Context) -> ModuleResult<Option<String>> {
    let sh = ctx.shell;
    let owner = match ctx.package_system {
        crate::PackageSystem::Arch => {
            let release = ctx.read(cmd!(sh, "uname -r"))?;
            let image = format!("/usr/lib/modules/{release}/vmlinuz");
            ctx.read_unchecked(cmd!(sh, "pacman -Qqo {image}"))?
        }
        crate::PackageSystem::Apt => {
            let release = ctx.read(cmd!(sh, "uname -r"))?;
            let image = format!("/boot/vmlinuz-{release}");
            let (found, output) = ctx.read_unchecked(cmd!(sh, "dpkg -S {image}"))?;
            (
                found,
                output.split(':').next().unwrap_or_default().to_owned(),
            )
        }
        crate::PackageSystem::Winget => return Ok(None),
    };
    Ok(match owner {
        (true, name) if !name.is_empty() => Some(name),
        _ => None,
    })
}

pub(crate) fn ensure_paru(ctx: &Context) -> ModuleResult {
    let sh = ctx.shell;
    for installed in ["paru", "paru-bin", "paru-git"] {
        if ctx.succeeds(cmd!(sh, "pacman -Q {installed}"))? {
            return Ok(());
        }
    }
    // CachyOS ships paru in its own repository; plain Arch has it only in the AUR.
    if ctx.succeeds(cmd!(sh, "pacman -Si paru"))? {
        return ctx.install_packages(&[crate::Package::Paru]);
    }
    // paru-bin packages paru's official release binary, so nothing is compiled.
    ctx.run(cmd!(
        sh,
        "sudo pacman -S --needed --noconfirm base-devel git"
    ))?;
    with_temporary_directory("paru", |root| {
        let repo = root.join("paru-bin");
        ctx.run(cmd!(
            sh,
            "git clone https://aur.archlinux.org/paru-bin.git {repo}"
        ))?;
        let _cwd = sh.push_dir(&repo);
        ctx.run(cmd!(sh, "makepkg --noconfirm"))?;
        let packages: Vec<PathBuf> = ctx
            .read(cmd!(sh, "makepkg --packagelist"))?
            .lines()
            .map(PathBuf::from)
            .map(|path| {
                if path.is_absolute() {
                    path
                } else {
                    repo.join(path)
                }
            })
            .collect();
        if packages.is_empty() {
            return Err("makepkg did not produce a package".into());
        }
        ctx.local_package_installed("paru-bin")?;
        ctx.run(cmd!(
            sh,
            "sudo pacman -U --needed --noconfirm {packages...}"
        ))
    })
}

pub(crate) fn import_winget_packages(ctx: &Context, names: &[&str]) -> ModuleResult {
    let mut packages = Vec::new();
    for name in names {
        if name.is_empty()
            || !name
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b"._-".contains(&byte))
        {
            return Err(format!("invalid Winget package identifier: {name}").into());
        }
        packages.push(format!("{{\"PackageIdentifier\":\"{name}\"}}"));
    }
    let manifest = format!(
        "{{\"$schema\":\"https://aka.ms/winget-packages.schema.2.0.json\",\"Sources\":[{{\"Packages\":[{}],\"SourceDetails\":{{\"Argument\":\"https://cdn.winget.microsoft.com/cache\",\"Identifier\":\"Microsoft.Winget.Source_8wekyb3d8bbwe\",\"Name\":\"winget\",\"Type\":\"Microsoft.PreIndexed.Package\"}}}}]}}",
        packages.join(",")
    );
    with_temporary_directory("winget", |directory| {
        let path = directory.join("packages.json");
        fs::write(&path, manifest)?;
        ctx.run(cmd!(ctx.shell, "winget import -i {path} --accept-source-agreements --accept-package-agreements --ignore-unavailable"))
    })
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

pub(crate) fn startup_directory(ctx: &Context) -> ModuleResult<PathBuf> {
    let startup = powershell(ctx, "[Environment]::GetFolderPath('Startup')")?;
    if startup.is_empty() {
        return Err("Windows Startup directory is unavailable".into());
    }
    Ok(PathBuf::from(startup))
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
