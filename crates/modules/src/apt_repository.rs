//! Vendor apt repositories for packages that Ubuntu does not carry, or carries only in
//! old versions.
use std::{collections::BTreeSet, path::Path};

use xshell::cmd;

use crate::{Context, ModuleResult, Package, support::read_system_file};

struct AptRepository {
    id: &'static str,
    /// The signing key, binary or ASCII-armored. `{codename}` becomes the release code
    /// name of this Ubuntu.
    key_url: &'static str,
    uri: &'static str,
    suite: &'static str,
    component: &'static str,
}

const TAILSCALE: AptRepository = AptRepository {
    id: "tailscale",
    key_url: "https://pkgs.tailscale.com/stable/ubuntu/{codename}.noarmor.gpg",
    uri: "https://pkgs.tailscale.com/stable/ubuntu",
    suite: "{codename}",
    component: "main",
};
const GITHUB: AptRepository = AptRepository {
    id: "github",
    key_url: "https://cli.github.com/packages/githubcli-archive-keyring.gpg",
    uri: "https://cli.github.com/packages",
    suite: "stable",
    component: "main",
};
const DOCKER: AptRepository = AptRepository {
    id: "docker",
    key_url: "https://download.docker.com/linux/ubuntu/gpg",
    uri: "https://download.docker.com/linux/ubuntu",
    suite: "{codename}",
    component: "stable",
};
const YAZI: AptRepository = AptRepository {
    id: "yazi",
    key_url: "https://yazi-rs.github.io/builds/yazi-keyring.gpg",
    uri: "https://yazi-rs.github.io/builds/",
    suite: "stable",
    component: "main",
};

fn repository(package: Package) -> Option<&'static AptRepository> {
    match package {
        Package::Tailscale => Some(&TAILSCALE),
        Package::GithubCli => Some(&GITHUB),
        Package::Docker
        | Package::DockerBuildx
        | Package::DockerCompose
        | Package::DockerRootlessExtras => Some(&DOCKER),
        Package::Yazi => Some(&YAZI),
        _ => None,
    }
}

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
    packages: &[Package],
    codename: &str,
) -> ModuleResult {
    let ids: BTreeSet<&str> = packages
        .iter()
        .filter_map(|package| repository(*package))
        .map(|repository| repository.id)
        .collect();
    let mut changed = false;
    for repository in [&TAILSCALE, &GITHUB, &DOCKER, &YAZI] {
        if ids.contains(repository.id) {
            changed |= add(ctx, repository, codename)?;
        }
    }
    if changed {
        ctx.run(cmd!(ctx.shell, "sudo apt-get update"))?;
    }
    Ok(())
}
