//! The TPM2 slots of the LUKS2 volumes listed in `/etc/crypttab.initramfs`.
//!
//! Each volume normally has one TPM2 slot bound to PCR 7, the Secure Boot state, and the
//! recovery key. A temporary slot is a TPM2 slot bound to no PCR: the installer leaves one
//! for the first boot, and `prepare-firmware-update` adds one before a firmware or Secure
//! Boot database update changes PCR 7. At the next normal boot, `check-boot` enrolls a
//! new slot bound to PCR 7 and removes the temporary slot and the earlier PCR 7 slot.
//!
//! The guard reads the slots from `cryptsetup luksDump --dump-json-metadata`, the LUKS2
//! header's JSON. A token written by `systemd-cryptenroll --tpm2-device` reads:
//!
//! ```json
//! "tokens": {
//!   "1": { "type": "systemd-tpm2", "keyslots": ["2"], "tpm2-pcrs": [7], "tpm2-pcr-bank": "sha256", ... }
//! }
//! ```
//!
//! `tpm2-pcrs` is always written, and is `[]` for a slot bound to no PCR. A slot with a
//! PIN (`tpm2-pin`), a pcrlock policy (`tpm2_pcrlock`) or a signed PCR policy
//! (`tpm2_pubkey_pcrs`) is not one the guard made, and it leaves such slots alone.
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use serde::Deserialize;

use crate::{
    Error,
    checks::device_path,
    system::{System, read_text},
};

pub const CRYPTTAB: &str = "/etc/crypttab.initramfs";

/// A LUKS volume the initramfs opens: a line of `/etc/crypttab.initramfs`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Volume {
    /// The mapper name, such as `system` for `/dev/mapper/system`.
    pub name: String,
    /// The encrypted device as crypttab names it, such as `UUID=<luks uuid>`.
    pub source: String,
}

/// The volumes of a crypttab: lines of `<name> <device> <key file> <options>`, comments
/// and blank lines left out.
pub fn parse_crypttab(text: &str) -> Vec<Volume> {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .filter_map(|line| {
            let mut fields = line.split_whitespace();
            Some(Volume {
                name: fields.next()?.to_owned(),
                source: fields.next()?.to_owned(),
            })
        })
        .collect()
}

/// The volumes of `/etc/crypttab.initramfs`, or None when the system is not encrypted.
pub fn volumes(system: &dyn System) -> Result<Option<Vec<Volume>>, Error> {
    let path = Path::new(CRYPTTAB);
    if !system.exists(path) {
        return Ok(None);
    }
    read_text(system, path)
        .map(|text| Some(parse_crypttab(&text)))
        .map_err(|source| Error::Read {
            path: path.to_owned(),
            source,
        })
}

/// What a TPM2 slot is bound to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Binding {
    /// No PCR: the temporary slot, which unlocks whatever the machine booted.
    Nothing,
    /// PCR 7 alone: the normal slot.
    Pcr7,
    /// Anything else, which the guard did not make.
    Other,
}

/// A TPM2 slot: its LUKS2 token and the key slot it unlocks.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TpmSlot {
    pub token: u32,
    /// The key slot number, which `systemd-cryptenroll --wipe-slot` takes.
    pub keyslot: u32,
    pub binding: Binding,
}

#[derive(Deserialize)]
struct Metadata {
    #[serde(default)]
    tokens: BTreeMap<String, Token>,
}

#[derive(Deserialize)]
struct Token {
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    keyslots: Vec<String>,
    #[serde(rename = "tpm2-pcrs", default)]
    pcrs: Vec<u32>,
    #[serde(rename = "tpm2-pin", default)]
    pin: bool,
    #[serde(rename = "tpm2_pcrlock", default)]
    pcrlock: bool,
    #[serde(rename = "tpm2_pubkey_pcrs", default)]
    pubkey_pcrs: Vec<u32>,
}

impl Token {
    fn binding(&self) -> Binding {
        if self.pin || self.pcrlock || !self.pubkey_pcrs.is_empty() {
            Binding::Other
        } else if self.pcrs.is_empty() {
            Binding::Nothing
        } else if self.pcrs == [7] {
            Binding::Pcr7
        } else {
            Binding::Other
        }
    }
}

/// The TPM2 slots in `json`, the output of `cryptsetup luksDump --dump-json-metadata`,
/// in token order.
pub fn tpm_slots(json: &str) -> Result<Vec<TpmSlot>, String> {
    let metadata: Metadata = serde_json::from_str(json)
        .map_err(|error| format!("the LUKS2 header's JSON cannot be read: {error}"))?;
    let mut slots = Vec::new();
    for (id, token) in &metadata.tokens {
        if token.kind != "systemd-tpm2" {
            continue;
        }
        let token_number = id
            .parse()
            .map_err(|_| format!("the LUKS2 header has a token named {id}"))?;
        let keyslot = token
            .keyslots
            .first()
            .and_then(|slot| slot.parse().ok())
            .ok_or_else(|| format!("TPM2 token {id} names no key slot"))?;
        slots.push(TpmSlot {
            token: token_number,
            keyslot,
            binding: token.binding(),
        });
    }
    slots.sort_by_key(|slot| slot.token);
    Ok(slots)
}

/// One line describing the TPM2 slots of the volume `name`, for the record. `dead` holds
/// the key slots that were tested and do not open the volume on this boot.
pub fn describe_slots(name: &str, slots: &[TpmSlot], dead: &[u32]) -> String {
    if slots.is_empty() {
        return format!("{name}: no TPM2 slot, so it opens only with the recovery key");
    }
    let parts: Vec<String> = slots
        .iter()
        .map(|slot| {
            let text = match slot.binding {
                Binding::Pcr7 => format!("TPM2 slot {} bound to PCR 7", slot.keyslot),
                Binding::Nothing => {
                    format!("TPM2 slot {} temporary, bound to no PCR", slot.keyslot)
                }
                Binding::Other => format!("TPM2 slot {} with another policy", slot.keyslot),
            };
            if dead.contains(&slot.keyslot) {
                format!("{text}, does not open the volume")
            } else {
                text
            }
        })
        .collect();
    format!("{name}: {}", parts.join("; "))
}

/// A volume with the device the guard runs the LUKS tools on.
struct Device<'a> {
    volume: &'a Volume,
    path: PathBuf,
}

impl Device<'_> {
    fn find(volume: &Volume) -> Result<Device<'_>, String> {
        let path = device_path(&volume.source).ok_or_else(|| {
            format!(
                "{}: {CRYPTTAB} names the device {}, which the guard cannot find",
                volume.name, volume.source
            )
        })?;
        Ok(Device { volume, path })
    }

    fn name(&self) -> &str {
        &self.volume.name
    }

    fn path(&self) -> String {
        self.path.to_string_lossy().into_owned()
    }

    fn slots(&self, system: &dyn System) -> Result<Vec<TpmSlot>, String> {
        let path = self.path();
        let output = run(
            system,
            "cryptsetup",
            &["luksDump", "--dump-json-metadata", &path],
        )
        .map_err(|error| format!("{}: cannot read the LUKS2 header: {error}", self.name()))?;
        tpm_slots(&output).map_err(|error| format!("{}: {error}", self.name()))
    }

    /// Whether the TPM2 slot opens the volume on this boot, by its token alone.
    fn opens(&self, system: &dyn System, slot: &TpmSlot) -> Result<(), String> {
        let token = slot.token.to_string();
        let path = self.path();
        run(
            system,
            "cryptsetup",
            &[
                "open",
                "--test-passphrase",
                "--token-only",
                "--token-id",
                &token,
                &path,
            ],
        )
        .map(|_| ())
    }

    /// The key slots of the guard's TPM2 slots that do not open the volume on this boot.
    fn dead_slots(&self, system: &dyn System, slots: &[TpmSlot]) -> Vec<u32> {
        slots
            .iter()
            .filter(|slot| slot.binding != Binding::Other)
            .filter(|slot| self.opens(system, slot).is_err())
            .map(|slot| slot.keyslot)
            .collect()
    }
}

/// Whether the TPM opens the volume on this boot with no typed key: one of the guard's
/// TPM2 slots opens it, or it has a TPM2 slot the guard did not make and leaves alone.
fn tpm_opens(slots: &[TpmSlot], dead: &[u32]) -> bool {
    slots
        .iter()
        .any(|slot| slot.binding == Binding::Other || !dead.contains(&slot.keyslot))
}

/// The command that binds the volumes to PCR 7 again with the recovery key.
pub const REBIND: &str = "sudo inayos-guard rebind";

/// Runs a program and returns its standard output, or the failure in plain words.
fn run(system: &dyn System, program: &str, args: &[&str]) -> Result<String, String> {
    match system.run(program, args) {
        Ok(output) if output.success() => Ok(output.stdout),
        Ok(output) => Err(format!("{program} failed: {}", output.stderr.trim())),
        Err(error) => Err(format!("{program} could not run: {error}")),
    }
}

/// What a TPM step found and changed.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct TpmReport {
    /// The TPM2 slots of each volume after the step, one line per volume.
    pub state: Vec<String>,
    pub changes: Vec<String>,
    pub failures: Vec<String>,
    /// What the step left alone on purpose, and why.
    pub notes: Vec<String>,
}

impl TpmReport {
    fn state_of(&mut self, system: &dyn System, device: &Device) {
        match device.slots(system) {
            Ok(slots) => self.state.push(describe_slots(device.name(), &slots, &[])),
            Err(failure) => self.failures.push(failure),
        }
    }

    /// Records the TPM2 slots of the volume after testing each one, and a failure when
    /// none opens it: the volume then asks for the recovery key at every start.
    fn tested_state_of(&mut self, system: &dyn System, device: &Device) {
        let slots = match device.slots(system) {
            Ok(slots) => slots,
            Err(failure) => {
                self.failures.push(failure);
                return;
            }
        };
        let dead = device.dead_slots(system, &slots);
        if !tpm_opens(&slots, &dead) {
            self.failures.push(format!(
                "{}: no TPM2 slot opens the volume on this boot, so it asks for the recovery key at every start; run `{REBIND}` and type the recovery key once to bind the TPM to PCR 7 again",
                device.name()
            ));
        }
        self.state
            .push(describe_slots(device.name(), &slots, &dead));
    }
}

/// The key slots `wipe-slot` takes, as `systemd-cryptenroll` reads them: `3,4`.
fn slot_list(slots: &[u32]) -> String {
    slots
        .iter()
        .map(u32::to_string)
        .collect::<Vec<_>>()
        .join(",")
}

/// Why `bind_to_pcr7` leaves the temporary slots alone on this boot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hold<'a> {
    /// Nothing holds them: the system booted normally, in the Secure Boot state it was
    /// set up with.
    None,
    /// Secure Boot is in another state than the one the machine was set up with, for the
    /// reason given. A slot bound to PCR 7 now would not open the volume in the right
    /// state, so this is a failure.
    SecureBoot(&'a str),
    /// `prepare-firmware-update` ran during this boot, so the temporary slots protect a
    /// firmware update that has not changed PCR 7 yet.
    FirmwareUpdate,
}

/// On each volume with a temporary slot, enrolls a new TPM2 slot bound to PCR 7, checks
/// that it opens the volume, then wipes the temporary slot and the earlier PCR 7 slots.
/// The caller makes sure the system booted normally, and says in `hold` why the slots
/// must stay as they are. Then it tests every TPM2 slot of each volume and records a
/// failure for a volume that no TPM2 slot opens, such as after the TPM was cleared.
///
/// A volume whose step fails keeps every slot it had, and may keep a new PCR 7 slot
/// too, so it still opens with the temporary slot.
pub fn bind_to_pcr7(system: &dyn System, volumes: &[Volume], hold: Hold) -> TpmReport {
    let mut report = TpmReport::default();
    for volume in volumes {
        let device = match Device::find(volume) {
            Ok(device) => device,
            Err(failure) => {
                report.failures.push(failure);
                continue;
            }
        };
        let before = match device.slots(system) {
            Ok(slots) => slots,
            Err(failure) => {
                report.failures.push(failure);
                continue;
            }
        };
        if before.iter().any(|slot| slot.binding == Binding::Nothing) {
            match hold {
                Hold::None => {
                    if let Err(failure) =
                        enroll_pcr7(system, &device, &before, &mut report.changes)
                    {
                        report.failures.push(failure);
                    }
                }
                Hold::SecureBoot(reason) => report.failures.push(format!(
                    "{}: {reason}, so the TPM2 slot is not bound to PCR 7 and the temporary slot stays",
                    device.name()
                )),
                Hold::FirmwareUpdate => report.notes.push(format!(
                    "{}: a firmware update was prepared during this boot, so the temporary slot stays until the next boot",
                    device.name()
                )),
            }
        }
        report.tested_state_of(system, &device);
    }
    report
}

fn enroll_pcr7(
    system: &dyn System,
    device: &Device,
    before: &[TpmSlot],
    changes: &mut Vec<String>,
) -> Result<(), String> {
    let name = device.name();
    let path = device.path();
    run(
        system,
        "systemd-cryptenroll",
        &[
            "--unlock-tpm2-device=auto",
            "--tpm2-device=auto",
            "--tpm2-pcrs=7",
            &path,
        ],
    )
    .map_err(|error| {
        format!("{name}: could not enroll a TPM2 slot bound to PCR 7, so the temporary slot stays: {error}")
    })?;
    let after = device.slots(system)?;
    let new = after
        .iter()
        .find(|slot| slot.binding == Binding::Pcr7 && !before.iter().any(|old| old.token == slot.token))
        .ok_or_else(|| {
            format!("{name}: systemd-cryptenroll added no TPM2 slot bound to PCR 7, so the temporary slot stays")
        })?;
    device.opens(system, new).map_err(|error| {
        format!(
            "{name}: the new TPM2 slot {} does not open the volume, so the temporary slot stays: {error}",
            new.keyslot
        )
    })?;
    changes.push(format!(
        "{name}: enrolled TPM2 slot {} bound to PCR 7",
        new.keyslot
    ));
    let replaced: Vec<u32> = before
        .iter()
        .filter(|slot| matches!(slot.binding, Binding::Nothing | Binding::Pcr7))
        .map(|slot| slot.keyslot)
        .collect();
    let list = slot_list(&replaced);
    run(
        system,
        "systemd-cryptenroll",
        &[&format!("--wipe-slot={list}"), &path],
    )
    .map_err(|error| {
        format!("{name}: could not wipe the replaced TPM2 slots {list}, so the temporary slot stays: {error}")
    })?;
    changes.push(format!("{name}: wiped the replaced TPM2 slots {list}"));
    Ok(())
}

/// On each volume without a temporary slot, adds one, unlocking with the TPM2 slot it
/// has, so the volume still opens after a firmware or Secure Boot database update
/// changes PCR 7.
pub fn add_temporary_slots(system: &dyn System, volumes: &[Volume]) -> TpmReport {
    let mut report = TpmReport::default();
    for volume in volumes {
        let device = match Device::find(volume) {
            Ok(device) => device,
            Err(failure) => {
                report.failures.push(failure);
                continue;
            }
        };
        let before = match device.slots(system) {
            Ok(slots) => slots,
            Err(failure) => {
                report.failures.push(failure);
                continue;
            }
        };
        if !before.iter().any(|slot| slot.binding == Binding::Nothing) {
            match add_temporary(system, &device, &before) {
                Ok(change) => report.changes.push(change),
                Err(failure) => report.failures.push(failure),
            }
        }
        report.state_of(system, &device);
    }
    report
}

fn add_temporary(
    system: &dyn System,
    device: &Device,
    before: &[TpmSlot],
) -> Result<String, String> {
    let name = device.name();
    let path = device.path();
    run(
        system,
        "systemd-cryptenroll",
        &[
            "--unlock-tpm2-device=auto",
            "--tpm2-device=auto",
            "--tpm2-pcrs=",
            &path,
        ],
    )
    .map_err(|error| format!("{name}: could not add a temporary TPM2 slot: {error}"))?;
    let after = device.slots(system)?;
    let new = after
        .iter()
        .find(|slot| {
            slot.binding == Binding::Nothing && !before.iter().any(|old| old.token == slot.token)
        })
        .ok_or_else(|| format!("{name}: systemd-cryptenroll added no temporary TPM2 slot"))?;
    Ok(format!(
        "{name}: added temporary TPM2 slot {} bound to no PCR",
        new.keyslot
    ))
}

/// A volume that no TPM2 slot opens on this boot, with its dead TPM2 slots.
pub struct Unbound<'a> {
    device: Device<'a>,
    slots: Vec<TpmSlot>,
    dead: Vec<u32>,
}

impl Unbound<'_> {
    pub fn name(&self) -> &str {
        self.device.name()
    }
}

/// The volumes that no TPM2 slot opens on this boot, which `rebind` binds again.
pub fn unbound<'a>(
    system: &dyn System,
    volumes: &'a [Volume],
) -> Result<Vec<Unbound<'a>>, Vec<String>> {
    let mut found = Vec::new();
    let mut failures = Vec::new();
    for volume in volumes {
        let result = Device::find(volume).and_then(|device| {
            let slots = device.slots(system)?;
            let dead = device.dead_slots(system, &slots);
            Ok((device, slots, dead))
        });
        match result {
            Ok((device, slots, dead)) if !tpm_opens(&slots, &dead) => found.push(Unbound {
                device,
                slots,
                dead,
            }),
            Ok(_) => {}
            Err(failure) => failures.push(failure),
        }
    }
    if failures.is_empty() {
        Ok(found)
    } else {
        Err(failures)
    }
}

/// Enrolls a TPM2 slot bound to PCR 7 on each volume, unlocking it with the recovery key
/// in `key_file`, and wipes its dead TPM2 slots once the new slot is in. The caller makes
/// sure the system booted normally, in the Secure Boot state it was set up with.
pub fn rebind(system: &dyn System, volumes: &[Unbound], key_file: &Path) -> TpmReport {
    let mut report = TpmReport::default();
    let key = format!("--unlock-key-file={}", key_file.display());
    for volume in volumes {
        let name = volume.name();
        let path = volume.device.path();
        let wipe = format!("--wipe-slot={}", slot_list(&volume.dead));
        let mut args = vec![
            key.as_str(),
            "--tpm2-device=auto",
            "--tpm2-pcrs=7",
            path.as_str(),
        ];
        // systemd-cryptenroll wipes only once the new slot is in, and never the new slot.
        if !volume.dead.is_empty() {
            args.insert(3, &wipe);
        }
        match run(system, "systemd-cryptenroll", &args) {
            Ok(_) => {
                let added = volume.device.slots(system).ok().and_then(|after| {
                    after
                        .into_iter()
                        .find(|slot| !volume.slots.iter().any(|old| old.token == slot.token))
                });
                match added {
                    Some(slot) => report.changes.push(format!(
                        "{name}: enrolled TPM2 slot {} bound to PCR 7 with the recovery key",
                        slot.keyslot
                    )),
                    None => report.failures.push(format!(
                        "{name}: systemd-cryptenroll added no TPM2 slot bound to PCR 7"
                    )),
                }
                if !volume.dead.is_empty() {
                    report.changes.push(format!(
                        "{name}: wiped the TPM2 slots {} that no longer opened the volume",
                        slot_list(&volume.dead)
                    ));
                }
            }
            Err(error) => report.failures.push(format!(
                "{name}: could not bind the TPM to PCR 7 with the recovery key typed, so the volume still asks for it: {error}"
            )),
        }
    }
    report
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::fake::{FakeSystem, exit, ok};

    pub const DEVICE: &str = "/dev/disk/by-uuid/1111";

    /// A LUKS2 header's JSON with the recovery key in slot 0 and a TPM2 token for each
    /// `(token, keyslot, pcrs)`.
    pub fn header(tpm: &[(u32, u32, &str)]) -> String {
        let mut tokens =
            vec!["\"0\": {\"type\": \"systemd-recovery\", \"keyslots\": [\"0\"]}".to_owned()];
        for (token, keyslot, pcrs) in tpm {
            tokens.push(format!(
                "\"{token}\": {{\"type\": \"systemd-tpm2\", \"keyslots\": [\"{keyslot}\"], \"tpm2-blob\": \"AA==\", \"tpm2-pcrs\": [{pcrs}], \"tpm2-pcr-bank\": \"sha256\", \"tpm2-primary-alg\": \"ecc\", \"tpm2-policy-hash\": \"ab\"}}"
            ));
        }
        format!(
            "{{\"keyslots\": {{}}, \"tokens\": {{{}}}, \"segments\": {{}}, \"digests\": {{}}, \"config\": {{}}}}",
            tokens.join(", ")
        )
    }

    fn volume() -> Vec<Volume> {
        vec![Volume {
            name: "system".to_owned(),
            source: "UUID=1111".to_owned(),
        }]
    }

    fn dump() -> String {
        format!("cryptsetup luksDump --dump-json-metadata {DEVICE}")
    }

    fn test(token: u32) -> String {
        format!("cryptsetup open --test-passphrase --token-only --token-id {token} {DEVICE}")
    }

    #[test]
    fn crypttab_lines_name_the_volume_and_its_device() {
        let text = "# <name> <device> <password> <options>\nsystem UUID=1111 none tpm2-device=auto,discard\n\nhome   UUID=2222 none tpm2-device=auto,discard\n";
        assert_eq!(
            parse_crypttab(text),
            [
                Volume {
                    name: "system".to_owned(),
                    source: "UUID=1111".to_owned()
                },
                Volume {
                    name: "home".to_owned(),
                    source: "UUID=2222".to_owned()
                },
            ]
        );
    }

    #[test]
    fn tpm_slots_reads_the_systemd_tpm2_tokens_and_their_binding() {
        // A token with a PIN is not the guard's, even when bound to PCR 7 alone.
        let json = header(&[(1, 1, ""), (2, 3, "7"), (4, 4, "0,7")]).replace(
            "\"tokens\": {",
            "\"tokens\": {\"6\": {\"type\": \"systemd-tpm2\", \"keyslots\": [\"6\"], \"tpm2-pcrs\": [7], \"tpm2-pin\": true}, ",
        );
        assert_eq!(
            tpm_slots(&json),
            Ok(vec![
                TpmSlot {
                    token: 1,
                    keyslot: 1,
                    binding: Binding::Nothing
                },
                TpmSlot {
                    token: 2,
                    keyslot: 3,
                    binding: Binding::Pcr7
                },
                TpmSlot {
                    token: 4,
                    keyslot: 4,
                    binding: Binding::Other
                },
                TpmSlot {
                    token: 6,
                    keyslot: 6,
                    binding: Binding::Other
                },
            ])
        );
    }

    #[test]
    fn the_first_boot_replaces_the_temporary_slot_with_a_pcr7_slot() {
        let system = FakeSystem::default();
        system.respond_in_turn(
            &dump(),
            vec![
                ok(&header(&[(1, 1, "")])),
                ok(&header(&[(1, 1, ""), (2, 2, "7")])),
                ok(&header(&[(2, 2, "7")])),
            ],
        );
        let enroll = format!(
            "systemd-cryptenroll --unlock-tpm2-device=auto --tpm2-device=auto --tpm2-pcrs=7 {DEVICE}"
        );
        let opens = test(2);
        let wipe = format!("systemd-cryptenroll --wipe-slot=1 {DEVICE}");
        for command in [&enroll, &opens, &wipe] {
            system.respond(command, ok(""));
        }
        let report = bind_to_pcr7(&system, &volume(), Hold::None);
        assert_eq!(report.failures, Vec::<String>::new());
        assert_eq!(
            report.changes,
            [
                "system: enrolled TPM2 slot 2 bound to PCR 7",
                "system: wiped the replaced TPM2 slots 1"
            ]
        );
        assert_eq!(report.state, ["system: TPM2 slot 2 bound to PCR 7"]);
        let log = system.log.borrow();
        let position = |command: &str| log.iter().position(|line| line == command);
        assert!(position(&enroll) < position(&opens));
        assert!(position(&opens) < position(&wipe));
    }

    #[test]
    fn after_a_firmware_update_the_stale_pcr7_slot_goes_too() {
        let system = FakeSystem::default();
        system.respond_in_turn(
            &dump(),
            vec![
                ok(&header(&[(1, 2, "7"), (2, 1, "")])),
                ok(&header(&[(1, 2, "7"), (2, 1, ""), (3, 3, "7")])),
                ok(&header(&[(3, 3, "7")])),
            ],
        );
        system.respond(
            &format!("systemd-cryptenroll --unlock-tpm2-device=auto --tpm2-device=auto --tpm2-pcrs=7 {DEVICE}"),
            ok(""),
        );
        system.respond(&test(3), ok(""));
        system.respond(
            &format!("systemd-cryptenroll --wipe-slot=2,1 {DEVICE}"),
            ok(""),
        );
        let report = bind_to_pcr7(&system, &volume(), Hold::None);
        assert_eq!(report.failures, Vec::<String>::new());
        assert_eq!(
            report.changes[1],
            "system: wiped the replaced TPM2 slots 2,1"
        );
    }

    #[test]
    fn a_new_slot_that_does_not_open_the_volume_keeps_the_temporary_slot() {
        let system = FakeSystem::default();
        system.respond_in_turn(
            &dump(),
            vec![
                ok(&header(&[(1, 1, "")])),
                ok(&header(&[(1, 1, ""), (2, 2, "7")])),
            ],
        );
        system.respond(
            &format!("systemd-cryptenroll --unlock-tpm2-device=auto --tpm2-device=auto --tpm2-pcrs=7 {DEVICE}"),
            ok(""),
        );
        system.respond(&test(2), exit(2));
        system.respond(&test(1), ok(""));
        let report = bind_to_pcr7(&system, &volume(), Hold::None);
        assert_eq!(report.failures.len(), 1);
        assert!(
            report.failures[0].starts_with("system: the new TPM2 slot 2 does not open the volume")
        );
        assert!(
            !system
                .log
                .borrow()
                .iter()
                .any(|line| line.contains("--wipe-slot"))
        );
        assert_eq!(
            report.state,
            [
                "system: TPM2 slot 1 temporary, bound to no PCR; TPM2 slot 2 bound to PCR 7, does not open the volume"
            ]
        );
    }

    #[test]
    fn the_wrong_secure_boot_state_changes_nothing() {
        let system = FakeSystem::default();
        system.respond(&dump(), ok(&header(&[(1, 1, "")])));
        system.respond(&test(1), ok(""));
        let report = bind_to_pcr7(&system, &volume(), Hold::SecureBoot("Secure Boot is off"));
        assert_eq!(
            report.failures,
            [
                "system: Secure Boot is off, so the TPM2 slot is not bound to PCR 7 and the temporary slot stays"
            ]
        );
        assert_eq!(system.log.borrow().as_slice(), [dump(), dump(), test(1)]);
    }

    #[test]
    fn a_volume_without_a_temporary_slot_is_left_alone() {
        let system = FakeSystem::default();
        system.respond(&dump(), ok(&header(&[(1, 1, "7")])));
        system.respond(&test(1), ok(""));
        let report = bind_to_pcr7(&system, &volume(), Hold::SecureBoot("Secure Boot is off"));
        assert_eq!(
            report,
            TpmReport {
                state: vec!["system: TPM2 slot 1 bound to PCR 7".to_owned()],
                ..TpmReport::default()
            }
        );
    }

    #[test]
    fn a_firmware_update_adds_one_temporary_slot() {
        let system = FakeSystem::default();
        system.respond_in_turn(
            &dump(),
            vec![
                ok(&header(&[(1, 1, "7")])),
                ok(&header(&[(1, 1, "7"), (2, 2, "")])),
            ],
        );
        let add = format!(
            "systemd-cryptenroll --unlock-tpm2-device=auto --tpm2-device=auto --tpm2-pcrs= {DEVICE}"
        );
        system.respond(&add, ok(""));
        let report = add_temporary_slots(&system, &volume());
        assert_eq!(report.failures, Vec::<String>::new());
        assert_eq!(
            report.changes,
            ["system: added temporary TPM2 slot 2 bound to no PCR"]
        );

        // A second run finds the temporary slot and adds nothing.
        system.log.borrow_mut().clear();
        let report = add_temporary_slots(&system, &volume());
        assert!(report.changes.is_empty());
        assert!(!system.ran(&add));
    }

    #[test]
    fn a_pcr7_slot_that_no_longer_opens_the_volume_is_a_failure_that_names_rebind() {
        // The TPM was cleared: the PCR 7 slot is still in the header but cannot unseal.
        let system = FakeSystem::default();
        system.respond(&dump(), ok(&header(&[(1, 1, "7")])));
        system.respond(&test(1), exit(1));
        let report = bind_to_pcr7(&system, &volume(), Hold::None);
        assert_eq!(report.failures.len(), 1);
        assert!(
            report.failures[0].starts_with("system: no TPM2 slot opens the volume on this boot")
        );
        assert!(report.failures[0].contains("sudo inayos-guard rebind"));
        assert_eq!(
            report.state,
            ["system: TPM2 slot 1 bound to PCR 7, does not open the volume"]
        );
    }

    #[test]
    fn a_firmware_update_prepared_during_this_boot_keeps_its_temporary_slot() {
        let system = FakeSystem::default();
        system.respond(&dump(), ok(&header(&[(1, 1, "7"), (2, 2, "")])));
        system.respond(&test(1), ok(""));
        system.respond(&test(2), ok(""));
        let report = bind_to_pcr7(&system, &volume(), Hold::FirmwareUpdate);
        assert_eq!(report.failures, Vec::<String>::new());
        assert!(report.changes.is_empty());
        assert_eq!(report.notes.len(), 1);
        assert!(
            !system
                .log
                .borrow()
                .iter()
                .any(|line| line.starts_with("systemd-cryptenroll"))
        );
    }

    #[test]
    fn rebind_enrolls_with_the_recovery_key_and_wipes_the_dead_slots() {
        let system = FakeSystem::default();
        system.respond_in_turn(
            &dump(),
            vec![ok(&header(&[(1, 1, "7")])), ok(&header(&[(2, 2, "7")]))],
        );
        system.respond(&test(1), exit(1));
        let enroll = format!(
            "systemd-cryptenroll --unlock-key-file=/run/key --tpm2-device=auto --tpm2-pcrs=7 --wipe-slot=1 {DEVICE}"
        );
        system.respond(&enroll, ok(""));
        let volumes = volume();
        let found = unbound(&system, &volumes).expect("headers read");
        assert_eq!(found.len(), 1);
        let report = rebind(&system, &found, Path::new("/run/key"));
        assert_eq!(report.failures, Vec::<String>::new());
        assert_eq!(
            report.changes,
            [
                "system: enrolled TPM2 slot 2 bound to PCR 7 with the recovery key",
                "system: wiped the TPM2 slots 1 that no longer opened the volume"
            ]
        );
        assert!(system.ran(&enroll));
    }

    #[test]
    fn a_volume_the_tpm_still_opens_needs_no_rebind() {
        let system = FakeSystem::default();
        system.respond(&dump(), ok(&header(&[(1, 1, "7")])));
        system.respond(&test(1), ok(""));
        let volumes = volume();
        assert!(unbound(&system, &volumes).expect("headers read").is_empty());
    }
}
