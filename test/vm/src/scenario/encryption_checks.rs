//! Reads what the encryption and hibernation scenarios see: the TPM2 tokens of a LUKS2 volume,
//! the recovery key, the guard's report of its PCR 7 enrollment, and the installer's log and
//! the disk checksums of a refused install. The scenario turns each problem these return into a
//! failed check.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::Value;

/// The firmware variable that systemd writes before it hibernates and clears on resume.
pub const HIBERNATE_LOCATION: &str =
    "/sys/firmware/efi/efivars/HibernateLocation-8cf2644b-4b0b-428f-9387-6d876050dc67";
/// The installer's refusal of a disk that holds a hibernated InayOS.
pub const HIBERNATED_MESSAGE: &str =
    "This disk holds a hibernated InayOS: start it and shut it down properly first";
/// What the installer adds to a failure of its checks stage.
pub const DISK_UNCHANGED: &str = "the disk is unchanged";

/// The characters of systemd's recovery keys, the "modhex" alphabet.
const MODHEX: &str = "cbdefghijklnrtuv";
const KEY_GROUPS: usize = 8;
const KEY_GROUP_LENGTH: usize = 8;

/// One systemd-tpm2 token of a LUKS2 volume: the keyslots it opens and the PCRs its policy
/// binds. An empty PCR list is a temporary slot that any boot on this TPM can open.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tpm2Token {
    pub token: u32,
    pub keyslots: Vec<String>,
    pub pcrs: Vec<u32>,
}

impl Tpm2Token {
    fn is_temporary(&self) -> bool {
        self.pcrs.is_empty()
    }

    fn binds_only_pcr7(&self) -> bool {
        self.pcrs == [7]
    }
}

/// Reads the systemd-tpm2 tokens from `cryptsetup luksDump --dump-json-metadata`, in token
/// order.
pub fn tpm2_tokens(json: &str) -> Result<Vec<Tpm2Token>, String> {
    let value: Value = serde_json::from_str(json)
        .map_err(|error| format!("the LUKS2 metadata is not JSON: {error}"))?;
    let tokens = value
        .get("tokens")
        .and_then(Value::as_object)
        .ok_or("the LUKS2 metadata has no tokens object")?;
    let mut found = Vec::new();
    for (id, token) in tokens {
        if token.get("type").and_then(Value::as_str) != Some("systemd-tpm2") {
            continue;
        }
        let token_number = id
            .parse()
            .map_err(|_| format!("the LUKS2 token id {id:?} is not a number"))?;
        let keyslots = token
            .get("keyslots")
            .and_then(Value::as_array)
            .map(|slots| {
                slots
                    .iter()
                    .filter_map(|slot| slot.as_str().map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default();
        let pcrs = token
            .get("tpm2-pcrs")
            .and_then(Value::as_array)
            .ok_or_else(|| format!("the systemd-tpm2 token {id} has no tpm2-pcrs list"))?
            .iter()
            .map(|pcr| {
                pcr.as_u64()
                    .and_then(|pcr| u32::try_from(pcr).ok())
                    .ok_or_else(|| format!("the systemd-tpm2 token {id} lists the PCR {pcr}"))
            })
            .collect::<Result<_, _>>()?;
        found.push(Tpm2Token {
            token: token_number,
            keyslots,
            pcrs,
        });
    }
    found.sort_by_key(|token| token.token);
    Ok(found)
}

/// The keyslots that a TPM2 token bound to PCR 7 alone opens.
pub fn pcr7_keyslots(tokens: &[Tpm2Token]) -> Vec<String> {
    tokens
        .iter()
        .filter(|token| token.binds_only_pcr7())
        .flat_map(|token| token.keyslots.iter().cloned())
        .collect()
}

/// What is wrong with a volume whose TPM enrollment is finished: it needs a TPM2 token bound to
/// PCR 7 and no temporary token with an empty PCR list.
pub fn enrollment_problem(tokens: &[Tpm2Token]) -> Option<String> {
    if let Some(temporary) = tokens.iter().find(|token| token.is_temporary()) {
        return Some(format!(
            "the temporary TPM2 token {} with an empty PCR list is still there",
            temporary.token
        ));
    }
    if pcr7_keyslots(tokens).is_empty() {
        return Some(format!(
            "no TPM2 token is bound to PCR 7; the TPM2 tokens bind {:?}",
            tokens.iter().map(|token| &token.pcrs).collect::<Vec<_>>()
        ));
    }
    None
}

/// Whether the volume has a temporary TPM2 token, with an empty PCR list.
pub fn has_temporary_token(tokens: &[Tpm2Token]) -> bool {
    tokens.iter().any(Tpm2Token::is_temporary)
}

/// The first systemd recovery key in `text`: eight groups of eight modhex characters joined by
/// dashes.
pub fn recovery_key(text: &str) -> Option<&str> {
    text.split_whitespace().find(|word| {
        let groups: Vec<&str> = word.split('-').collect();
        groups.len() == KEY_GROUPS
            && groups.iter().all(|group| {
                group.len() == KEY_GROUP_LENGTH && group.chars().all(|c| MODHEX.contains(c))
            })
    })
}

/// Whether the output of `inayos-guard status` reports a PCR 7 enrollment: a line that names
/// PCR 7, written as "PCR 7" or "PCR7" in any case.
pub fn mentions_pcr7(status: &str) -> bool {
    status.lines().any(|line| {
        let line = line.to_lowercase();
        line.contains("pcr 7") || line.contains("pcr7")
    })
}

/// The LUKS volumes in `lsblk -nrpo NAME,FSTYPE` output.
pub fn luks_devices(lsblk: &str) -> Vec<String> {
    lsblk
        .lines()
        .filter_map(|line| {
            let mut fields = line.split_whitespace();
            let name = fields.next()?;
            (fields.next() == Some("crypto_LUKS")).then(|| name.to_owned())
        })
        .collect()
}

/// What is wrong with the installer's output and log for a refused install of a hibernated
/// disk, or none when they hold the hibernation message and say that the disk is unchanged.
pub fn refusal_problem(log: &str) -> Option<String> {
    let refusal = log.lines().find(|line| line.contains(HIBERNATED_MESSAGE));
    match refusal {
        None => Some(format!(
            "the installer did not refuse with {HIBERNATED_MESSAGE:?}"
        )),
        Some(line) if !line.contains(DISK_UNCHANGED) => Some(format!(
            "the installer refused, but not in its checks stage, which says {DISK_UNCHANGED:?}: {line}"
        )),
        Some(_) => None,
    }
}

/// The commands in the installer's log, lines that start with `$ `, that can write to a disk:
/// partitioning, formatting, LUKS header changes, filesystem repairs and resizes, swap, and
/// any mount that is not read-only.
pub fn disk_writes(log: &str) -> Vec<String> {
    log.lines()
        .filter_map(|line| line.trim().strip_prefix("$ "))
        .filter(|command| writes_to_disk(command))
        .map(str::to_owned)
        .collect()
}

fn writes_to_disk(command: &str) -> bool {
    let words: Vec<&str> = command.split_whitespace().collect();
    let Some(first) = words.first() else {
        return false;
    };
    let program = first.rsplit('/').next().unwrap_or(first);
    let has = |flag: &str| words.contains(&flag);
    match program {
        "sgdisk"
        | "sfdisk"
        | "parted"
        | "wipefs"
        | "mkswap"
        | "swapon"
        | "resize2fs"
        | "pacstrap"
        | "systemd-cryptenroll"
        | "btrfs" => {
            // btrfs only reads with these subcommands.
            !(program == "btrfs"
                && matches!(
                    words.get(1..3),
                    Some(["filesystem", "show" | "usage" | "df"])
                        | Some(["subvolume", "list" | "show"])
                        | Some(["inspect-internal", _])
                ))
        }
        // dd writes only to its `of=` file; without one it writes to its standard output.
        "dd" => words
            .iter()
            .any(|word| word.trim_start_matches(['\'', '"']).starts_with("of=")),
        "e2fsck" | "fsck" | "fsck.ext4" => !has("-n"),
        "tune2fs" => !has("-l"),
        "cryptsetup" => words.iter().skip(1).any(|word| {
            matches!(
                *word,
                "luksFormat"
                    | "luksAddKey"
                    | "luksRemoveKey"
                    | "luksChangeKey"
                    | "luksKillSlot"
                    | "erase"
                    | "luksErase"
                    | "reencrypt"
                    | "token"
                    | "config"
                    | "convert"
                    | "luksHeaderRestore"
            )
        }),
        "mount" => !mounts_read_only(&words[1..]),
        _ => program.starts_with("mkfs"),
    }
}

/// Whether the arguments of `mount` ask for a read-only mount.
fn mounts_read_only(arguments: &[&str]) -> bool {
    let read_only_options = |options: &str| options.split(',').any(|option| option == "ro");
    arguments.iter().enumerate().any(|(index, argument)| {
        matches!(*argument, "-r" | "--read-only")
            || (matches!(*argument, "-o" | "--options")
                && arguments
                    .get(index + 1)
                    .is_some_and(|options| read_only_options(options)))
            || argument
                .strip_prefix("-o")
                .or_else(|| argument.strip_prefix("--options="))
                .is_some_and(|options| !options.is_empty() && read_only_options(options))
    })
}

/// The names whose checksum differs between two outputs of `<name> <checksum>` lines, or that
/// only one of them lists.
pub fn changed_heads(before: &str, after: &str) -> Vec<String> {
    let read = |text: &str| -> BTreeMap<String, String> {
        text.lines()
            .filter_map(|line| line.split_once(' '))
            .map(|(name, sum)| (name.to_owned(), sum.trim().to_owned()))
            .collect()
    };
    let (before, after) = (read(before), read(after));
    let names: BTreeSet<&String> = before
        .keys()
        .chain(after.keys())
        .filter(|name| before.get(*name) != after.get(*name))
        .collect();
    names.into_iter().cloned().collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::qmp::key_for;

    const KEY: &str = "fdbjrbge-dekhbdtv-cnfkljng-hijdtbrc-bdicruvl-ulevtnri-gfejrvld-btrhcnje";

    fn dump(tokens: &str) -> String {
        format!(
            r#"{{"keyslots":{{"0":{{"type":"luks2"}},"1":{{"type":"luks2"}},"2":{{"type":"luks2"}}}},"tokens":{tokens},"segments":{{}}}}"#
        )
    }

    #[test]
    fn the_tpm2_tokens_give_their_keyslots_and_pcrs_and_other_tokens_are_left_out() {
        let json = dump(
            r#"{"2":{"type":"systemd-tpm2","keyslots":["2"],"tpm2-pcrs":[],"tpm2-pcr-bank":"sha256"},
                "0":{"type":"systemd-recovery","keyslots":["0"]},
                "1":{"type":"systemd-tpm2","keyslots":["1"],"tpm2-pcrs":[7],"tpm2-pcr-bank":"sha256"}}"#,
        );
        assert_eq!(
            tpm2_tokens(&json).unwrap(),
            [
                Tpm2Token {
                    token: 1,
                    keyslots: vec!["1".to_owned()],
                    pcrs: vec![7],
                },
                Tpm2Token {
                    token: 2,
                    keyslots: vec!["2".to_owned()],
                    pcrs: vec![],
                },
            ]
        );
    }

    #[test]
    fn a_finished_enrollment_has_a_pcr7_token_and_no_temporary_one() {
        let finished = tpm2_tokens(&dump(
            r#"{"0":{"type":"systemd-recovery","keyslots":["0"]},
                "1":{"type":"systemd-tpm2","keyslots":["1"],"tpm2-pcrs":[7]}}"#,
        ))
        .unwrap();
        assert_eq!(enrollment_problem(&finished), None);
        assert_eq!(pcr7_keyslots(&finished), ["1"]);
        assert!(!has_temporary_token(&finished));

        let first_boot_pending = tpm2_tokens(&dump(
            r#"{"1":{"type":"systemd-tpm2","keyslots":["1"],"tpm2-pcrs":[]}}"#,
        ))
        .unwrap();
        assert!(has_temporary_token(&first_boot_pending));
        assert!(
            enrollment_problem(&first_boot_pending)
                .unwrap()
                .contains("temporary")
        );

        let other_pcrs = tpm2_tokens(&dump(
            r#"{"1":{"type":"systemd-tpm2","keyslots":["1"],"tpm2-pcrs":[0,7]}}"#,
        ))
        .unwrap();
        assert!(enrollment_problem(&other_pcrs).unwrap().contains("PCR 7"));
    }

    #[test]
    fn metadata_without_tokens_is_an_error() {
        assert!(tpm2_tokens(r#"{"keyslots":{}}"#).is_err());
        assert!(tpm2_tokens("not json").is_err());
    }

    #[test]
    fn the_recovery_key_is_found_in_the_file_text_and_other_words_are_not_keys() {
        let file = format!(
            "InayOS recovery key\n\n{KEY}\n\nStore it in your password manager, then delete this file.\n"
        );
        assert_eq!(recovery_key(&file), Some(KEY));
        assert_eq!(recovery_key("abcdefgh-abcdefgh"), None);
        // "a" is not a modhex character.
        let wrong = KEY.replacen('f', "a", 1);
        assert_eq!(recovery_key(&wrong), None);
    }

    #[test]
    fn a_recovery_key_types_on_the_us_layout_without_shift() {
        for character in format!("{KEY}\n").chars() {
            assert!(
                matches!(key_for(character), Some((_, false))),
                "{character:?}"
            );
        }
    }

    #[test]
    fn the_guard_status_counts_when_a_line_names_pcr_7() {
        assert!(mentions_pcr7("TPM: enrolled PCR 7 on system and home\n"));
        assert!(mentions_pcr7("tpm2 pcr7 enrollment done"));
        assert!(!mentions_pcr7("last check passed\n"));
    }

    #[test]
    fn the_luks_volumes_come_from_lsblk() {
        let lsblk = "/dev/vda \n/dev/vda1 vfat\n/dev/vda2 crypto_LUKS\n/dev/vda3 crypto_LUKS\n/dev/mapper/system btrfs\n";
        assert_eq!(luks_devices(lsblk), ["/dev/vda2", "/dev/vda3"]);
    }

    #[test]
    fn a_refusal_counts_only_with_the_hibernation_message_in_the_checks_stage() {
        let refused =
            format!("==> Checking the machine\nerror: {HIBERNATED_MESSAGE}; {DISK_UNCHANGED}\n");
        assert_eq!(refusal_problem(&refused), None);
        let late =
            format!("error: {HIBERNATED_MESSAGE}; the disk is partly partitioned or formatted\n");
        assert!(refusal_problem(&late).unwrap().contains("checks stage"));
        assert!(
            refusal_problem("error: no network\n")
                .unwrap()
                .contains("did not refuse")
        );
    }

    #[test]
    fn reads_and_read_only_mounts_are_not_disk_writes() {
        let log = "\
==> Checking the machine
$ lsblk -J -b -o NAME,SIZE /dev/vda
$ mount -o ro,noload /dev/vda3 /run/inayos-installer/home
$ mount -r /dev/vda2 /mnt
$ mount --options=ro /dev/vda2 /mnt
$ cryptsetup luksDump /dev/vda2
$ btrfs filesystem show /dev/vda2
$ e2fsck -n /dev/vda3
$ debugfs -R 'bmap /swapfile 0' /dev/vda3
$ dumpe2fs -h /dev/vda3
$ dd if=/dev/vda3 iflag=skip_bytes,count_bytes skip=135266304 count=4096 status=none
$ umount /run/inayos-installer/home
";
        assert_eq!(disk_writes(log), Vec::<String>::new());
    }

    #[test]
    fn partitioning_formatting_and_writable_mounts_are_disk_writes() {
        let log = "\
$ sgdisk --zap-all /dev/vda
$ /usr/bin/mkfs.btrfs -L inayos-system /dev/vda2
$ mount /dev/vda3 /mnt/home
$ mount -o noatime /dev/vda2 /mnt
$ cryptsetup --type luks2 luksFormat /dev/vda2 /run/key
$ e2fsck -f -y /dev/vda3
$ dd if=/dev/zero of=/dev/vda3 bs=1M count=1
";
        assert_eq!(
            disk_writes(log),
            [
                "sgdisk --zap-all /dev/vda",
                "/usr/bin/mkfs.btrfs -L inayos-system /dev/vda2",
                "mount /dev/vda3 /mnt/home",
                "mount -o noatime /dev/vda2 /mnt",
                "cryptsetup --type luks2 luksFormat /dev/vda2 /run/key",
                "e2fsck -f -y /dev/vda3",
                "dd if=/dev/zero of=/dev/vda3 bs=1M count=1",
            ]
        );
    }

    #[test]
    fn the_changed_heads_name_each_area_whose_checksum_differs_or_is_missing() {
        let before = "/dev/vda aaa\n/dev/vda1 bbb\n/dev/vda3 ccc\nend ddd\n";
        assert_eq!(changed_heads(before, before), Vec::<String>::new());
        let after = "/dev/vda aaa\n/dev/vda1 bbb\n/dev/vda3 eee\n";
        assert_eq!(changed_heads(before, after), ["/dev/vda3", "end"]);
    }
}
