//! Arch: pacman for official packages, and paru for AUR packages.
use std::path::PathBuf;

use myconfig_utils::{PackageSpec, PackageTool};
use xshell::cmd;

use super::{PackageManager, foreign};
use crate::{Context, ModuleResult, support::with_temporary_directory};

pub(super) struct Pacman;

impl PackageManager for Pacman {
    fn refresh(&self, ctx: &Context) -> ModuleResult {
        ctx.run(cmd!(ctx.shell, "sudo pacman -Syu --noconfirm"))
    }

    fn installed(&self, ctx: &Context, spec: PackageSpec) -> ModuleResult<bool> {
        let name = spec.name;
        ctx.succeeds(cmd!(ctx.shell, "pacman -Q {name}"))
    }

    fn install(&self, ctx: &Context, specs: &[PackageSpec]) -> ModuleResult {
        let sh = ctx.shell;
        let mut official = Vec::new();
        let mut aur = Vec::new();
        for spec in specs {
            match spec.tool {
                PackageTool::Pacman => official.push(spec.name),
                PackageTool::Paru => aur.push(spec.name),
                PackageTool::Apt | PackageTool::Winget => return Err(foreign(*spec)),
            }
        }
        if !official.is_empty() {
            ctx.run(cmd!(
                sh,
                "sudo pacman -S --needed --noconfirm {official...}"
            ))?;
        }
        if !aur.is_empty() {
            ensure_paru(ctx)?;
            ctx.run(cmd!(
                sh,
                "paru -S --needed --noconfirm --skipreview --removemake {aur...}"
            ))?;
        }
        Ok(())
    }

    fn uninstall(&self, ctx: &Context, specs: &[PackageSpec]) -> ModuleResult {
        let names = pacman_names(specs)?;
        if names.is_empty() {
            return Ok(());
        }
        ctx.run(cmd!(ctx.shell, "sudo pacman -Rs --noconfirm {names...}"))
    }

    fn uninstall_with_dependents(&self, ctx: &Context, spec: PackageSpec) -> ModuleResult {
        let names = pacman_names(&[spec])?;
        ctx.run(cmd!(ctx.shell, "sudo pacman -Rsc --noconfirm {names...}"))
    }

    /// pacman keeps an edited configuration file as `<file>.pacsave` when it uninstalls the
    /// package, and a reinstall writes the packaged default instead.
    fn restore_saved_configuration(&self, ctx: &Context, spec: PackageSpec) -> ModuleResult {
        let sh = ctx.shell;
        let name = spec.name;
        let info = ctx.read(cmd!(sh, "pacman -Qii {name}"))?;
        for file in backup_files(&info) {
            let saved = format!("{file}.pacsave");
            if ctx.succeeds(cmd!(sh, "sudo test -e {saved}"))? {
                ctx.run(cmd!(sh, "sudo mv -f {saved} {file}"))?;
            }
        }
        Ok(())
    }

    fn dependents(&self, ctx: &Context, spec: PackageSpec) -> ModuleResult<Vec<String>> {
        let name = spec.name;
        let info = ctx.read(cmd!(ctx.shell, "pacman -Qi {name}"))?;
        Ok(info
            .lines()
            .find_map(|line| line.strip_prefix("Required By"))
            .and_then(|line| line.split_once(':'))
            .map(|(_, names)| {
                names
                    .split_whitespace()
                    .filter(|name| *name != "None")
                    .map(str::to_owned)
                    .collect()
            })
            .unwrap_or_default())
    }

    fn running_kernel(&self, ctx: &Context) -> ModuleResult<Option<String>> {
        let sh = ctx.shell;
        let release = ctx.read(cmd!(sh, "uname -r"))?;
        let image = format!("/usr/lib/modules/{release}/vmlinuz");
        Ok(match ctx.read_unchecked(cmd!(sh, "pacman -Qqo {image}"))? {
            (true, name) if !name.is_empty() => Some(name),
            _ => None,
        })
    }

    fn uninstall_local(&self, ctx: &Context, name: &str) -> ModuleResult {
        let sh = ctx.shell;
        if ctx.succeeds(cmd!(sh, "pacman -Q {name}"))? {
            ctx.run(cmd!(sh, "sudo pacman -Rs --noconfirm {name}"))?;
        }
        Ok(())
    }
}

/// The configuration files that `pacman -Qii` lists under "Backup Files", one per line,
/// each followed by its state in brackets.
fn backup_files(info: &str) -> Vec<String> {
    let mut files = Vec::new();
    let mut lines = info.lines();
    let Some(first) = lines.find_map(|line| line.strip_prefix("Backup Files")) else {
        return files;
    };
    let first = first.split_once(':').map_or("", |(_, rest)| rest);
    for entry in std::iter::once(first).chain(lines.take_while(|line| line.starts_with(' '))) {
        let path = entry.trim().split(" [").next().unwrap_or_default();
        if path.starts_with('/') {
            files.push(path.to_owned());
        }
    }
    files
}

/// The names of `specs`, which pacman removes whether they came from pacman or the AUR.
fn pacman_names(specs: &[PackageSpec]) -> ModuleResult<Vec<&'static str>> {
    let mut names = Vec::new();
    for spec in specs {
        match spec.tool {
            PackageTool::Pacman | PackageTool::Paru => names.push(spec.name),
            PackageTool::Apt | PackageTool::Winget => return Err(foreign(*spec)),
        }
    }
    Ok(names)
}

/// Installs paru for the first AUR package, recorded under the module that needs it.
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
