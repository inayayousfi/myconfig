use std::{error::Error, fmt, fs, path::PathBuf};

use package_catalog::Package;
use xshell::{Shell, cmd};

use crate::{PackageSystem, PackageTool, find_program, resolve_package};

#[derive(Debug)]
pub enum PackageInstallError {
    MissingMapping(Package, PackageSystem),
    Command(xshell::Error),
    Io(std::io::Error),
}

impl fmt::Display for PackageInstallError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingMapping(package, system) => {
                write!(f, "no package mapping for {package:?} on {system:?}")
            }
            Self::Command(error) => write!(f, "{error}"),
            Self::Io(error) => write!(f, "{error}"),
        }
    }
}

impl Error for PackageInstallError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::MissingMapping(_, _) => None,
            Self::Command(error) => Some(error),
            Self::Io(error) => Some(error),
        }
    }
}

impl From<xshell::Error> for PackageInstallError {
    fn from(error: xshell::Error) -> Self {
        Self::Command(error)
    }
}

impl From<std::io::Error> for PackageInstallError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

pub fn install_packages(
    sh: &Shell,
    system: PackageSystem,
    packages: &[Package],
) -> Result<(), PackageInstallError> {
    // Resolve everything first, so a missing mapping cannot cause a partial install.
    let specs = packages
        .iter()
        .map(|package| {
            resolve_package(*package, system)
                .ok_or(PackageInstallError::MissingMapping(*package, system))
        })
        .collect::<Result<Vec<_>, _>>()?;

    let official: Vec<_> = specs
        .iter()
        .filter(|spec| spec.tool == PackageTool::Pacman)
        .map(|spec| spec.name)
        .collect();
    let aur: Vec<_> = specs
        .iter()
        .filter(|spec| spec.tool == PackageTool::Paru)
        .map(|spec| spec.name)
        .collect();
    let apt: Vec<_> = specs
        .iter()
        .filter(|spec| spec.tool == PackageTool::Apt)
        .map(|spec| spec.name)
        .collect();
    let winget: Vec<_> = specs
        .iter()
        .filter(|spec| spec.tool == PackageTool::Winget)
        .map(|spec| spec.name)
        .collect();
    if !official.is_empty() {
        cmd!(sh, "sudo pacman -S --needed --noconfirm {official...}").run()?;
    }
    if !aur.is_empty() {
        ensure_paru(sh)?;
        cmd!(sh, "paru -S --needed --noconfirm --skipreview {aur...}").run()?;
    }
    if !apt.is_empty() {
        cmd!(sh, "sudo apt-get install -y {apt...}").run()?;
    }
    if !winget.is_empty() {
        import_winget_packages(sh, &winget)?;
    }
    Ok(())
}

fn import_winget_packages(sh: &Shell, names: &[&str]) -> Result<(), PackageInstallError> {
    let mut packages = String::new();
    for (index, name) in names.iter().enumerate() {
        if name.is_empty()
            || !name
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b"._-".contains(&byte))
        {
            return Err(PackageInstallError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("invalid Winget package identifier: {name}"),
            )));
        }
        if index > 0 {
            packages.push(',');
        }
        packages.push_str(&format!("{{\"PackageIdentifier\":\"{name}\"}}"));
    }
    let manifest = format!(
        "{{\"$schema\":\"https://aka.ms/winget-packages.schema.2.0.json\",\"Sources\":[{{\"Packages\":[{packages}],\"SourceDetails\":{{\"Argument\":\"https://cdn.winget.microsoft.com/cache\",\"Identifier\":\"Microsoft.Winget.Source_8wekyb3d8bbwe\",\"Name\":\"winget\",\"Type\":\"Microsoft.PreIndexed.Package\"}}}}]}}"
    );
    let path = std::env::temp_dir().join(format!(
        "myconfig-winget-{}-{}.json",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|error| PackageInstallError::Io(std::io::Error::other(error)))?
            .as_nanos()
    ));
    use std::io::Write;
    let mut file = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&path)?;
    file.write_all(manifest.as_bytes())?;
    file.sync_all()?;
    drop(file);
    let result = cmd!(sh, "winget import -i {path} --accept-source-agreements --accept-package-agreements --ignore-unavailable").run();
    match (result, fs::remove_file(&path)) {
        (Ok(()), Ok(())) => Ok(()),
        (Err(error), Ok(())) => Err(error.into()),
        (Ok(()), Err(error)) => Err(error.into()),
        (Err(error), Err(cleanup)) => Err(PackageInstallError::Io(std::io::Error::other(format!(
            "Winget import failed: {error}; could not remove {}: {cleanup}",
            path.display()
        )))),
    }
}

fn ensure_paru(sh: &Shell) -> Result<(), PackageInstallError> {
    if find_program(sh, "paru").is_ok() {
        return Ok(());
    }
    cmd!(
        sh,
        "sudo pacman -S --needed --noconfirm base-devel git rustup"
    )
    .run()?;
    if !cmd!(sh, "cargo --version")
        .quiet()
        .ignore_status()
        .output()
        .is_ok_and(|output| output.status.success())
    {
        cmd!(sh, "rustup default stable").run()?;
    }

    let root = std::env::temp_dir().join(format!(
        "myconfig-paru-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock precedes Unix epoch")
            .as_nanos()
    ));
    fs::create_dir(&root)?;
    let result = (|| -> Result<(), PackageInstallError> {
        let repo = root.join("paru");
        cmd!(sh, "git clone https://aur.archlinux.org/paru.git {repo}").run()?;
        let _cwd = sh.push_dir(&repo);
        cmd!(sh, "makepkg --noconfirm").run()?;
        let package_paths: Vec<PathBuf> = cmd!(sh, "makepkg --packagelist")
            .read()?
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
        if package_paths.is_empty() {
            return Err(PackageInstallError::Io(std::io::Error::other(
                "makepkg did not produce a package",
            )));
        }
        cmd!(sh, "sudo pacman -U --needed --noconfirm {package_paths...}").run()?;
        Ok(())
    })();
    match (result, fs::remove_dir_all(&root)) {
        (Ok(()), Ok(())) => Ok(()),
        (Err(error), Ok(())) => Err(error),
        (Ok(()), Err(cleanup)) => Err(PackageInstallError::Io(cleanup)),
        (Err(error), Err(cleanup)) => Err(PackageInstallError::Io(std::io::Error::other(format!(
            "{error}; could not remove {}: {cleanup}",
            root.display()
        )))),
    }
}

pub fn remove_arch_packages(sh: &Shell, packages: &[Package]) -> Result<(), PackageInstallError> {
    let specs = packages
        .iter()
        .map(|package| {
            resolve_package(*package, PackageSystem::Arch).ok_or(
                PackageInstallError::MissingMapping(*package, PackageSystem::Arch),
            )
        })
        .collect::<Result<Vec<_>, _>>()?;

    let mut installed = Vec::new();
    for spec in specs {
        let name = spec.name;
        if cmd!(sh, "pacman -Q {name}")
            .quiet()
            .ignore_status()
            .output()?
            .status
            .success()
        {
            installed.push(name);
        }
    }
    if !installed.is_empty() {
        cmd!(sh, "sudo pacman -R --noconfirm {installed...}").run()?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_mapping_is_reported_before_any_command_runs() {
        let sh = Shell::new().unwrap();
        let error = install_packages(&sh, PackageSystem::Winget, &[Package::Git, Package::Yazi])
            .unwrap_err();
        assert!(matches!(
            error,
            PackageInstallError::MissingMapping(Package::Yazi, PackageSystem::Winget)
        ));
    }
}
