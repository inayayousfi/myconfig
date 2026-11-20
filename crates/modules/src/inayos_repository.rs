//! The InayOS package repository: pacman trusts the InayOS signing key, reads the
//! `[inayos]` repository from `/etc/pacman.d/inayos.conf`, and installs the `inayos-keyring`
//! and `inayos` packages from it, so `pacman -Syu` updates InayOS with the rest of the
//! system.
//!
//! The InayOS installer installs both packages while it builds the system. On a running
//! system without the keyring package, the module first trusts the key that this binary
//! carries from `packaging/inayos-keyring`, because pacman refuses the repository's
//! packages until it trusts their signer.
use std::{fs, path::Path};

use embedded_dotfiles::{INAYOS_KEY, INAYOS_TRUSTED};
use xshell::cmd;

use crate::{
    Context, Footprint, Module, ModuleResult, Need, Package,
    support::{edit_system_file, read_system_file, verify_packages, with_temporary_directory},
};

pub struct InayosRepository;

impl InayosRepository {
    pub const NAME: &'static str = "inayos-repository";
}

/// The keyring first: pacman checks the signature of `inayos` against it.
const PACKAGES: &[Package] = &[Package::InayosKeyring, Package::Inayos];
const PACMAN_CONF: &str = "/etc/pacman.conf";
const REPOSITORY_CONFIG: &str = "/etc/pacman.d/inayos.conf";
const INCLUDE: &str = "Include = /etc/pacman.d/inayos.conf";
/// GitHub sends each file name under this address to the file of the latest release.
const SERVER: &str = "https://github.com/inayayousfi/myconfig/releases/latest/download";
/// The trusted fingerprints that the keyring package installs.
const TRUSTED: &str = "/usr/share/pacman/keyrings/inayos-trusted";
/// The mark of the committed keyring files while the InayOS signing key does not exist.
const PLACEHOLDER: &str = "INAYOS-PLACEHOLDER";
/// Deletes each key it is given that pacman's keyring holds, so it also succeeds when the
/// keys are already gone.
const DELETE_KEYS: &str = r#"for key; do sudo pacman-key --list-keys "$key" >/dev/null 2>&1 || continue; sudo pacman-key --delete "$key" || exit 1; done"#;

/// `/etc/pacman.d/inayos.conf`. Database and packages both need a signature from a trusted
/// key.
///
/// When `on` is false, the file names no repository: a `[inayos]` section that pacman cannot
/// read or check, because the releases are not signed with the key that pacman trusts or no
/// release publishes the repository yet, would make every `pacman -Syu` fail, Arch updates
/// included.
fn repository_config(on: bool) -> String {
    if on {
        format!(
            "# The InayOS package repository, written by the inayos-repository module.\n\
             [inayos]\n\
             SigLevel = Required\n\
             Server = {SERVER}\n"
        )
    } else {
        "# The InayOS package repository, written by the inayos-repository module.\n\
         # It stays off: no InayOS release publishes a repository signed with the key that\n\
         # this system trusts. A later `inayos install` turns it on once one does.\n"
            .to_owned()
    }
}

/// Whether `conf` has the include line outside a comment.
fn includes_repository(conf: &str) -> bool {
    conf.lines().any(|line| line.trim() == INCLUDE)
}

/// `conf` with the include line at its end, where it follows every Arch repository, so a
/// package name that Arch also uses comes from Arch.
fn with_include(conf: &str) -> String {
    if includes_repository(conf) {
        return conf.to_owned();
    }
    let mut conf = conf.to_owned();
    if !conf.is_empty() && !conf.ends_with('\n') {
        conf.push('\n');
    }
    conf.push_str("\n# The InayOS package repository, added by the inayos-repository module.\n");
    conf.push_str(INCLUDE);
    conf.push('\n');
    conf
}

/// The fingerprints of an `inayos-trusted` file, whose lines are `<fingerprint>:<trust>:`.
/// `None` when the file is the placeholder or names no key.
fn fingerprints(trusted: &str) -> Option<Vec<String>> {
    if trusted.contains(PLACEHOLDER) {
        return None;
    }
    let keys: Vec<String> = trusted
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .filter_map(|line| line.split(':').next())
        .map(str::to_owned)
        .collect();
    (!keys.is_empty()).then_some(keys)
}

/// Whether pacman's keyring lacks one of `keys`.
fn keys_missing(ctx: &Context, keys: &[String]) -> ModuleResult<bool> {
    for key in keys {
        if !ctx.succeeds(cmd!(ctx.shell, "sudo pacman-key --list-keys {key}"))? {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Records the undo that deletes `keys` from pacman's keyring, before they are added.
fn record_key_removal(ctx: &Context, keys: &[String]) -> ModuleResult {
    let mut args = vec!["-c", DELETE_KEYS, "sh"];
    args.extend(keys.iter().map(String::as_str));
    ctx.record_undo_command(Path::new("/bin/sh"), &args)
}

/// Adds and locally signs the key this binary carries, so pacman accepts the keyring
/// package. Fails before any change when this binary carries the placeholder.
fn trust_embedded_key(ctx: &Context) -> ModuleResult {
    let Some(keys) = fingerprints(INAYOS_TRUSTED) else {
        return Err(
            "this inayos binary carries no InayOS signing key, so pacman could not trust the \
             InayOS repository; install inayos-keyring first, or use a released inayos"
                .into(),
        );
    };
    if !keys_missing(ctx, &keys)? {
        return Ok(());
    }
    record_key_removal(ctx, &keys)?;
    with_temporary_directory("inayos-key", |folder| {
        let key = folder.join("inayos.gpg");
        fs::write(&key, INAYOS_KEY)?;
        ctx.run(cmd!(ctx.shell, "sudo pacman-key --add {key}"))?;
        for fingerprint in &keys {
            ctx.run(cmd!(ctx.shell, "sudo pacman-key --lsign-key {fingerprint}"))?;
        }
        Ok(())
    })
}

/// Whether the releases are signed with the key that pacman trusts here: this binary
/// carries a real key, and the installed keyring package, when there is one, names the same
/// one. A system installed from a test build trusts a test key instead, and neither can read
/// the GitHub repository.
fn release_signed(ctx: &Context) -> ModuleResult<bool> {
    Ok(signed_with_trusted_key(
        INAYOS_TRUSTED,
        read_system_file(ctx, TRUSTED)?.as_deref(),
    ))
}

/// Whether the latest release holds the repository database. Before the first release that
/// publishes it, GitHub answers 404.
fn published(ctx: &Context) -> ModuleResult<bool> {
    let sh = ctx.shell;
    ctx.succeeds(cmd!(
        sh,
        "curl --fail --silent --location --max-time 30 --output /dev/null {SERVER}/inayos.db"
    ))
}

/// `release_signed` for the `inayos-trusted` text this binary carries and the installed one.
fn signed_with_trusted_key(carried: &str, installed: Option<&str>) -> bool {
    let Some(mut released) = fingerprints(carried) else {
        return false;
    };
    let Some(installed) = installed else {
        return true;
    };
    let Some(mut installed) = fingerprints(installed) else {
        return false;
    };
    released.sort();
    installed.sort();
    released == installed
}

/// The fingerprints of the installed keyring package.
fn installed_keys(ctx: &Context) -> ModuleResult<Vec<String>> {
    let trusted = read_system_file(ctx, TRUSTED)?
        .ok_or_else(|| format!("{TRUSTED} is missing, so inayos-keyring is incomplete"))?;
    fingerprints(&trusted).ok_or_else(|| format!("{TRUSTED} names no InayOS signing key").into())
}

impl Module for InayosRepository {
    fn name(&self) -> &'static str {
        Self::NAME
    }

    fn needs(&self) -> &'static [Need] {
        &[Need::Hard("base")]
    }

    fn footprint(&self, _ctx: &Context) -> Footprint {
        Footprint {
            packages: PACKAGES.to_vec(),
            settings: Vec::new(),
        }
    }

    fn install(&self, ctx: &Context) -> ModuleResult {
        let sh = ctx.shell;
        let manager = ctx.packages();
        let mut missing = false;
        for (_, spec) in ctx.specs(PACKAGES)? {
            missing |= !manager.installed(ctx, spec)?;
        }
        // Trusted before pacman reads the repository: with the repository configured and
        // its signer unknown, every pacman refresh would fail.
        let keyring_spec = ctx.specs(&[Package::InayosKeyring])?[0].1;
        if !manager.installed(ctx, keyring_spec)? {
            trust_embedded_key(ctx)?;
        }
        let off = if !release_signed(ctx)? {
            Some("the InayOS releases are not signed with the key that this system trusts")
        } else if !published(ctx)? {
            Some("no InayOS release publishes the repository yet, or it cannot be reached")
        } else {
            None
        };
        if let Some(reason) = off {
            if missing {
                return Err(format!("{reason}, so pacman cannot install inayos from it").into());
            }
            ctx.note(&format!(
                "{reason}, so pacman does not read the InayOS repository"
            ));
        }
        ctx.write_system_file(
            Path::new(REPOSITORY_CONFIG),
            repository_config(off.is_none()).as_bytes(),
            "0644",
        )?;
        if edit_system_file(ctx, PACMAN_CONF, with_include)?.is_none() {
            return Err(format!("{PACMAN_CONF} is missing").into());
        }
        if missing {
            // pacman knows the packages of a repository only once it read its database.
            manager.refresh(ctx)?;
        }
        ctx.install_packages(PACKAGES)?;
        let keys = installed_keys(ctx)?;
        if keys_missing(ctx, &keys)? {
            record_key_removal(ctx, &keys)?;
            ctx.run(cmd!(sh, "sudo pacman-key --populate inayos"))?;
        }
        Ok(())
    }

    fn verify(&self, ctx: &Context) -> ModuleResult {
        verify_packages(ctx, PACKAGES)?;
        // Whether the repository is on also depends on the network at install time, so
        // either file passes once the release key is trusted.
        let written = read_system_file(ctx, REPOSITORY_CONFIG)?;
        let written = written.as_deref();
        let expected = written == Some(repository_config(false).as_str())
            || (release_signed(ctx)? && written == Some(repository_config(true).as_str()));
        if !expected {
            return Err(format!("{REPOSITORY_CONFIG} does not hold the InayOS repository").into());
        }
        let conf = read_system_file(ctx, PACMAN_CONF)?.unwrap_or_default();
        if !includes_repository(&conf) {
            return Err(format!("{PACMAN_CONF} lacks `{INCLUDE}`").into());
        }
        if keys_missing(ctx, &installed_keys(ctx)?)? {
            return Err(
                "pacman's keyring lacks the InayOS signing key; run `sudo pacman-key --populate inayos`"
                    .into(),
            );
        }
        Ok(())
    }

    fn remove(&self, _ctx: &Context) -> ModuleResult {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_include_follows_the_arch_repositories_once() {
        let shipped = "[core]\nInclude = /etc/pacman.d/mirrorlist\n\n[extra]\nInclude = /etc/pacman.d/mirrorlist";
        let edited = with_include(shipped);
        assert!(edited.starts_with(shipped));
        assert!(edited.trim_end().ends_with(INCLUDE));
        assert_eq!(with_include(&edited), edited);
    }

    #[test]
    fn a_commented_include_does_not_count() {
        let conf = format!("[core]\n#{INCLUDE}\n");
        assert!(!includes_repository(&conf));
        assert!(includes_repository(&with_include(&conf)));
    }

    #[test]
    fn an_unsigned_release_leaves_pacman_without_the_repository() {
        assert!(repository_config(true).contains("[inayos]"));
        assert!(repository_config(true).contains(SERVER));
        let off = repository_config(false);
        assert!(off.lines().all(|line| line.starts_with('#')));
    }

    #[test]
    fn the_repository_is_on_only_with_the_release_key_trusted() {
        let release = "ABCD1234:4:\n";
        let placeholder = "INAYOS-PLACEHOLDER: no key yet\n";
        assert!(signed_with_trusted_key(release, None));
        assert!(signed_with_trusted_key(release, Some("ABCD1234:4:\n")));
        assert!(!signed_with_trusted_key(release, Some("TEST5678:4:\n")));
        assert!(!signed_with_trusted_key(release, Some(placeholder)));
        assert!(!signed_with_trusted_key(placeholder, None));
        assert!(!signed_with_trusted_key(placeholder, Some(release)));
    }

    #[test]
    fn the_trusted_file_gives_its_fingerprints_and_the_placeholder_none() {
        assert_eq!(
            fingerprints("ABCD1234:4:\n\nEF567890:4:\n"),
            Some(vec!["ABCD1234".to_owned(), "EF567890".to_owned()])
        );
        assert_eq!(
            fingerprints("INAYOS-PLACEHOLDER: no key yet\n<fingerprint>:4:\n"),
            None
        );
        assert_eq!(fingerprints("\n"), None);
    }
}
