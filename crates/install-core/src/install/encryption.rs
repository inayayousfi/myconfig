//! The encrypted layout: the system and home partitions as LUKS2 volumes. The install opens
//! them with a random temporary key that lives only in the live system's `/run`. Once the
//! new system is configured, the volumes get one recovery key for the user and a TPM slot
//! bound to no measurement, and the temporary key leaves both. The guard of the new system
//! binds the TPM slot to the Secure Boot state at its first boot.
use std::{fmt, path::Path};

use super::{Installed, Machine, TARGET, user::NewUser};
use crate::{Error, host::Command, parse};

/// The live system's folder for the temporary key, readable by root only. `/run` is a
/// memory filesystem, so the key never reaches a disk.
const KEY_FOLDER: &str = "/run/inayos-installer-keys";
const TEMPORARY_KEY: &str = "/run/inayos-installer-keys/temporary.key";
/// The keyslot that `luksFormat` gives the temporary key.
const TEMPORARY_SLOT: &str = "0";
/// The mapper names of the opened volumes, which the new system's crypttab uses too.
pub(crate) const SYSTEM_NAME: &str = "system";
pub(crate) const HOME_NAME: &str = "home";
/// The new system's files that open the volumes and point the kernel at the root.
const CRYPTTAB: &str = "etc/crypttab.initramfs";
const KERNEL_CMDLINE: &str = "etc/kernel/cmdline";
/// The copy of the recovery key in the user's home folder, relative to it.
pub(crate) const RECOVERY_KEY_FILE: &str = "inayos-recovery-key.txt";

/// The recovery key that opens both encrypted volumes. Its `Debug` form hides it, so it
/// never reaches a log by accident.
#[derive(Clone, PartialEq, Eq)]
pub struct RecoveryKey(String);

impl RecoveryKey {
    /// The key as the user types it: eight groups of eight letters joined by `-`.
    pub fn text(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for RecoveryKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("<hidden>")
    }
}

/// The two LUKS2 partitions.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Volumes {
    pub system: String,
    pub home: String,
}

pub(crate) fn mapper(name: &str) -> String {
    format!("/dev/mapper/{name}")
}

/// Creates the random temporary key in the live system's `/run`.
pub(crate) fn create_temporary_key(m: &Machine) -> Result<(), Error> {
    m.create_dir(Path::new(KEY_FOLDER), 0o700)?;
    m.run(Command::action("dd").args([
        "if=/dev/urandom",
        &format!("of={TEMPORARY_KEY}"),
        "bs=64",
        "count=1",
        "iflag=fullblock",
        "status=none",
    ]))?;
    Ok(())
}

/// Formats `partition` as a LUKS2 volume labeled `label`, opened with the temporary key,
/// and opens it as `/dev/mapper/<name>`, which it returns.
pub(crate) fn format_volume(
    m: &Machine,
    partition: &str,
    label: &str,
    name: &str,
) -> Result<String, Error> {
    m.run(Command::action("cryptsetup").args([
        "luksFormat",
        "--batch-mode",
        "--type",
        "luks2",
        "--key-file",
        TEMPORARY_KEY,
        "--key-slot",
        TEMPORARY_SLOT,
        "--label",
        label,
        partition,
    ]))?;
    m.run(Command::action("cryptsetup").args([
        "open",
        "--key-file",
        TEMPORARY_KEY,
        partition,
        name,
    ]))?;
    Ok(mapper(name))
}

/// The new system's `/etc/crypttab.initramfs`: the initramfs opens both volumes with the
/// TPM, and asks for the recovery key when the TPM refuses.
pub(crate) fn crypttab(system_uuid: &str, home_uuid: &str) -> String {
    format!(
        "# Written by the InayOS installer. The initramfs opens both volumes, so the home\n\
         # volume, which holds the hibernation swap file, is open before resume.\n\
         {SYSTEM_NAME} UUID={system_uuid} none tpm2-device=auto,discard\n\
         {HOME_NAME} UUID={home_uuid} none tpm2-device=auto,discard\n"
    )
}

/// The kernel options that mkinitcpio puts into the unified kernel image. The bootvine
/// module adds its own options to this line.
pub(crate) const CMDLINE: &str = "root=/dev/mapper/system rootflags=subvol=@ rw\n";

fn luks_uuid(m: &Machine, partition: &str) -> Result<String, Error> {
    let command = Command::query("cryptsetup").args(["luksUUID", partition]);
    let shown = command.shown();
    let uuid = m.query(command)?.trim().to_owned();
    if uuid.is_empty() || uuid.contains(char::is_whitespace) {
        return Err(Error::UnexpectedOutput {
            command: shown,
            problem: format!("{uuid:?} is not a UUID"),
        });
    }
    Ok(uuid)
}

/// Writes the files that open the volumes at boot and point the kernel at the root, before
/// the modules build the initramfs and the unified kernel image from them.
pub(crate) fn write_boot_files(m: &Machine, volumes: &Volumes) -> Result<(), Error> {
    let system = luks_uuid(m, &volumes.system)?;
    let home = luks_uuid(m, &volumes.home)?;
    let target = Path::new(TARGET);
    m.write(&target.join(CRYPTTAB), &crypttab(&system, &home), 0o600)?;
    m.create_dir(&target.join("etc/kernel"), 0o755)?;
    m.write(&target.join(KERNEL_CMDLINE), CMDLINE, 0o644)?;
    Ok(())
}

/// The text of the recovery key file.
pub(crate) fn recovery_key_text(key: &RecoveryKey) -> String {
    format!(
        "InayOS recovery key\n\
         \n\
         {}\n\
         \n\
         This key opens the encrypted disk of this computer when the TPM does not, such as\n\
         after a firmware update, a cleared TPM, or on another computer. Type it at the\n\
         \"passphrase or recovery key\" prompt when the computer starts.\n\
         \n\
         Copy it into your password manager, then delete this file. It is only a copy, and\n\
         inside the encrypted disk it cannot help when the disk does not open.\n",
        key.text()
    )
}

/// Gives both volumes the recovery key and a TPM slot bound to no measurement, checks that
/// each opens both volumes, saves a copy of the key in the user's home folder, then takes the
/// temporary key out of both. Puts the key and that copy's path in the new system into
/// `made` as soon as each exists, so a failure still hands them over. The key never reaches
/// the sink.
pub(crate) fn enroll(
    m: &Machine,
    volumes: &Volumes,
    user: &NewUser,
    made: &mut Installed,
) -> Result<(), Error> {
    let unlock = format!("--unlock-key-file={TEMPORARY_KEY}");
    let partitions = [&volumes.system, &volumes.home];
    m.sink.step("Creating the recovery key");
    let output = m.query(
        Command::query("systemd-cryptenroll")
            .args([unlock.as_str(), "--recovery-key", volumes.system.as_str()])
            .hidden_output(),
    )?;
    let key = RecoveryKey(parse::recovery_key(&output).ok_or(Error::NoRecoveryKey)?);
    made.recovery_key = Some(key.clone());
    // The same passphrase opens home, so one key opens both volumes. cryptsetup reads the
    // whole new key file, so the key goes in without a line break.
    m.run(
        Command::action("cryptsetup")
            .args([
                "luksAddKey",
                "--key-file",
                TEMPORARY_KEY,
                volumes.home.as_str(),
                "/dev/stdin",
            ])
            .stdin(key.text()),
    )?;
    m.sink
        .step("Letting the TPM open the encrypted volumes until the first boot");
    for partition in partitions {
        m.run(Command::action("systemd-cryptenroll").args([
            unlock.as_str(),
            "--tpm2-device=auto",
            "--tpm2-pcrs=",
            partition.as_str(),
        ]))?;
    }
    // The temporary key leaves only once the recovery key and the TPM each open both
    // volumes, so a wrong enrollment never locks the user out.
    m.sink
        .step("Checking that the recovery key and the TPM open both encrypted volumes");
    for partition in partitions {
        // `--key-file=-` reads the whole input as the key, as luksAddKey stored it.
        m.run(
            Command::query("cryptsetup")
                .args([
                    "open",
                    "--test-passphrase",
                    "--key-file=-",
                    partition.as_str(),
                ])
                .stdin(key.text()),
        )?;
        m.run(Command::query("cryptsetup").args([
            "open",
            "--test-passphrase",
            "--token-only",
            "--token-type",
            "systemd-tpm2",
            partition.as_str(),
        ]))?;
    }
    let home = Path::new("/home").join(&user.name);
    let copy = home.join(RECOVERY_KEY_FILE);
    let in_target = Path::new(TARGET).join(copy.strip_prefix("/").expect("the path is absolute"));
    m.write(&in_target, &recovery_key_text(&key), 0o600)?;
    m.chown(&in_target, user.uid, user.gid)?;
    made.recovery_key_file = Some(copy.clone());
    m.sink
        .step("Taking the installer's temporary key out of the encrypted volumes");
    for partition in partitions {
        m.run(
            Command::action("systemd-cryptenroll")
                .args([format!("--wipe-slot={TEMPORARY_SLOT}"), partition.clone()]),
        )?;
    }
    m.sink.step(&format!(
        "The recovery key is saved in {}: copy it into a password manager, then delete the file",
        copy.display()
    ));
    Ok(())
}

/// Closes the volumes that the install opened and deletes the temporary key. Runs after the
/// new system is unmounted, and returns every step that failed.
pub(crate) fn close(m: &Machine) -> Vec<Error> {
    let mut errors = Vec::new();
    for name in [HOME_NAME, SYSTEM_NAME] {
        if m.exists(Path::new(&mapper(name)))
            && let Err(error) = m.run(Command::action("cryptsetup").args(["close", name]))
        {
            errors.push(error);
        }
    }
    let folder = Path::new(KEY_FOLDER);
    if m.exists(folder)
        && let Err(error) = m.remove_all(folder)
    {
        errors.push(error);
    }
    errors
}
