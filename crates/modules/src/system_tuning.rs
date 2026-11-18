//! The system settings of the InayOS desktop: zram swap with zswap off, networking through
//! NetworkManager with DNS through systemd-resolved and its stub `/etc/resolv.conf`, the
//! swap and console kernel settings, the higher open-file limit, and the `ntsync` module
//! for Wine and Proton.
//!
//! Each setting is a file under `dotfiles/assets/system-tuning`, installed at the same
//! path under `/`.
use std::path::{Path, PathBuf};

use embedded_dotfiles::SYSTEM_TUNING_ASSETS;
use typed_fs_rs::{EmbeddedDirectory, EmbeddedFile};
use xshell::cmd;

use crate::{
    Context, Footprint, Module, ModuleResult, Need, Package, ServiceScope,
    support::{read_system_bytes, verify_packages},
};

pub struct SystemTuning;

const PACKAGES: &[Package] = &[
    Package::ZramGenerator,
    Package::SystemdResolvconf,
    Package::Networkmanager,
];
const RESOLVED: &str = "systemd-resolved.service";
const NETWORK_MANAGER: &str = "NetworkManager.service";
/// NetworkManager reads this drop-in only when it starts or reloads its configuration.
const NETWORK_MANAGER_DNS: &str = "/etc/NetworkManager/conf.d/inayos-system-tuning.conf";
const SYSCTL: &str = "/etc/sysctl.d/99-inayos-system-tuning.conf";
/// Its rule makes `/etc/resolv.conf` a link to resolved's stub file at every boot.
const TMPFILES: &str = "/etc/tmpfiles.d/inayos-system-tuning.conf";
const RESOLV_CONF: &str = "/etc/resolv.conf";
/// Makes a running NetworkManager read its configuration again. It does nothing when
/// NetworkManager is missing or stopped, so it also succeeds once its change is undone.
const RELOAD_NETWORK_MANAGER: &str =
    "if nmcli -t -f RUNNING general >/dev/null 2>&1; then sudo nmcli general reload; fi";

/// Each embedded file with the absolute path it is installed at.
fn files() -> Vec<(PathBuf, &'static EmbeddedFile)> {
    SYSTEM_TUNING_ASSETS
        .files()
        .into_iter()
        .map(|file| (Path::new("/").join(file.path_from_root), file))
        .collect()
}

fn is_current(ctx: &Context, path: &Path, file: &EmbeddedFile) -> ModuleResult<bool> {
    let path = path.to_str().ok_or("a system tuning path is not UTF-8")?;
    Ok(read_system_bytes(ctx, path)?.as_deref() == Some(file.content))
}

impl Module for SystemTuning {
    fn name(&self) -> &'static str {
        "system-tuning"
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
        ctx.install_packages(PACKAGES)?;
        let (dns, others): (Vec<_>, Vec<_>) = files()
            .into_iter()
            .partition(|(path, _)| path == Path::new(NETWORK_MANAGER_DNS));
        for (path, file) in &others {
            // Recorded before the rule, so remove deletes the rule first and then puts
            // back the file that the rule replaces at the next boot.
            if path == Path::new(TMPFILES) && !is_current(ctx, path, file)? {
                ctx.record_system_path(Path::new(RESOLV_CONF))?;
            }
            ctx.write_system_file(path, file.content, "0644")?;
        }
        // resolved runs before NetworkManager hands it the DNS servers, so name lookups
        // never point at a resolver that is not there.
        ctx.enable_service(RESOLVED, ServiceScope::System)?;
        let _ = ctx.start_service(RESOLVED, ServiceScope::System)?;
        for (path, file) in &dns {
            if !is_current(ctx, path, file)? {
                // Recorded before the drop-in, so remove deletes the drop-in first and
                // NetworkManager then writes resolv.conf again before resolved stops.
                ctx.record_undo_command(Path::new("/bin/sh"), &["-c", RELOAD_NETWORK_MANAGER])?;
                ctx.write_system_file(path, file.content, "0644")?;
            }
        }
        // Only enabled: starting it next to another network manager would disturb the
        // running system's network, and the installed system starts it at boot.
        ctx.enable_service(NETWORK_MANAGER, ServiceScope::System)?;
        // zram, zswap, the limits of the service managers and the console level take
        // effect at the next boot. The installed system loads the rest from its files at boot.
        let _ = ctx.on_running_system("apply the sysctl values and load ntsync", || {
            ctx.run(cmd!(sh, "sudo sysctl --load {SYSCTL}"))?;
            ctx.run(cmd!(sh, "sudo modprobe ntsync"))?;
            ctx.run(cmd!(sh, "sh -c {RELOAD_NETWORK_MANAGER}"))
        })?;
        Ok(())
    }

    fn verify(&self, ctx: &Context) -> ModuleResult {
        verify_packages(ctx, PACKAGES)?;
        for (path, file) in files() {
            if !is_current(ctx, &path, file)? {
                return Err(format!("{} does not hold the InayOS settings", path.display()).into());
            }
        }
        let _ = ctx.require_active(RESOLVED, ServiceScope::System)?;
        if !ctx
            .service_state(NETWORK_MANAGER, ServiceScope::System)?
            .enabled
        {
            return Err(format!("{NETWORK_MANAGER} is not enabled").into());
        }
        Ok(())
    }

    fn remove(&self, _ctx: &Context) -> ModuleResult {
        Ok(())
    }
}
