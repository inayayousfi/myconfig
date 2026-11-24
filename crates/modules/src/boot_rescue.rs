//! The boot rescue: a system service that watches each boot until a graphical session is
//! active on seat0, restarts the login screen when it runs without a session, and opens
//! Claude Code on tty9 with the boot's context when the boot does not get there. Its user,
//! the one who installs it, runs sudo without a password, so Claude can repair the system
//! from that console.
//!
//! The service and its program are under `dotfiles/assets/boot-rescue`, installed at the
//! same path under `/`. `docs/boot-rescue.md` describes when the console opens.
use std::path::{Path, PathBuf};

use embedded_dotfiles::BOOT_RESCUE_ASSETS;
use typed_fs_rs::{EmbeddedDirectory, EmbeddedFile};
use xshell::cmd;

use crate::{
    Context, Footprint, Module, ModuleResult, Need, Package, ServiceScope,
    support::{current_user, read_system_bytes, verify_packages},
};

pub struct BootRescue;

impl BootRescue {
    pub const NAME: &'static str = "boot-rescue";
}

/// The program checks that Anthropic's API answers with curl and waits for the network
/// with NetworkManager's `nm-online`.
const PACKAGES: &[Package] = &[Package::Curl, Package::Networkmanager];
const SERVICE: &str = "inayos-boot-rescue.service";
/// The embedded files that are programs.
const PROGRAM_FOLDER: &str = "usr/local/libexec/";
/// Names the user the console runs as when the login screen recorded none.
const CONFIG: &str = "/etc/inayos-boot-rescue.conf";
/// Lets that user run sudo without a password.
const SUDO_RULE: &str = "/etc/sudoers.d/20-inayos-boot-rescue";

/// Each embedded file with the absolute path it is installed at and its mode.
fn files() -> Vec<(PathBuf, &'static EmbeddedFile, &'static str)> {
    BOOT_RESCUE_ASSETS
        .files()
        .into_iter()
        .map(|file| {
            let mode = if file.path_from_root.starts_with(PROGRAM_FOLDER) {
                "0755"
            } else {
                "0644"
            };
            (Path::new("/").join(file.path_from_root), file, mode)
        })
        .collect()
}

fn config(user: &str) -> String {
    format!("RESCUE_USER={user}\n")
}

fn sudo_rule(user: &str) -> String {
    format!("{user} ALL=(ALL:ALL) NOPASSWD: ALL\n")
}

/// The files the module writes for `user`, with their contents and modes.
fn generated(user: &str) -> [(&'static str, String, &'static str); 2] {
    [
        (CONFIG, config(user), "0644"),
        (SUDO_RULE, sudo_rule(user), "0440"),
    ]
}

fn is_current(ctx: &Context, path: &Path, content: &[u8]) -> ModuleResult<bool> {
    let path = path.to_str().ok_or("a boot rescue path is not UTF-8")?;
    Ok(read_system_bytes(ctx, path)?.as_deref() == Some(content))
}

impl Module for BootRescue {
    fn name(&self) -> &'static str {
        Self::NAME
    }

    /// The service restarts plasmalogin, which kde-plasma installs and enables.
    fn needs(&self) -> &'static [Need] {
        &[Need::Hard("base"), Need::Hard("kde-plasma")]
    }

    fn footprint(&self, _ctx: &Context) -> Footprint {
        Footprint {
            packages: PACKAGES.to_vec(),
            settings: Vec::new(),
        }
    }

    fn install(&self, ctx: &Context) -> ModuleResult {
        let sh = ctx.shell;
        let user = current_user(ctx)?;
        // A sudoers file that sudo cannot parse stops every later sudo, this installer's
        // included, so visudo checks the rule before it is written.
        let rule = sudo_rule(&user);
        ctx.run_with_input(cmd!(sh, "visudo -cqf -"), rule.as_bytes())
            .map_err(|error| format!("visudo refused the sudo rule for {user}: {error}"))?;
        ctx.install_packages(PACKAGES)?;
        for (path, file, mode) in files() {
            ctx.write_system_file(&path, file.content, mode)?;
        }
        for (path, content, mode) in generated(&user) {
            ctx.write_system_file(Path::new(path), content.as_bytes(), mode)?;
        }
        // It starts with the next boot: this boot already has its session.
        ctx.enable_service(SERVICE, ServiceScope::System)
    }

    fn verify(&self, ctx: &Context) -> ModuleResult {
        let user = current_user(ctx)?;
        verify_packages(ctx, PACKAGES)?;
        for (path, file, _) in files() {
            if !is_current(ctx, &path, file.content)? {
                return Err(format!(
                    "{} does not hold the InayOS boot rescue file",
                    path.display()
                )
                .into());
            }
        }
        for (path, content, _) in generated(&user) {
            if !is_current(ctx, Path::new(path), content.as_bytes())? {
                return Err(format!("{path} does not name {user}").into());
            }
        }
        if !ctx.service_state(SERVICE, ServiceScope::System)?.enabled {
            return Err(format!("{SERVICE} is not enabled").into());
        }
        Ok(())
    }

    fn remove(&self, _ctx: &Context) -> ModuleResult {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{generated, sudo_rule};

    #[test]
    fn the_rule_lets_the_user_run_every_command_without_a_password() {
        assert_eq!(sudo_rule("iy"), "iy ALL=(ALL:ALL) NOPASSWD: ALL\n");
    }

    #[test]
    fn only_root_reads_the_sudo_rule() {
        let rule = generated("iy")
            .into_iter()
            .find(|(path, _, _)| path.starts_with("/etc/sudoers.d/"))
            .unwrap();
        assert_eq!(rule.2, "0440");
    }
}
