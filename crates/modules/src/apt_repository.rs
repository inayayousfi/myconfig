//! Adds the vendor APT repositories that the package catalog names for a package.
use std::{collections::BTreeMap, path::Path};

use myconfig_utils::PackageSpec;
use package_catalog::AptRepository;
use xshell::cmd;

use crate::{Context, ModuleResult, support::read_system_file};

/// The release code name from `/etc/os-release`.
pub(crate) fn codename() -> ModuleResult<String> {
    std::fs::read_to_string("/etc/os-release")?
        .lines()
        .find_map(|line| line.strip_prefix("VERSION_CODENAME="))
        .map(|value| value.trim_matches('"').to_owned())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "this system does not report a release code name".into())
}

/// Writes a root-owned file, and reports whether its contents changed.
fn write_if_changed(ctx: &Context, path: &str, contents: &[u8]) -> ModuleResult<bool> {
    if read_system_file(ctx, path)?.is_some_and(|current| current.as_bytes() == contents) {
        return Ok(false);
    }
    ctx.write_system_file(Path::new(path), contents, "0644")?;
    Ok(true)
}

fn add(ctx: &Context, repository: &AptRepository, codename: &str) -> ModuleResult<bool> {
    let sh = ctx.shell;
    let id = repository.id;
    let key_url = repository.key_url.replace("{codename}", codename);
    let key = ctx
        .read_bytes(cmd!(sh, "curl -fsSL {key_url}"))
        .map_err(|error| format!("could not download the {id} repository key: {error}"))?;
    // apt reads an ASCII-armored key only from a file named .asc.
    let armored = String::from_utf8_lossy(&key).contains("-----BEGIN PGP PUBLIC KEY BLOCK-----");
    let key_path = format!(
        "/etc/apt/keyrings/myconfig-{id}.{}",
        if armored { "asc" } else { "gpg" }
    );
    let architecture = ctx.read(cmd!(sh, "dpkg --print-architecture"))?;
    let source = format!(
        "Types: deb\nURIs: {}\nSuites: {}\nComponents: {}\nArchitectures: {architecture}\nSigned-By: {key_path}\n",
        repository.uri,
        repository.suite.replace("{codename}", codename),
        repository.component,
    );
    let key_changed = write_if_changed(ctx, &key_path, &key)?;
    let source_path = format!("/etc/apt/sources.list.d/myconfig-{id}.sources");
    let source_changed = write_if_changed(ctx, &source_path, source.as_bytes())?;
    Ok(key_changed || source_changed)
}

/// Adds the vendor repositories that `packages` come from, then rereads the package
/// databases when a repository changed.
pub(crate) fn add_repositories(
    ctx: &Context,
    packages: &[PackageSpec],
    codename: &str,
) -> ModuleResult {
    let repositories: BTreeMap<&str, &AptRepository> = packages
        .iter()
        .filter_map(|spec| spec.apt_repository)
        .map(|repository| (repository.id, repository))
        .collect();
    let mut changed = false;
    for repository in repositories.into_values() {
        changed |= add(ctx, repository, codename)?;
    }
    if changed {
        ctx.run(cmd!(ctx.shell, "sudo apt-get update"))?;
    }
    Ok(())
}
