//! The snapshots the "Previous system" entry can boot: the snapper "pre" snapshots of the
//! root configuration, which snap-pac takes before each pacman transaction.
//!
//! The guard reads snapper's `info.xml` files in /.snapshots instead of running snapper,
//! so it needs neither snapper's D-Bus service nor its output format.
use std::path::{Path, PathBuf};

use crate::system::{System, read_text};

pub const SNAPSHOTS: &str = "/.snapshots";
/// The Btrfs subvolume mounted at /.snapshots, as rEFInd and the kernel name it.
pub const SNAPSHOTS_SUBVOLUME: &str = "@snapshots";
const MICROCODE: [&str; 2] = ["intel-ucode.img", "amd-ucode.img"];

/// What snapper's `info.xml` says about one snapshot.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Snapshot {
    pub number: u32,
    /// `single`, `pre` or `post`.
    pub kind: String,
    pub description: String,
}

/// The snapshot the "Previous system" entry boots, with the boot files found inside it.
/// File names are relative to the snapshot's /boot.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PreviousSystem {
    pub number: u32,
    pub description: String,
    pub kernel: String,
    pub initramfs: String,
    pub microcode: Vec<String>,
}

impl PreviousSystem {
    /// The snapshot's subvolume, relative to the top of the Btrfs filesystem.
    pub fn subvolume(&self) -> String {
        format!("{SNAPSHOTS_SUBVOLUME}/{}/snapshot", self.number)
    }

    /// The snapshot's kernel as the running system sees it.
    pub fn kernel_path(&self) -> PathBuf {
        self.boot_path(&self.kernel)
    }

    /// The snapshot's boot image as the running system sees it.
    pub fn initramfs_path(&self) -> PathBuf {
        self.boot_path(&self.initramfs)
    }

    fn boot_path(&self, file: &str) -> PathBuf {
        Path::new(SNAPSHOTS)
            .join(self.number.to_string())
            .join("snapshot/boot")
            .join(file)
    }
}

/// The text inside the first `<tag>...</tag>` of `xml`.
fn element<'a>(xml: &'a str, tag: &str) -> Option<&'a str> {
    let start = xml.find(&format!("<{tag}>"))? + tag.len() + 2;
    let length = xml[start..].find(&format!("</{tag}>"))?;
    Some(xml[start..start + length].trim())
}

fn unescape(text: &str) -> String {
    text.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}

/// Reads snapper's `info.xml` of one snapshot.
pub fn parse_info(xml: &str) -> Option<Snapshot> {
    Some(Snapshot {
        number: element(xml, "num")?.parse().ok()?,
        kind: element(xml, "type")?.to_owned(),
        description: element(xml, "description")
            .map(unescape)
            .unwrap_or_default(),
    })
}

/// The "pre" snapshots in `snapshots`, newest first.
pub fn pre_snapshots(snapshots: &[Snapshot]) -> Vec<&Snapshot> {
    let mut pre: Vec<&Snapshot> = snapshots.iter().filter(|s| s.kind == "pre").collect();
    pre.sort_by_key(|snapshot| std::cmp::Reverse(snapshot.number));
    pre
}

/// The "pre" snapshots that hold a kernel and its boot image, newest first. `kernel` is
/// the file name of the normal entry's kernel, preferred when the snapshot has it. The
/// files are only found here: whether they start is for the checks to say.
pub fn candidates(system: &dyn System, kernel: &str) -> Vec<PreviousSystem> {
    let root = Path::new(SNAPSHOTS);
    let snapshots: Vec<Snapshot> = system
        .list(root)
        .unwrap_or_default()
        .iter()
        .filter(|name| name.chars().all(|c| c.is_ascii_digit()))
        .filter_map(|name| read_text(system, &root.join(name).join("info.xml")).ok())
        .filter_map(|xml| parse_info(&xml))
        .collect();
    pre_snapshots(&snapshots)
        .into_iter()
        .filter_map(|snapshot| {
            let boot = root.join(snapshot.number.to_string()).join("snapshot/boot");
            let names = system.list(&boot).ok()?;
            let kernel = if names.iter().any(|name| name == kernel) {
                kernel.to_owned()
            } else {
                names
                    .iter()
                    .find(|name| name.starts_with("vmlinuz-"))?
                    .clone()
            };
            let initramfs = format!(
                "initramfs-{}.img",
                kernel.strip_prefix("vmlinuz-").unwrap_or(&kernel)
            );
            if !names.contains(&initramfs) {
                return None;
            }
            let microcode = MICROCODE
                .iter()
                .filter(|file| names.iter().any(|name| name == *file))
                .map(|file| (*file).to_owned())
                .collect();
            Some(PreviousSystem {
                number: snapshot.number,
                description: snapshot.description.clone(),
                kernel,
                initramfs,
                microcode,
            })
        })
        .collect()
}

/// Marks snapshot `number` important, so that snapper's number cleanup counts it against
/// `NUMBER_LIMIT_IMPORTANT` instead of `NUMBER_LIMIT` and keeps it for longer while the
/// "Previous system" entry boots it. Returns the failure in plain words.
pub fn keep(system: &dyn System, number: u32) -> Result<(), String> {
    let number = number.to_string();
    let args = [
        "--no-dbus",
        "-c",
        "root",
        "modify",
        "--userdata",
        "important=yes",
        number.as_str(),
    ];
    match system.run("snapper", &args) {
        Ok(output) if output.success() => Ok(()),
        Ok(output) => Err(format!(
            "snapper could not mark snapshot {number} important, so its cleanup can delete it: {}",
            output.stderr.trim()
        )),
        Err(error) => Err(format!(
            "snapper could not run to mark snapshot {number} important, so its cleanup can delete it: {error}"
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fake::FakeSystem;

    fn info(number: u32, kind: &str, description: &str) -> String {
        format!(
            "<?xml version=\"1.0\"?>\n<snapshot>\n  <type>{kind}</type>\n  <num>{number}</num>\n  <date>2026-11-18 10:00:00</date>\n  <description>{description}</description>\n  <cleanup>number</cleanup>\n</snapshot>\n"
        )
    }

    #[test]
    fn parse_info_reads_snapper_info_files() {
        let snapshot = parse_info(&info(12, "pre", "pacman -U /var/cache/linux &amp; more"));
        assert_eq!(
            snapshot,
            Some(Snapshot {
                number: 12,
                kind: "pre".to_owned(),
                description: "pacman -U /var/cache/linux & more".to_owned(),
            })
        );
    }

    fn snapshot(system: &FakeSystem, number: u32, kind: &str, boot: &[&str]) {
        system.file(
            &format!("/.snapshots/{number}/info.xml"),
            info(number, kind, "pacman -Syu").as_bytes(),
        );
        for file in boot {
            system.file(&format!("/.snapshots/{number}/snapshot/boot/{file}"), b"x");
        }
    }

    #[test]
    fn the_candidates_are_the_pre_snapshots_with_a_kernel_newest_first() {
        let system = FakeSystem::default();
        let full = ["vmlinuz-linux", "initramfs-linux.img", "intel-ucode.img"];
        snapshot(&system, 3, "pre", &full);
        snapshot(&system, 4, "post", &full);
        snapshot(&system, 9, "pre", &full);
        snapshot(&system, 10, "post", &full);
        // A pre snapshot whose /boot has no boot image cannot boot.
        snapshot(&system, 11, "pre", &["vmlinuz-linux"]);
        snapshot(&system, 12, "single", &full);
        let candidates = candidates(&system, "vmlinuz-linux");
        let numbers: Vec<u32> = candidates.iter().map(|c| c.number).collect();
        assert_eq!(numbers, [9, 3]);
        let previous = &candidates[0];
        assert_eq!(previous.kernel, "vmlinuz-linux");
        assert_eq!(previous.initramfs, "initramfs-linux.img");
        assert_eq!(previous.microcode, ["intel-ucode.img"]);
        assert_eq!(previous.subvolume(), "@snapshots/9/snapshot");
        assert_eq!(
            previous.initramfs_path(),
            Path::new("/.snapshots/9/snapshot/boot/initramfs-linux.img")
        );
    }

    #[test]
    fn no_pre_snapshot_means_no_previous_system() {
        let system = FakeSystem::default();
        snapshot(
            &system,
            1,
            "single",
            &["vmlinuz-linux", "initramfs-linux.img"],
        );
        assert_eq!(candidates(&system, "vmlinuz-linux"), []);
    }
}
