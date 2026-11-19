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

/// The kernel options of the "Previous system" entry: the snapshot's root as a writable
/// overlay over the read-only snapshot, then the other options of the normal entry
/// `normal`, such as `quiet` and the console colors. None when `normal` has no `root=`.
pub fn previous_options(previous: &PreviousSystem, normal: &str) -> Option<String> {
    let root = option_value(normal, "root")?;
    let mut options: Vec<String> = previous
        .microcode
        .iter()
        .map(|file| format!("initrd={}", efi_path(previous, file)))
        .collect();
    options.push(format!("root={root}"));
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

/// The whole file: the "Previous system" stanza when `previous` holds the snapshot and
/// its options, then `default_selection`.
pub fn render(previous: Option<(&PreviousSystem, &str)>, selection: &str) -> String {
    let mut text = String::from(
        "# Written by inayos-guard after each change to the boot files. Edits are overwritten.\n",
    );
    if let Some((previous, options)) = previous {
        text.push_str(&format!(
            "\n# Snapshot {}: {}\nmenuentry \"{PREVIOUS_TITLE}\" {{\n    volume {VOLUME}\n    loader /{}/boot/{}\n    initrd {}\n    options \"{options}\"\n}}\n",
            previous.number,
            previous.description.replace('\n', " "),
            previous.subvolume(),
            previous.kernel,
            efi_path(previous, &previous.initramfs),
        ));
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
        let text = render(Some((&previous, "root=UUID=abc rw")), "Previous system");
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
}
