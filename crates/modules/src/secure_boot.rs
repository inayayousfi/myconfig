//! Secure Boot with the machine's own keys: sbctl creates the keys, signs rEFInd, its
//! drivers, the tools rEFInd shows, every kernel in /boot and every unified kernel image in
//! `EFI/Linux`, then enrolls the keys with
//! Microsoft's keys and the firmware's built-in keys while the firmware is in Setup Mode.
//! sbctl's own pacman hook signs the saved files again after each update.
//!
//! Enrolling writes into the firmware and cannot be undone, so a running system asks
//! first. In installer mode the module leaves the enrollment to the InayOS installer, which
//! enrolls once `inayos install` has passed, so a failed install never leaves a firmware
//! that refuses the unsigned installer stick. On a running encrypted system, the boot guard
//! first gives the volumes a TPM slot that does not depend on the keys, so the disk still
//! opens at the next boot.
use std::path::{Path, PathBuf};

use serde::Deserialize;
use xshell::cmd;

use crate::{
    Applied, Context, Encryption, Footprint, Module, ModuleResult, Need, Package,
    refind::refind_config, support::verify_packages,
};

pub struct SecureBoot;

impl SecureBoot {
    pub const NAME: &'static str = "secure-boot";
}

const PACKAGES: &[Package] = &[Package::Sbctl];
/// Where the refind package keeps the files that refind-install copies onto the EFI
/// system partition, with the same layout as `EFI/refind`.
const REFIND_SOURCE: &str = "/usr/share/refind";
/// What the module needs when sbctl has no keys yet, and what it skips without it.
const NO_SETUP_MODE: &str = "a firmware in Setup Mode, or Secure Boot keys of this machine,";
const SKIPPED: &str =
    "Secure Boot (turn on Setup Mode in the firmware settings, then install again)";
/// The question asked before the keys go into the firmware.
const ENROLL_QUESTION: &str = "Enroll this machine's Secure Boot keys into the firmware, with Microsoft's and the firmware's own keys? From the next boot on, the firmware starts only files signed with them, and InayOS cannot take them out of the firmware again.";
/// The step as installer mode shows it, deferred to the InayOS installer.
const ENROLL_STEP: &str = "enrolling the Secure Boot keys into the firmware, which the InayOS installer does once the install has passed";
/// Takes a file out of sbctl's list of files to sign again after updates. It does nothing
/// when the file is not in the list, or when sbctl is gone.
const FORGET: &str = r#"if sudo sbctl list-files --json 2>/dev/null | grep -qF "\"$1\""; then sudo sbctl remove-file "$1"; fi"#;

/// The fields of `sbctl status --json` that the module reads.
#[derive(Deserialize)]
struct Status {
    /// Whether sbctl has keys.
    installed: bool,
    setup_mode: bool,
    secure_boot: bool,
}

/// One file of `sbctl list-files --json`.
#[derive(Deserialize)]
struct Saved {
    file: String,
}

/// One file of `sbctl verify --json`.
#[derive(Deserialize)]
struct Verified {
    file_name: String,
    /// 1 when signed with the machine's key, 0 when not, -1 when the file is missing.
    is_signed: i8,
}

/// A file to sign: sbctl signs `source` into `output`, which are the same path when the
/// file is signed in place.
struct Target {
    source: PathBuf,
    output: PathBuf,
}

/// The volumes of an encrypted system, which the InayOS installer writes.
const CRYPTTAB: &str = "/etc/crypttab.initramfs";
/// The boot guard, which adds the TPM slot that survives a change of the keys.
const GUARD: &str = "/usr/local/bin/inayos-guard";

/// The firmware's built-in signature database. Some firmware, OVMF among them, has none, and
/// `sbctl enroll-keys --firmware-builtin` then fails.
const FIRMWARE_DEFAULT_DB: &str =
    "/sys/firmware/efi/efivars/dbDefault-8be4df61-93ca-11d2-aa0d-00e098032b8c";

fn status(ctx: &Context) -> ModuleResult<Status> {
    let json = ctx.read(cmd!(ctx.shell, "sudo sbctl status --json"))?;
    serde_json::from_str(&json)
        .map_err(|error| format!("could not read `sbctl status --json`: {error}: {json}").into())
}

/// The sources of the files that sbctl signs again after updates. It is empty before
/// sbctl has keys, when `list-files` fails.
fn saved_files(ctx: &Context) -> ModuleResult<Vec<String>> {
    let (listed, json) = ctx.read_unchecked(cmd!(ctx.shell, "sudo sbctl list-files --json"))?;
    if !listed {
        return Ok(Vec::new());
    }
    let saved: Vec<Saved> = serde_json::from_str(&json)
        .map_err(|error| format!("could not read `sbctl list-files --json`: {error}"))?;
    Ok(saved.into_iter().map(|saved| saved.file).collect())
}

/// The EFI files under `folder` on the EFI system partition, relative to it.
fn efi_files(ctx: &Context, folder: &Path) -> ModuleResult<Vec<PathBuf>> {
    let sh = ctx.shell;
    if !ctx.succeeds(cmd!(sh, "sudo test -d {folder}"))? {
        return Ok(Vec::new());
    }
    let listed = ctx.read(cmd!(
        sh,
        "sudo find {folder} -type f -iname *.efi -printf %P\\n"
    ))?;
    let mut files: Vec<PathBuf> = listed.lines().map(PathBuf::from).collect();
    files.sort();
    Ok(files)
}

/// Every file that the firmware or rEFInd starts: rEFInd and its drivers, signed from the
/// package's copies so a rEFInd update signs them again, the tools in `EFI/tools`, the
/// unified kernel images in `EFI/Linux`, and the kernels in /boot, signed in place.
fn targets(ctx: &Context, refind: &Path) -> ModuleResult<Vec<Target>> {
    let sh = ctx.shell;
    let mut targets = Vec::new();
    for relative in efi_files(ctx, refind)? {
        let output = refind.join(&relative);
        let shipped = Path::new(REFIND_SOURCE).join(&relative);
        let source = if ctx.succeeds(cmd!(sh, "sudo test -f {shipped}"))? {
            shipped
        } else {
            output.clone()
        };
        targets.push(Target { source, output });
    }
    let tools = refind
        .parent()
        .ok_or("the rEFInd folder has no parent folder")?
        .join("tools");
    // sbctl's mkinitcpio hook signs a unified kernel image again each time mkinitcpio
    // builds it.
    let images = tools.with_file_name("Linux");
    for folder in [tools, images] {
        for relative in efi_files(ctx, &folder)? {
            let path = folder.join(relative);
            targets.push(Target {
                source: path.clone(),
                output: path,
            });
        }
    }
    let kernels = ctx.read(cmd!(
        sh,
        "sudo find /boot/ -maxdepth 1 -type f -name vmlinuz-* -printf %f\\n"
    ))?;
    let mut kernels: Vec<&str> = kernels.lines().collect();
    kernels.sort();
    if kernels.is_empty() {
        return Err("/boot holds no kernel to sign".into());
    }
    for kernel in kernels {
        let path = Path::new("/boot").join(kernel);
        targets.push(Target {
            source: path.clone(),
            output: path,
        });
    }
    Ok(targets)
}

/// The folder that holds rEFInd on the EFI system partition.
fn refind_folder(ctx: &Context) -> ModuleResult<Option<PathBuf>> {
    Ok(refind_config(ctx)?.and_then(|config| config.parent().map(Path::to_path_buf)))
}

/// Signs each target and saves it in sbctl's list, so its pacman hook signs it again.
/// Each new entry in the list is recorded first, so remove takes it out.
fn sign(ctx: &Context, targets: &[Target]) -> ModuleResult {
    let sh = ctx.shell;
    let saved = saved_files(ctx)?;
    for Target { source, output } in targets {
        let name = source.to_str().ok_or("a path to sign is not UTF-8")?;
        if !saved.iter().any(|file| file == name) {
            ctx.record_undo_command(Path::new("/bin/sh"), &["-c", FORGET, "sh", name])?;
        }
        if source == output {
            ctx.run(cmd!(sh, "sudo sbctl sign -s {output}"))?;
        } else {
            ctx.run(cmd!(sh, "sudo sbctl sign -s -o {output} {source}"))?;
        }
    }
    Ok(())
}

/// Fails unless sbctl reports every target signed with the machine's key.
fn verify_signed(ctx: &Context, targets: &[Target]) -> ModuleResult {
    let outputs: Vec<&Path> = targets
        .iter()
        .map(|target| target.output.as_path())
        .collect();
    let arguments = outputs.clone();
    let json = ctx.read(cmd!(ctx.shell, "sudo sbctl verify --json {arguments...}"))?;
    let verified: Vec<Verified> = serde_json::from_str(&json)
        .map_err(|error| format!("could not read `sbctl verify --json`: {error}: {json}"))?;
    let unsigned: Vec<String> = outputs
        .iter()
        .filter(|output| {
            !verified
                .iter()
                .any(|file| Path::new(&file.file_name) == **output && file.is_signed == 1)
        })
        .map(|output| output.display().to_string())
        .collect();
    if !unsigned.is_empty() {
        return Err(format!(
            "these boot files are not signed with this machine's Secure Boot key: {}",
            unsigned.join(", ")
        )
        .into());
    }
    Ok(())
}

impl Module for SecureBoot {
    fn name(&self) -> &'static str {
        Self::NAME
    }

    fn needs(&self) -> &'static [Need] {
        &[Need::Hard("refind"), Need::Soft(Encryption::NAME)]
    }

    fn footprint(&self, _ctx: &Context) -> Footprint {
        Footprint {
            packages: PACKAGES.to_vec(),
            settings: Vec::new(),
        }
    }

    /// The keys and their enrollment are not recorded: remove cannot take keys out of the
    /// firmware, and deleting the keys would leave it trusting keys that nothing holds.
    fn install(&self, ctx: &Context) -> ModuleResult {
        let sh = ctx.shell;
        let Some(refind) = refind_folder(ctx)? else {
            return ctx.missing_part("a rEFInd installation", "Secure Boot");
        };
        ctx.install_packages(PACKAGES)?;
        let before = status(ctx)?;
        if !before.setup_mode && !before.installed {
            return ctx.missing_part(NO_SETUP_MODE, SKIPPED);
        }
        let mut enroll = false;
        if before.setup_mode {
            let asked = ctx.on_running_system(ENROLL_STEP, || {
                enroll = ctx.confirm(ENROLL_QUESTION)?;
                Ok(())
            })?;
            if asked == Applied::Now && !enroll {
                return ctx.record_declined();
            }
        }
        ctx.record_accepted()?;
        if !before.installed {
            ctx.run(cmd!(sh, "sudo sbctl create-keys"))?;
        }
        let targets = targets(ctx, &refind)?;
        sign(ctx, &targets)?;
        verify_signed(ctx, &targets)?;
        // Last, because the firmware keeps the keys: from the next boot on it starts only
        // files signed with them, and every boot file is signed by now.
        if enroll {
            // New keys change the Secure Boot state that the TPM checks before it opens
            // an encrypted disk.
            if ctx.succeeds(cmd!(sh, "sudo test -e {CRYPTTAB}"))? {
                if !ctx.succeeds(cmd!(sh, "sudo test -x {GUARD}"))? {
                    return Err(format!("{GUARD} is missing, and without its TPM slot the encrypted disk would ask for the recovery key after the keys change; install the boot-guard module first").into());
                }
                ctx.run(cmd!(sh, "sudo {GUARD} prepare-firmware-update"))?;
            }
            if ctx.succeeds(cmd!(sh, "sudo test -e {FIRMWARE_DEFAULT_DB}"))? {
                ctx.run(cmd!(
                    sh,
                    "sudo sbctl enroll-keys --microsoft --firmware-builtin"
                ))?;
            } else {
                ctx.run(cmd!(sh, "sudo sbctl enroll-keys --microsoft"))?;
            }
        }
        Ok(())
    }

    fn verify(&self, ctx: &Context) -> ModuleResult {
        if ctx.declined() {
            return Ok(());
        }
        let Some(refind) = refind_folder(ctx)? else {
            return ctx.missing_part("a rEFInd installation", "the Secure Boot check");
        };
        verify_packages(ctx, PACKAGES)?;
        let status = status(ctx)?;
        if !status.installed && !status.setup_mode {
            return ctx.missing_part(NO_SETUP_MODE, "the Secure Boot check");
        }
        if !status.installed {
            return Err("sbctl has no Secure Boot keys".into());
        }
        verify_signed(ctx, &targets(ctx, &refind)?)?;
        // In installer mode the InayOS installer enrolls after this check, and checks the
        // firmware itself.
        if status.setup_mode {
            let _ = ctx.on_running_system("checking that the firmware holds the keys", || {
                Err("the firmware is still in Setup Mode, so it holds no Secure Boot keys".into())
            })?;
        }
        Ok(())
    }

    /// Refuses while Secure Boot is on: without sbctl and its list, the next kernel
    /// update would leave an unsigned kernel that the firmware refuses to start.
    fn remove(&self, ctx: &Context) -> ModuleResult {
        if ctx.find_program("sbctl").is_err() {
            return Ok(());
        }
        if status(ctx)?.secure_boot {
            return Err("removing secure-boot while Secure Boot is on would leave the next kernel update unsigned, and the machine would not start it. Turn off Secure Boot in the firmware settings first; nothing was removed".into());
        }
        Ok(())
    }
}
