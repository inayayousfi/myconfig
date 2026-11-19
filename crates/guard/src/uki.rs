//! The unified kernel images of an encrypted system, on the EFI system partition because
//! rEFInd cannot read the encrypted system partition: the normal one that mkinitcpio
//! builds, `EFI/Linux/inayos-linux.efi`, and the "Previous system" one the guard builds
//! from a snapshot, `EFI/inayos/previous.efi`.
//!
//! A unified kernel image is a PE file whose sections hold the kernel (`.linux`), the
//! initramfs (`.initrd`) and the kernel options (`.cmdline`). The guard reads them from
//! the PE section table itself, so it needs neither `ukify inspect` nor `objcopy`.
use std::path::{Path, PathBuf};

use crate::{
    boot_files::option_value,
    checks::{has_modules, kernel_version, signature_failures},
    encryption::Volume,
    refind_conf,
    snapshots::PreviousSystem,
    system::{System, read_text},
};

/// The normal image, from the top of the EFI system partition.
pub const NORMAL: &str = "EFI/Linux/inayos-linux.efi";
/// The "Previous system" image, from the top of the EFI system partition.
pub const PREVIOUS: &str = "EFI/inayos/previous.efi";
/// The kernel options mkinitcpio puts in the normal image.
pub const KERNEL_CMDLINE: &str = "/etc/kernel/cmdline";
/// Where the guard puts the normal image's initramfs while `lsinitcpio` lists it.
const WORK_DIR: &str = "/run/inayos-guard";

fn u16_at(bytes: &[u8], offset: usize) -> Option<usize> {
    Some(u16::from_le_bytes(bytes.get(offset..offset + 2)?.try_into().ok()?) as usize)
}

fn u32_at(bytes: &[u8], offset: usize) -> Option<usize> {
    Some(u32::from_le_bytes(bytes.get(offset..offset + 4)?.try_into().ok()?) as usize)
}

/// The contents of the section `name` of the PE file `image`, such as `.linux`. The PE
/// header starts at the offset stored at 0x3C, with the signature `PE\0\0`; the COFF
/// header follows with the section count at +2 and the optional header's size at +16; the
/// section table follows the optional header, 40 bytes per section: the name in 8 bytes,
/// then the virtual size at +8, the size on disk at +16 and the file offset at +20.
pub fn section<'a>(image: &'a [u8], name: &str) -> Option<&'a [u8]> {
    let pe = u32_at(image, 0x3C)?;
    if image.get(pe..pe + 4)? != b"PE\0\0" {
        return None;
    }
    let coff = pe + 4;
    let count = u16_at(image, coff + 2)?;
    let table = coff + 20 + u16_at(image, coff + 16)?;
    (0..count).find_map(|index| {
        let header = image.get(table + 40 * index..table + 40 * (index + 1))?;
        let end = header[..8].iter().position(|b| *b == 0).unwrap_or(8);
        if &header[..end] != name.as_bytes() {
            return None;
        }
        let virtual_size = u32_at(header, 8)?;
        let raw_size = u32_at(header, 16)?;
        let offset = u32_at(header, 20)?;
        // The size on disk is rounded up to the file alignment; the virtual size is exact.
        let size = if virtual_size == 0 {
            raw_size
        } else {
            virtual_size.min(raw_size)
        };
        image.get(offset..offset + size)
    })
}

/// Checks that `root=` in the kernel options `cmdline`, read from `source`, opens one of
/// `volumes`: `/dev/mapper/<name>` with `name` listed in crypttab.initramfs.
pub fn root_failure(cmdline: &str, source: &str, volumes: &[Volume]) -> Option<String> {
    let Some(root) = option_value(cmdline, "root") else {
        return Some(format!("{source} has no root= option"));
    };
    let listed = root
        .strip_prefix("/dev/mapper/")
        .is_some_and(|name| volumes.iter().any(|volume| volume.name == name));
    (!listed).then(|| {
        format!(
            "{source} boots root={root}, which is not a volume that {} opens",
            crate::encryption::CRYPTTAB
        )
    })
}

/// The kernel options of `/etc/kernel/cmdline` on one line, or the failure.
pub fn kernel_cmdline(system: &dyn System) -> Result<String, String> {
    read_text(system, Path::new(KERNEL_CMDLINE))
        .map(|text| text.split_whitespace().collect::<Vec<_>>().join(" "))
        .map_err(|error| format!("{KERNEL_CMDLINE} cannot be read: {error}"))
}

/// Checks the normal image `image`: its kernel is a Linux kernel, its initramfs holds
/// that kernel's modules, and its kernel options open an encrypted root. Also checks
/// `/etc/kernel/cmdline`, which mkinitcpio puts in the next image.
pub fn normal_failures(system: &dyn System, image: &Path, volumes: &[Volume]) -> Vec<String> {
    let shown = image.display();
    let mut failures: Vec<String> = match kernel_cmdline(system) {
        Ok(cmdline) => root_failure(&cmdline, KERNEL_CMDLINE, volumes)
            .into_iter()
            .collect(),
        Err(failure) => vec![failure],
    };
    let bytes = match system.read(image) {
        Ok(bytes) => bytes,
        Err(error) => {
            failures.push(format!(
                "{shown} cannot be read, so rEFInd cannot start InayOS: {error}"
            ));
            return failures;
        }
    };
    let Some(linux) = section(&bytes, ".linux") else {
        failures.push(format!(
            "{shown} is not a unified kernel image with a kernel"
        ));
        return failures;
    };
    let Some(version) = kernel_version(linux) else {
        failures.push(format!(
            "the kernel inside {shown} is not a Linux kernel the guard can read"
        ));
        return failures;
    };
    match section(&bytes, ".cmdline") {
        Some(cmdline) => failures.extend(root_failure(
            String::from_utf8_lossy(cmdline).trim_end_matches('\0'),
            &format!("the kernel options inside {shown}"),
            volumes,
        )),
        None => failures.push(format!("{shown} holds no kernel options")),
    }
    match section(&bytes, ".initrd") {
        Some(initrd) => failures.extend(initrd_failure(system, image, initrd, &version)),
        None => failures.push(format!("{shown} holds no initramfs")),
    }
    failures
}

/// Checks that `initrd`, the initramfs inside `image`, holds the modules of `version`.
/// `lsinitcpio` reads a file, so the initramfs goes to a file under /run for the time of
/// the check.
fn initrd_failure(
    system: &dyn System,
    image: &Path,
    initrd: &[u8],
    version: &str,
) -> Option<String> {
    let shown = image.display();
    let work = Path::new(WORK_DIR);
    let file = work.join("initrd.img");
    if let Err(error) = system
        .create_dir_all(work)
        .and_then(|()| system.write_atomic(&file, initrd, 0o600))
    {
        return Some(format!(
            "the initramfs inside {shown} cannot be checked: {} cannot be written: {error}",
            file.display()
        ));
    }
    let path = file.to_string_lossy();
    let failure = match system.run("lsinitcpio", &[path.as_ref()]) {
        Ok(output) if output.success() => (!has_modules(&output.stdout, version)).then(|| {
            format!("the initramfs inside {shown} holds no kernel modules of {version}, the version of its kernel")
        }),
        Ok(output) => Some(format!(
            "lsinitcpio could not list the initramfs inside {shown}: {}",
            output.stderr.trim()
        )),
        Err(error) => Some(format!(
            "lsinitcpio could not run to list the initramfs inside {shown}: {error}"
        )),
    };
    let _ = system.remove_file(&file);
    failure
}

/// The `ukify build` arguments of the "Previous system" image for `previous`: the
/// snapshot's kernel, its microcode then its initramfs, and `cmdline`.
pub fn build_args(previous: &PreviousSystem, cmdline: &str, output: &Path) -> Vec<String> {
    let boot = previous.kernel_path();
    let boot = boot.parent().unwrap_or(Path::new("/"));
    let mut args = vec![
        "build".to_owned(),
        format!("--linux={}", previous.kernel_path().display()),
    ];
    args.extend(
        previous
            .microcode
            .iter()
            .map(|file| format!("--initrd={}", boot.join(file).display())),
    );
    args.push(format!("--initrd={}", previous.initramfs_path().display()));
    args.push(format!("--cmdline={cmdline}"));
    args.push(format!("--output={}", output.display()));
    args
}

/// Builds the "Previous system" image `output` for `previous`, with the normal kernel
/// options `normal` turned into the snapshot's, and signs it when `signed` is true. The
/// image is built and signed under a temporary name and renamed at the end, so `output`
/// is either the earlier image or a complete new one. Returns the failure in plain words.
pub fn build_previous(
    system: &dyn System,
    previous: &PreviousSystem,
    normal: &str,
    output: &Path,
    signed: bool,
) -> Result<(), String> {
    let cmdline = refind_conf::previous_cmdline(previous, normal)
        .ok_or_else(|| format!("{KERNEL_CMDLINE} has no root= option"))?;
    let building: PathBuf = output.with_extension("efi.new");
    if let Some(folder) = output.parent() {
        system
            .create_dir_all(folder)
            .map_err(|error| format!("{} cannot be created: {error}", folder.display()))?;
    }
    let result = build_and_sign(system, previous, &cmdline, &building, signed).and_then(|()| {
        system.rename(&building, output).map_err(|error| {
            format!(
                "{} cannot replace {}: {error}",
                building.display(),
                output.display()
            )
        })
    });
    if result.is_err() {
        let _ = system.remove_file(&building);
    }
    result
}

fn build_and_sign(
    system: &dyn System,
    previous: &PreviousSystem,
    cmdline: &str,
    building: &Path,
    signed: bool,
) -> Result<(), String> {
    let args = build_args(previous, cmdline, building);
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    match system.run("ukify", &args) {
        Ok(output) if output.success() => {}
        Ok(output) => {
            return Err(format!(
                "ukify could not build it: {}",
                output.stderr.trim()
            ));
        }
        Err(error) => return Err(format!("ukify could not run: {error}")),
    }
    if !signed {
        return Ok(());
    }
    let path = building.to_string_lossy();
    match system.run("sbctl", &["sign", path.as_ref()]) {
        Ok(output) if output.success() => {}
        Ok(output) => return Err(format!("sbctl could not sign it: {}", output.stderr.trim())),
        Err(error) => return Err(format!("sbctl could not run to sign it: {error}")),
    }
    let failures = signature_failures(system, &[building.to_owned()]);
    if failures.is_empty() {
        Ok(())
    } else {
        Err(failures.join("; "))
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::fake::{FakeSystem, kernel_image, ok};

    /// A PE file with the sections `sections`, laid out as a unified kernel image.
    pub fn image(sections: &[(&str, &[u8])]) -> Vec<u8> {
        let pe = 0x80usize;
        let optional = 0xF0usize;
        let table = pe + 24 + optional;
        let mut data_at = table + 40 * sections.len();
        data_at = data_at.div_ceil(0x200) * 0x200;
        let mut bytes = vec![0u8; data_at];
        bytes[0..2].copy_from_slice(b"MZ");
        bytes[0x3C..0x40].copy_from_slice(&(pe as u32).to_le_bytes());
        bytes[pe..pe + 4].copy_from_slice(b"PE\0\0");
        bytes[pe + 6..pe + 8].copy_from_slice(&(sections.len() as u16).to_le_bytes());
        bytes[pe + 20..pe + 22].copy_from_slice(&(optional as u16).to_le_bytes());
        for (index, (name, contents)) in sections.iter().enumerate() {
            let header = table + 40 * index;
            let raw = contents.len().div_ceil(0x200) * 0x200;
            bytes[header..header + name.len()].copy_from_slice(name.as_bytes());
            bytes[header + 8..header + 12].copy_from_slice(&(contents.len() as u32).to_le_bytes());
            bytes[header + 16..header + 20].copy_from_slice(&(raw as u32).to_le_bytes());
            let start = bytes.len();
            bytes[header + 20..header + 24].copy_from_slice(&(start as u32).to_le_bytes());
            bytes.extend_from_slice(contents);
            bytes.resize(start + raw, 0);
        }
        bytes
    }

    fn volumes() -> Vec<Volume> {
        crate::encryption::parse_crypttab("system UUID=1 none\nhome UUID=2 none\n")
    }

    #[test]
    fn section_reads_each_section_at_its_exact_size() {
        let uki = image(&[
            (".osrel", b"ID=arch\n"),
            (".cmdline", b"root=/dev/mapper/system rw\0"),
        ]);
        assert_eq!(section(&uki, ".osrel"), Some(&b"ID=arch\n"[..]));
        assert_eq!(
            section(&uki, ".cmdline"),
            Some(&b"root=/dev/mapper/system rw\0"[..])
        );
        assert_eq!(section(&uki, ".linux"), None);
        assert_eq!(
            section(b"not a PE file at all, far too short", ".linux"),
            None
        );
    }

    #[test]
    fn the_root_must_be_a_volume_of_crypttab() {
        let volumes = volumes();
        assert_eq!(
            root_failure("root=/dev/mapper/system rw", "x", &volumes),
            None
        );
        assert_eq!(
            root_failure("root=/dev/mapper/other rw", "/etc/kernel/cmdline", &volumes).as_deref(),
            Some(
                "/etc/kernel/cmdline boots root=/dev/mapper/other, which is not a volume that /etc/crypttab.initramfs opens"
            )
        );
        assert_eq!(
            root_failure("rw quiet", "x", &volumes).as_deref(),
            Some("x has no root= option")
        );
    }

    #[test]
    fn a_healthy_normal_image_passes_and_leaves_no_work_file() {
        let system = FakeSystem::default();
        let cmdline = "root=/dev/mapper/system rootflags=subvol=@ rw quiet";
        system.file(KERNEL_CMDLINE, format!("{cmdline}\n").as_bytes());
        let uki = image(&[
            (".cmdline", cmdline.as_bytes()),
            (".linux", &kernel_image("7.3.4-arch1-2")),
            (".initrd", b"cpio"),
        ]);
        system.file("/efi/EFI/Linux/inayos-linux.efi", &uki);
        system.respond(
            "lsinitcpio /run/inayos-guard/initrd.img",
            ok("usr/lib/modules/7.3.4-arch1-2/kernel/drivers/block/virtio_blk.ko.zst\n"),
        );
        let failures = normal_failures(
            &system,
            Path::new("/efi/EFI/Linux/inayos-linux.efi"),
            &volumes(),
        );
        assert_eq!(failures, Vec::<String>::new());
        assert!(!system.exists(Path::new("/run/inayos-guard/initrd.img")));
    }

    #[test]
    fn an_initramfs_without_modules_fails_the_normal_image() {
        let system = FakeSystem::default();
        system.file(KERNEL_CMDLINE, b"root=/dev/mapper/system rw\n");
        let uki = image(&[
            (".cmdline", b"root=/dev/mapper/system rw"),
            (".linux", &kernel_image("7.3.4-arch1-2")),
            (".initrd", b"cpio"),
        ]);
        system.file("/efi/EFI/Linux/inayos-linux.efi", &uki);
        system.respond("lsinitcpio /run/inayos-guard/initrd.img", ok("usr/\n"));
        let failures = normal_failures(
            &system,
            Path::new("/efi/EFI/Linux/inayos-linux.efi"),
            &volumes(),
        );
        assert_eq!(
            failures,
            [
                "the initramfs inside /efi/EFI/Linux/inayos-linux.efi holds no kernel modules of 7.3.4-arch1-2, the version of its kernel"
            ]
        );
    }

    #[test]
    fn build_args_put_the_microcode_before_the_initramfs() {
        let previous = PreviousSystem {
            number: 7,
            description: String::new(),
            kernel: "vmlinuz-linux".to_owned(),
            initramfs: "initramfs-linux.img".to_owned(),
            microcode: vec!["intel-ucode.img".to_owned()],
        };
        assert_eq!(
            build_args(
                &previous,
                "root=/dev/mapper/system rw",
                Path::new("/efi/EFI/inayos/previous.efi.new")
            ),
            [
                "build",
                "--linux=/.snapshots/7/snapshot/boot/vmlinuz-linux",
                "--initrd=/.snapshots/7/snapshot/boot/intel-ucode.img",
                "--initrd=/.snapshots/7/snapshot/boot/initramfs-linux.img",
                "--cmdline=root=/dev/mapper/system rw",
                "--output=/efi/EFI/inayos/previous.efi.new",
            ]
        );
    }
}
