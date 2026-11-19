//! The checks of the boot files. Each check returns its failures in plain words, and an
//! empty list when it passed.
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::{
    boot_files::{BootFiles, Kernel, REFIND_LINUX_CONFIG, option_value},
    snapshots::PreviousSystem,
    system::System,
};

/// The firmware variable that holds 1 when Secure Boot is on. Like every efivarfs file,
/// it starts with 4 bytes of attributes; the last byte is the value.
pub const SECURE_BOOT_VARIABLE: &str =
    "/sys/firmware/efi/efivars/SecureBoot-8be4df61-93ca-11d2-aa0d-00e098032b8c";
pub const LOCKDOWN: &str = "/sys/kernel/security/lockdown";

/// Whether an efivarfs variable's contents say 1.
pub fn variable_is_set(contents: &[u8]) -> bool {
    contents.len() > 4 && contents.last() == Some(&1)
}

/// The fields of `sbctl status --json` that the guard reads.
#[derive(Debug, Default, Deserialize)]
struct SbctlStatus {
    #[serde(default)]
    installed: bool,
    #[serde(default)]
    setup_mode: bool,
    #[serde(default)]
    secure_boot: bool,
}

/// The JSON value in `output`, which can follow lines of other text.
fn json_part(output: &str, open: char) -> &str {
    output.find(open).map_or("", |start| &output[start..])
}

/// Whether sbctl's status says Secure Boot is on, or on from the next boot: sbctl has its
/// keys and the firmware has left Setup Mode, which happens when the keys are enrolled.
fn sbctl_enforces(status: &str) -> bool {
    serde_json::from_str::<SbctlStatus>(json_part(status, '{'))
        .is_ok_and(|status| status.secure_boot || (status.installed && !status.setup_mode))
}

/// Whether the files rEFInd starts must be signed: Secure Boot is on, or the firmware
/// holds sbctl's keys and turns it on at the next boot.
pub fn secure_boot_enforced(system: &dyn System) -> bool {
    if system
        .read(Path::new(SECURE_BOOT_VARIABLE))
        .is_ok_and(|contents| variable_is_set(&contents))
    {
        return true;
    }
    match system.run("sbctl", &["status", "--json"]) {
        Ok(output) if output.success() => sbctl_enforces(&output.stdout),
        _ => false,
    }
}

/// What `sbctl verify --json` says about one file.
#[derive(Debug, PartialEq, Eq)]
pub enum Signature {
    Signed,
    Unsigned,
    Missing,
    /// sbctl's answer does not list the file, for example because it could not read it.
    NotListed,
}

#[derive(Deserialize)]
struct VerifiedFile {
    file_name: String,
    is_signed: i8,
}

/// Reads `sbctl verify --json`: a list of `{"file_name": ..., "is_signed": 1 | 0 | -1}`,
/// where -1 means the file does not exist.
pub fn signature(output: &str, file: &Path) -> Signature {
    let files: Vec<VerifiedFile> = serde_json::from_str(json_part(output, '[')).unwrap_or_default();
    match files
        .iter()
        .find(|entry| Path::new(&entry.file_name) == file)
    {
        Some(entry) if entry.is_signed == 1 => Signature::Signed,
        Some(entry) if entry.is_signed == -1 => Signature::Missing,
        Some(_) => Signature::Unsigned,
        None => Signature::NotListed,
    }
}

/// Checks that sbctl's keys signed each of `files`. Each file is its own sbctl call,
/// because sbctl stops at the first file that is not an EFI binary and prints no answer.
/// Landlock is off because it would hide files outside the EFI system partition, such as
/// the kernels in /boot, that sbctl's database does not list.
pub fn signature_failures(system: &dyn System, files: &[PathBuf]) -> Vec<String> {
    let mut failures = Vec::new();
    for file in files {
        let shown = file.display();
        let path = file.to_string_lossy();
        let output = system.run(
            "sbctl",
            &["verify", "--json", "--disable-landlock", path.as_ref()],
        );
        let failure = match output {
            Err(error) => Some(format!("sbctl could not run to check {shown}: {error}")),
            Ok(output) if !output.success() => Some(format!(
                "sbctl could not check {shown}: {}",
                output.stderr.trim()
            )),
            Ok(output) => match signature(&output.stdout, file) {
                Signature::Signed => None,
                Signature::Unsigned => Some(format!(
                    "{shown} is not signed, so Secure Boot refuses to start it"
                )),
                Signature::Missing => Some(format!("{shown} is missing")),
                Signature::NotListed => Some(format!("sbctl did not report on {shown}")),
            },
        };
        failures.extend(failure);
    }
    failures
}

/// The version of the Linux kernel image `image`, read from its x86 boot header: the
/// "HdrS" signature at 0x202, and at 0x20E the offset from 0x200 of a text that starts
/// with the version.
pub fn kernel_version(image: &[u8]) -> Option<String> {
    if image.get(0x202..0x206)? != b"HdrS" {
        return None;
    }
    let pointer = u16::from_le_bytes([*image.get(0x20E)?, *image.get(0x20F)?]) as usize;
    if pointer == 0 {
        return None;
    }
    let text = image.get(0x200 + pointer..)?;
    let end = text
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(text.len());
    let version = String::from_utf8_lossy(&text[..end])
        .split_whitespace()
        .next()?
        .to_owned();
    Some(version)
}

/// Whether `listing`, the output of `lsinitcpio`, holds a kernel module of `version`.
pub fn has_modules(listing: &str, version: &str) -> bool {
    let folder = format!("usr/lib/modules/{version}/");
    listing.lines().any(|line| {
        let line = line.trim().trim_start_matches("./");
        line.starts_with(&folder) && line.contains(".ko")
    })
}

/// The version of the kernel image at `image`, or the failure in plain words.
fn version_of(system: &dyn System, image: &Path) -> Result<String, String> {
    let shown = image.display();
    let bytes = system
        .read(image)
        .map_err(|error| format!("{shown} cannot be read: {error}"))?;
    kernel_version(&bytes)
        .ok_or_else(|| format!("{shown} is not a Linux kernel the guard can read"))
}

/// Checks that the boot image `initramfs` holds the kernel modules of `version`, the
/// version of the kernel `image`.
fn boot_image_failure(
    system: &dyn System,
    initramfs: &Path,
    version: &str,
    image: &Path,
) -> Option<String> {
    let shown = initramfs.display();
    let path = initramfs.to_string_lossy();
    match system.run("lsinitcpio", &[path.as_ref()]) {
        Ok(output) if output.success() => (!has_modules(&output.stdout, version)).then(|| {
            format!(
                "{shown} holds no kernel modules of {version}, the version of {}",
                image.display()
            )
        }),
        Ok(output) => Some(format!(
            "lsinitcpio could not list {shown}: {}",
            output.stderr.trim()
        )),
        Err(error) => Some(format!("lsinitcpio could not run to list {shown}: {error}")),
    }
}

/// Checks that each kernel has its boot image, and that each boot image holds the kernel
/// modules of the kernel it starts.
pub fn initramfs_failures(system: &dyn System, kernels: &[Kernel]) -> Vec<String> {
    let mut failures = Vec::new();
    if kernels.is_empty() {
        failures.push("/boot holds no kernel".to_owned());
    }
    for kernel in kernels {
        if !kernel.initramfs.contains(&kernel.main_initramfs) {
            failures.push(format!(
                "{} is missing, so {} cannot start",
                kernel.main_initramfs.display(),
                kernel.image.display()
            ));
        }
        let version = match version_of(system, &kernel.image) {
            Ok(version) => version,
            Err(failure) => {
                failures.push(failure);
                continue;
            }
        };
        failures.extend(kernel.initramfs.iter().filter_map(|initramfs| {
            boot_image_failure(system, initramfs, &version, &kernel.image)
        }));
    }
    failures
}

/// Checks that the "Previous system" entry would start `previous`: its boot image holds
/// the modules of its kernel and, when `signed` is true, sbctl's keys signed its kernel.
/// These failures choose the snapshot; they are not failures of the normal entry.
pub fn snapshot_failures(
    system: &dyn System,
    previous: &PreviousSystem,
    signed: bool,
) -> Vec<String> {
    let image = previous.kernel_path();
    let mut failures = match version_of(system, &image) {
        Ok(version) => boot_image_failure(system, &previous.initramfs_path(), &version, &image)
            .into_iter()
            .collect(),
        Err(failure) => vec![failure],
    };
    if signed {
        failures.extend(signature_failures(system, &[image]));
    }
    failures
}

/// The device link udev keeps for `source`, the value of a `root=` option, and the blkid
/// arguments that find it otherwise. None for a value the guard does not know.
fn device_lookup(source: &str) -> Option<(PathBuf, Vec<String>)> {
    if source.starts_with("/dev/") {
        return Some((PathBuf::from(source), Vec::new()));
    }
    let (kind, value) = source.split_once('=')?;
    let (folder, blkid) = match kind {
        "UUID" => ("by-uuid", vec!["-U".to_owned(), value.to_owned()]),
        "LABEL" => ("by-label", vec!["-L".to_owned(), value.to_owned()]),
        "PARTUUID" => ("by-partuuid", vec!["-t".to_owned(), source.to_owned()]),
        "PARTLABEL" => ("by-partlabel", vec!["-t".to_owned(), source.to_owned()]),
        _ => return None,
    };
    Some((Path::new("/dev/disk").join(folder).join(value), blkid))
}

fn device_exists(system: &dyn System, source: &str) -> Option<bool> {
    let (link, blkid) = device_lookup(source)?;
    if system.exists(&link) {
        return Some(true);
    }
    if blkid.is_empty() {
        return Some(false);
    }
    let args: Vec<&str> = blkid.iter().map(String::as_str).collect();
    Some(
        system
            .run("blkid", &args)
            .is_ok_and(|output| output.success()),
    )
}

/// Checks that the `root=` option of every entry in `options` names a disk that exists.
pub fn root_failures(system: &dyn System, options: Option<&[String]>) -> Vec<String> {
    let Some(options) = options else {
        return vec![format!(
            "{REFIND_LINUX_CONFIG} is missing, so rEFInd starts Linux without its root filesystem"
        )];
    };
    if options.is_empty() {
        return vec![format!("{REFIND_LINUX_CONFIG} has no boot entry")];
    }
    let mut failures = Vec::new();
    let mut checked: Vec<&str> = Vec::new();
    for entry in options {
        let Some(root) = option_value(entry, "root") else {
            failures.push(format!(
                "an entry of {REFIND_LINUX_CONFIG} has no root= option: {entry}"
            ));
            continue;
        };
        if checked.contains(&root) {
            continue;
        }
        checked.push(root);
        match device_exists(system, root) {
            Some(true) => {}
            Some(false) => failures.push(format!(
                "{REFIND_LINUX_CONFIG} boots root={root}, a disk that does not exist"
            )),
            None => failures.push(format!(
                "{REFIND_LINUX_CONFIG} boots root={root}, which the guard cannot check"
            )),
        }
    }
    failures
}

/// Checks that rEFInd itself is on the EFI system partition.
pub fn refind_failures(system: &dyn System, boot: &BootFiles) -> Vec<String> {
    let refind = boot.refind_dir.join("refind_x64.efi");
    if system.exists(&refind) {
        Vec::new()
    } else {
        vec![format!("{} is missing", refind.display())]
    }
}

/// Whether `/proc/cmdline` says the system started from a snapshot.
pub fn booted_from_snapshot(cmdline: &str) -> bool {
    cmdline
        .split_whitespace()
        .any(|option| option == "systemd.volatile=overlay")
}

/// The lockdown mode the kernel uses, the bracketed word of
/// `/sys/kernel/security/lockdown`, such as `none` in `[none] integrity confidentiality`.
pub fn lockdown_mode(text: &str) -> Option<&str> {
    text.split_whitespace()
        .find_map(|word| word.strip_prefix('[')?.strip_suffix(']'))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fake::kernel_image;

    #[test]
    fn the_last_byte_of_an_efi_variable_is_its_value() {
        assert!(variable_is_set(&[6, 0, 0, 0, 1]));
        assert!(!variable_is_set(&[6, 0, 0, 0, 0]));
        assert!(!variable_is_set(&[]));
    }

    #[test]
    fn sbctl_status_enforces_once_its_keys_are_enrolled() {
        let status = |installed, setup, secure| {
            format!(
                "{{\"installed\":{installed},\"guid\":\"x\",\"setup_mode\":{setup},\"secure_boot\":{secure},\"vendors\":[],\"firmware_quirks\":[]}}"
            )
        };
        assert!(sbctl_enforces(&status(true, false, true)));
        // Keys enrolled during the install: Secure Boot starts with the next boot.
        assert!(sbctl_enforces(&status(true, false, false)));
        // Firmware still in Setup Mode: nothing is enforced yet.
        assert!(!sbctl_enforces(&status(true, true, false)));
        assert!(!sbctl_enforces(&status(false, false, false)));
    }

    #[test]
    fn signature_reads_sbctl_verify_json() {
        let output = r#"[
  { "file_name": "/efi/EFI/refind/refind_x64.efi", "is_signed": 1 },
  { "file_name": "/boot/vmlinuz-linux", "is_signed": 0 },
  { "file_name": "/efi/EFI/tools/gone.efi", "is_signed": -1 }
]"#;
        let check = |file: &str| signature(output, Path::new(file));
        assert_eq!(check("/efi/EFI/refind/refind_x64.efi"), Signature::Signed);
        assert_eq!(check("/boot/vmlinuz-linux"), Signature::Unsigned);
        assert_eq!(check("/efi/EFI/tools/gone.efi"), Signature::Missing);
        assert_eq!(check("/efi/EFI/other.efi"), Signature::NotListed);
    }

    #[test]
    fn kernel_version_reads_the_boot_header() {
        assert_eq!(
            kernel_version(&kernel_image("7.3.4-arch1-2")).as_deref(),
            Some("7.3.4-arch1-2")
        );
        assert_eq!(kernel_version(&[0u8; 0x600]), None);
    }

    #[test]
    fn a_boot_image_has_modules_only_when_it_holds_a_module_of_that_kernel() {
        let listing = "usr/\nusr/lib/modules/7.3.4-arch1-2/\nusr/lib/modules/7.3.4-arch1-2/modules.dep\nusr/lib/modules/7.3.4-arch1-2/kernel/drivers/block/virtio_blk.ko.zst\n";
        assert!(has_modules(listing, "7.3.4-arch1-2"));
        assert!(!has_modules(listing, "7.4.0-arch1-1"));
        // The folder with only depmod's files holds no module.
        let empty = "usr/lib/modules/7.3.4-arch1-2/\nusr/lib/modules/7.3.4-arch1-2/modules.dep\n";
        assert!(!has_modules(empty, "7.3.4-arch1-2"));
    }

    #[test]
    fn lockdown_mode_is_the_bracketed_word() {
        assert_eq!(
            lockdown_mode("[none] integrity confidentiality\n"),
            Some("none")
        );
        assert_eq!(
            lockdown_mode("none [integrity] confidentiality"),
            Some("integrity")
        );
    }
}
