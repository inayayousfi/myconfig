//! The text of `EFI/refind/inayos-guard.conf`, which rEFInd's configuration includes: the
//! "Previous system" entry and the entry rEFInd starts by default.
//!
//! rEFInd reads a manual entry as a `menuentry "<title>" { ... }` stanza:
//! - `volume` names the filesystem by its label, here the Btrfs system partition;
//! - `loader` is the kernel's path from the top of that filesystem;
//! - `initrd` names one boot image, which rEFInd passes as `initrd=<path>` after the
//!   `options`, so a microcode image goes first as an `initrd=` in `options`;
//! - `options` holds the kernel options in quotes.
//!
//! On an encrypted system the entry has only a `loader`: the guard's unified kernel image
//! on the EFI system partition, which holds the kernel options. A stanza without `volume`
//! loads from the volume rEFInd itself started from.
//!
//! rEFInd titles that entry "Boot Previous system from inayos-system" and an entry it
//! finds by itself "Boot \@\boot\vmlinuz-linux from inayos-system". `default_selection`
//! picks the first entry whose title contains its text.
use crate::{boot_files::option_value, snapshots::PreviousSystem};

pub const FILE_NAME: &str = "inayos-guard.conf";
/// The label of the Btrfs system partition.
pub const VOLUME: &str = "inayos-system";
pub const PREVIOUS_TITLE: &str = "Previous system";

/// The `default_selection` text that picks rEFInd's normal entry for `kernel`, such as
/// `vmlinuz-linux`. "from" follows the file name in the entry's title, so the text does
/// not also pick `vmlinuz-linux-lts`.
pub fn normal_selection(kernel: &str) -> String {
    format!("{kernel} from")
}

/// The `default_selection` text that picks rEFInd's entry for the unified kernel image
/// `image`: its file name, such as `inayos-linux.efi`, which rEFInd's title for the entry
/// holds after the image's path.
pub fn image_selection(image: &std::path::Path) -> String {
    image
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// The entry rEFInd starts by default: the normal entry when every check passed, and the
/// "Previous system" entry when one failed and there is a snapshot to boot.
pub fn default_selection(passed: bool, has_previous: bool, normal: &str) -> String {
    if passed || !has_previous {
        normal.to_owned()
    } else {
        PREVIOUS_TITLE.to_owned()
    }
}

/// The options the normal entry passes that the "Previous system" entry replaces.
fn replaced(option: &str) -> bool {
    let name = option.split('=').next().unwrap_or(option);
    matches!(
        name,
        "root" | "rootflags" | "rw" | "ro" | "initrd" | "systemd.volatile"
    )
}

/// A path inside the snapshot's /boot in the form the kernel reads from `initrd=`.
fn efi_path(previous: &PreviousSystem, file: &str) -> String {
    format!(
        "\\{}\\boot\\{file}",
        previous.subvolume().replace('/', "\\")
    )
}

/// The kernel options that start `previous`: the snapshot's root as a writable overlay
/// over the read-only snapshot, then the other options of the normal entry `normal`, such
/// as `quiet` and the console colors. None when `normal` has no `root=`.
pub fn previous_cmdline(previous: &PreviousSystem, normal: &str) -> Option<String> {
    let root = option_value(normal, "root")?;
    let mut options = vec![format!("root={root}")];
    options.push(format!("rootflags=subvol={}", previous.subvolume()));
    options.push("systemd.volatile=overlay".to_owned());
    options.push("rw".to_owned());
    options.extend(
        normal
            .split_whitespace()
            .filter(|option| !replaced(option))
            .map(str::to_owned),
    );
    Some(options.join(" "))
}

/// The kernel options of the "Previous system" entry that starts the snapshot's kernel
/// from the Btrfs volume: `previous_cmdline`, after the snapshot's microcode as `initrd=`
/// options.
pub fn previous_options(previous: &PreviousSystem, normal: &str) -> Option<String> {
    let mut options: Vec<String> = previous
        .microcode
        .iter()
        .map(|file| format!("initrd={}", efi_path(previous, file)))
        .collect();
    options.push(previous_cmdline(previous, normal)?);
    Some(options.join(" "))
}

/// What the "Previous system" entry starts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Loader {
    /// The snapshot's kernel and boot image on the Btrfs volume, with these kernel options.
    Btrfs { options: String },
    /// The unified kernel image at this path from the top of the EFI system partition.
    Image { path: String },
}

/// The whole file: the "Previous system" stanza when `previous` holds the snapshot and
/// what the entry starts, then `default_selection`.
pub fn render(previous: Option<(&PreviousSystem, &Loader)>, selection: &str) -> String {
    let mut text = String::from(
        "# Written by inayos-guard after each change to the boot files. Edits are overwritten.\n",
    );
    if let Some((previous, loader)) = previous {
        text.push_str(&format!(
            "\n# Snapshot {}: {}\nmenuentry \"{PREVIOUS_TITLE}\" {{\n",
            previous.number,
            previous.description.replace('\n', " "),
        ));
        match loader {
            Loader::Btrfs { options } => text.push_str(&format!(
                "    volume {VOLUME}\n    loader /{}/boot/{}\n    initrd {}\n    options \"{options}\"\n",
                previous.subvolume(),
                previous.kernel,
                efi_path(previous, &previous.initramfs),
            )),
            Loader::Image { path } => text.push_str(&format!("    loader /{path}\n")),
        }
        text.push_str("}\n");
    }
    text.push_str(&format!("\ndefault_selection \"{selection}\"\n"));
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    fn previous(microcode: &[&str]) -> PreviousSystem {
        PreviousSystem {
            number: 12,
            description: "pacman -Syu".to_owned(),
            kernel: "vmlinuz-linux".to_owned(),
            initramfs: "initramfs-linux.img".to_owned(),
            microcode: microcode.iter().map(|m| (*m).to_owned()).collect(),
        }
    }

    #[test]
    fn previous_options_boot_the_snapshot_as_an_overlay_and_keep_the_other_options() {
        let normal = "root=UUID=abc rw rootflags=subvol=@ quiet vt.default_red=0,248";
        assert_eq!(
            previous_options(&previous(&["amd-ucode.img"]), normal).as_deref(),
            Some(
                "initrd=\\@snapshots\\12\\snapshot\\boot\\amd-ucode.img root=UUID=abc rootflags=subvol=@snapshots/12/snapshot systemd.volatile=overlay rw quiet vt.default_red=0,248"
            )
        );
    }

    #[test]
    fn previous_options_need_the_root_of_the_normal_entry() {
        assert_eq!(previous_options(&previous(&[]), "quiet"), None);
    }

    #[test]
    fn a_failed_check_makes_the_previous_system_the_default_only_when_it_exists() {
        let normal = normal_selection("vmlinuz-linux");
        assert_eq!(default_selection(true, true, &normal), "vmlinuz-linux from");
        assert_eq!(default_selection(false, true, &normal), PREVIOUS_TITLE);
        assert_eq!(
            default_selection(false, false, &normal),
            "vmlinuz-linux from"
        );
    }

    #[test]
    fn render_writes_a_manual_stanza_and_the_default() {
        let previous = previous(&[]);
        let loader = Loader::Btrfs {
            options: "root=UUID=abc rw".to_owned(),
        };
        let text = render(Some((&previous, &loader)), "Previous system");
        assert_eq!(
            text,
            "# Written by inayos-guard after each change to the boot files. Edits are overwritten.\n\
             \n\
             # Snapshot 12: pacman -Syu\n\
             menuentry \"Previous system\" {\n    \
             volume inayos-system\n    \
             loader /@snapshots/12/snapshot/boot/vmlinuz-linux\n    \
             initrd \\@snapshots\\12\\snapshot\\boot\\initramfs-linux.img\n    \
             options \"root=UUID=abc rw\"\n\
             }\n\
             \n\
             default_selection \"Previous system\"\n"
        );
    }

    #[test]
    fn render_loads_a_unified_kernel_image_from_the_refind_volume() {
        let previous = previous(&["intel-ucode.img"]);
        let loader = Loader::Image {
            path: "EFI/inayos/previous.efi".to_owned(),
        };
        let text = render(Some((&previous, &loader)), "inayos-linux.efi");
        assert!(
            text.contains(
                "menuentry \"Previous system\" {\n    loader /EFI/inayos/previous.efi\n}\n"
            )
        );
    }

    #[test]
    fn previous_cmdline_leaves_the_microcode_to_the_image() {
        assert_eq!(
            previous_cmdline(
                &previous(&["intel-ucode.img"]),
                "root=/dev/mapper/system rootflags=subvol=@ rw quiet"
            )
            .as_deref(),
            Some(
                "root=/dev/mapper/system rootflags=subvol=@snapshots/12/snapshot systemd.volatile=overlay rw quiet"
            )
        );
    }
}
