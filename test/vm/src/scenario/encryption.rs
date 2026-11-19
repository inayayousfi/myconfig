//! The steps of the encryption and hibernation scenarios: the checks of the TPM unlock and its
//! enrollment, the recovery key, the boot that needs it, hibernation and resume, the install
//! that a hibernated disk refuses, and the Secure Boot database update that the guard prepares
//! the volumes for.

use std::fs;
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use super::answers_file::DiskChoice;
use super::boot_chain::GUARD;
use super::encryption_checks::{self as checks, HIBERNATE_LOCATION, HIBERNATED_MESSAGE, Tpm2Token};
use super::installed;
use super::{DEVICE, Scenario, StepError, sudo};
use crate::machine::{Media, SSH_PORT};
use crate::os::GUEST_USER;
use crate::ppm;
use crate::ssh::{Guest, SshError};

/// Where the installer saves the recovery key, in the new user's home folder.
const RECOVERY_KEY_FILE: &str = "inayos-recovery-key.txt";
/// The mapper names that /etc/crypttab.initramfs opens the volumes as.
const SYSTEM_MAPPER: &str = "/dev/mapper/system";
const HOME_MAPPER: &str = "/dev/mapper/home";
/// Words of systemd-cryptsetup's password prompt, which must not show in a TPM unlock.
const PROMPT_WORDS: [&str; 2] = ["passphrase", "recovery key"];
/// The firmware's few seconds, rEFInd's 20-second wait and the kernel come before the prompt.
const PROMPT_EARLIEST: Duration = Duration::from_secs(45);
/// When the screen keeps changing, the key is typed after this time anyway.
const PROMPT_LATEST: Duration = Duration::from_secs(150);
/// The prompt counts as shown when two screens this far apart are the same.
const PROMPT_SETTLE: Duration = Duration::from_secs(5);
/// How often the recovery key is typed. systemd-cryptsetup opens both volumes at once, and
/// the second one asks too when it asked before the first answer arrived.
const RECOVERY_ATTEMPTS: u32 = 3;
const UNLOCK_TIMEOUT: Duration = Duration::from_secs(3 * 60);
const SWAP_FILE: &str = "/home/swapfile";
const SWAP_TIMEOUT: Duration = Duration::from_secs(3 * 60);
/// Writing the memory image of 8 GiB of memory to the swap file takes a while.
const HIBERNATE_TIMEOUT: Duration = Duration::from_secs(10 * 60);
/// Where the live system keeps the HibernateLocation variable while the installer runs
/// without it.
const SAVED_HIBERNATE_LOCATION: &str = "/root/hibernate-location";
/// The Secure Boot database variable, which changes PCR 7 at the next boot.
const DB_VARIABLE: &str = "/sys/firmware/efi/efivars/db-d719b2cb-3d3a-4596-a3bc-dad00e67656f";
/// Makes a throwaway certificate that the install did not enroll, in the folder whose files
/// sbctl's `--custom` adds to db as X.509 certificates.
const TEST_CERTIFICATE: &str = "sh -c 'mkdir -p /var/lib/sbctl/keys/custom/db \
    && openssl req -x509 -newkey rsa:2048 -nodes -days 1 -subj /CN=inayos-test-db \
    -keyout /root/inayos-test-db.key -outform DER -out /var/lib/sbctl/keys/custom/db/inayos-test.der'";
/// Appends that certificate to db, signed with the machine's KEK that sbctl holds, which works
/// with Secure Boot on. The firmware marks its variables immutable in efivarfs, and
/// `--ignore-immutable` clears that flag first.
const DB_UPDATE: &str = "sbctl enroll-keys --partial db --append --custom --ignore-immutable";
/// Prints the SHA-256 of the first MiB of the disk and of each partition, which hold the
/// partition table and every filesystem's and LUKS header's superblock, and of the disk's last
/// MiB, which holds the backup partition table. Argument: the disk.
pub(super) const HEADS_SCRIPT: &str = r#"set -e
disk=$1
for device in "$disk" "$disk"[0-9]*; do
    printf '%s %s\n' "$device" "$(dd if="$device" bs=1M count=1 status=none | sha256sum | cut -d' ' -f1)"
done
sectors=$(( $(blockdev --getsize64 "$disk") / 512 ))
printf 'end %s\n' "$(dd if="$disk" bs=512 skip=$(( sectors - 2048 )) count=2048 status=none | sha256sum | cut -d' ' -f1)"
"#;

/// An install that the installer's checks stage must refuse: its disk answers, the message the
/// refusal holds, the command that prints the checksums of the disk's heads, and its output
/// before the install.
pub(super) struct Refusal<'a> {
    pub disk: &'a DiskChoice,
    pub message: &'a str,
    pub heads: &'a str,
    pub before: &'a str,
}

/// A process started before hibernating, with the boot it runs in. The same process in the
/// same boot after the next start shows that the machine resumed.
pub(super) struct Marker {
    argument: String,
    pid: String,
    boot_id: String,
}

/// The TPM2 tokens of one LUKS volume.
pub(super) struct Volume {
    device: String,
    tokens: Vec<Tpm2Token>,
}

impl Scenario {
    /// Checks that the boot that just reached SSH asked for no password: nothing was typed, so a
    /// prompt would have stopped the boot, and the serial log shows none. Saves a screenshot.
    pub(super) fn check_unlocked_without_prompt(&mut self, label: &str) -> Result<(), StepError> {
        let serial = self
            .layer
            .dir()
            .join(format!("serial-boot-{}.log", self.boots));
        let text = fs::read_to_string(&serial)
            .unwrap_or_default()
            .to_lowercase();
        if let Some(word) = PROMPT_WORDS.iter().find(|word| text.contains(*word)) {
            return Err(StepError::Check(format!(
                "the serial log {} shows a password prompt ({word:?}) on a boot that the TPM should unlock",
                serial.display()
            )));
        }
        self.screenshot(&format!("unlocked-{label}.png"))?;
        println!("vm: the TPM unlocked the disk with no prompt");
        Ok(())
    }

    /// Reads the recovery key from the new user's home folder, checks that only the user can
    /// read the file and that no installer log holds the key, and saves it with the copy of the
    /// installed disk, where the scenarios that need it read it.
    pub(super) fn save_recovery_key(&mut self, user: &Guest) -> Result<(), StepError> {
        println!("vm: reading the recovery key from ~/{RECOVERY_KEY_FILE}");
        let mode = user.run(&format!("stat -c '%a %U' ~/{RECOVERY_KEY_FILE}"))?;
        if mode.trim() != format!("600 {GUEST_USER}") {
            return Err(StepError::Check(format!(
                "~/{RECOVERY_KEY_FILE} should have mode 600 and owner {GUEST_USER}, but has {:?}",
                mode.trim()
            )));
        }
        let text = user.run(&format!("cat ~/{RECOVERY_KEY_FILE}"))?;
        let key = checks::recovery_key(&text)
            .ok_or_else(|| {
                StepError::Check(format!(
                    "~/{RECOVERY_KEY_FILE} holds no recovery key of eight groups of eight modhex characters"
                ))
            })?
            .to_owned();
        let logs = self.layer.dir().join("logs");
        for entry in fs::read_dir(&logs).map_err(|error| StepError::Io(logs.clone(), error))? {
            let path = entry
                .map_err(|error| StepError::Io(logs.clone(), error))?
                .path();
            let bytes = fs::read(&path).map_err(|error| StepError::Io(path.clone(), error))?;
            if String::from_utf8_lossy(&bytes).contains(&key) {
                return Err(StepError::Check(format!(
                    "the installer wrote the recovery key into {}",
                    path.display()
                )));
            }
        }
        let copy = self
            .installed_copy
            .clone()
            .expect("the scenario that reads the recovery key keeps the installed disk");
        let file = installed::recovery_key(&copy);
        fs::write(&file, format!("{key}\n")).map_err(|error| StepError::Io(file.clone(), error))?;
        println!("vm: the recovery key is saved in {}", file.display());
        Ok(())
    }

    /// Checks that the root and home are on the mapper devices that crypttab.initramfs names,
    /// and reads the TPM2 tokens of each LUKS volume, keeping a copy of each dump named after
    /// `label` in the layer.
    pub(super) fn volumes(&mut self, user: &Guest, label: &str) -> Result<Vec<Volume>, StepError> {
        let sources = user.run("findmnt -no SOURCE / && findmnt -no SOURCE /home")?;
        let mut lines = sources.lines();
        let (root, home) = (lines.next().unwrap_or(""), lines.next().unwrap_or(""));
        if !root.starts_with(SYSTEM_MAPPER) || home != HOME_MAPPER {
            return Err(StepError::Check(format!(
                "/ and /home come from {root:?} and {home:?}, not {SYSTEM_MAPPER} and {HOME_MAPPER}"
            )));
        }
        let devices =
            checks::luks_devices(&user.run(&format!("lsblk -nrpo NAME,FSTYPE {DEVICE}"))?);
        if devices.len() != 2 {
            return Err(StepError::Check(format!(
                "{DEVICE} should hold two LUKS volumes, system and home, but holds {devices:?}"
            )));
        }
        let mut volumes = Vec::new();
        for device in devices {
            let json = user.run(&sudo(&format!(
                "cryptsetup luksDump --dump-json-metadata {device}"
            )))?;
            let name = device.trim_start_matches("/dev/");
            self.write(&format!("luks-{label}-{name}.json"), &json)?;
            let tokens = checks::tpm2_tokens(&json)
                .map_err(|problem| StepError::Check(format!("{device}: {problem}")))?;
            volumes.push(Volume { device, tokens });
        }
        Ok(volumes)
    }

    /// Checks that each volume has a TPM2 token bound to PCR 7 and no temporary one, and
    /// returns the volumes.
    pub(super) fn check_tpm_enrolled(
        &mut self,
        user: &Guest,
        label: &str,
    ) -> Result<Vec<Volume>, StepError> {
        println!("vm: checking the TPM2 enrollment of both volumes");
        let volumes = self.volumes(user, label)?;
        for volume in &volumes {
            if let Some(problem) = checks::enrollment_problem(&volume.tokens) {
                return Err(StepError::Check(format!("{}: {problem}", volume.device)));
            }
        }
        println!("vm: both volumes have a TPM2 token bound to PCR 7 and no temporary one");
        Ok(volumes)
    }

    /// Checks that `inayos-guard status` reports the PCR 7 enrollment of the first boot.
    pub(super) fn check_guard_reports_pcr7(&mut self, user: &Guest) -> Result<(), StepError> {
        let status = user.run(&sudo(&format!("{GUARD} status")))?;
        self.write("guard-status-pcr7.txt", &status)?;
        if !checks::mentions_pcr7(&status) {
            return Err(StepError::Check(format!(
                "inayos-guard status does not report the PCR 7 enrollment:\n{status}"
            )));
        }
        Ok(())
    }

    /// Replaces the layer's TPM state with an empty one, as a cleared TPM or another machine
    /// would be: nothing sealed before opens with it.
    pub(super) fn replace_tpm_state(&mut self) -> Result<(), StepError> {
        let tpm = self.layer.tpm();
        println!("vm: replacing the TPM state with an empty one");
        fs::remove_dir_all(&tpm).map_err(|error| StepError::Io(tpm.clone(), error))?;
        fs::create_dir_all(&tpm).map_err(|error| StepError::Io(tpm, error))
    }

    /// The recovery key that the run which made the kept encrypted disk saved.
    pub(super) fn kept_recovery_key(&self) -> Result<String, StepError> {
        let installed = self
            .installed
            .as_ref()
            .expect("the scenario starts from the kept encrypted disk");
        let file = installed::recovery_key(installed);
        let text = fs::read_to_string(&file).map_err(|error| StepError::Io(file.clone(), error))?;
        checks::recovery_key(&text)
            .map(str::to_owned)
            .ok_or_else(|| StepError::Check(format!("{} holds no recovery key", file.display())))
    }

    /// Boots the installed disk, waits for bootvine's password prompt, types `key` through QMP,
    /// and waits until the new user accepts SSH. The system must not answer SSH before the key
    /// is typed.
    pub(super) fn boot_with_recovery_key(&mut self, key: &str) -> Result<Guest, StepError> {
        println!("vm: booting the installed system, which needs the recovery key");
        self.start(Media::default())?;
        let guest = Guest::new(self.key.clone(), GUEST_USER, SSH_PORT);
        thread::sleep(PROMPT_EARLIEST);
        for attempt in 1..=RECOVERY_ATTEMPTS {
            self.wait_for_prompt(&guest)?;
            self.screenshot(&format!("recovery-prompt-{attempt}.png"))?;
            println!("vm: typing the recovery key, attempt {attempt}");
            self.vm().qmp()?.type_text(&format!("{key}\n"))?;
            match guest.wait_until_ready(self.vm(), UNLOCK_TIMEOUT) {
                Ok(()) => {
                    println!("vm: the recovery key unlocked the disk");
                    return Ok(guest);
                }
                Err(SshError::Timeout(_)) => {}
                Err(error) => return Err(error.into()),
            }
        }
        Err(StepError::Check(format!(
            "the system did not accept SSH after the recovery key was typed {RECOVERY_ATTEMPTS} times"
        )))
    }

    /// Waits until the screen shows something and stays the same for a moment, which a waiting
    /// prompt does, or until the prompt's latest time. Fails when the system answers SSH, since
    /// it then booted without the recovery key.
    fn wait_for_prompt(&mut self, guest: &Guest) -> Result<(), StepError> {
        let deadline = Instant::now() + PROMPT_LATEST;
        let check = self.layer.dir().join("prompt-check.ppm");
        let mut previous: Option<Vec<u8>> = None;
        loop {
            self.vm().check_running()?;
            if guest.run("exit 0").is_ok() {
                return Err(StepError::Check(
                    "the system answered SSH before the recovery key was typed, so the replaced TPM still unlocked it"
                        .to_owned(),
                ));
            }
            self.vm().qmp()?.screendump_ppm(&check)?;
            let screen = fs::read(&check).map_err(|error| StepError::Io(check.clone(), error))?;
            let drawn = ppm::is_single_color(&screen).is_ok_and(|single| !single);
            if drawn && previous.as_ref() == Some(&screen) {
                println!("vm: the screen waits, as the password prompt does");
                break;
            }
            if Instant::now() >= deadline {
                println!("vm: the screen still changes; typing the key anyway");
                break;
            }
            previous = Some(screen);
            thread::sleep(PROMPT_SETTLE);
        }
        fs::remove_file(&check).map_err(|error| StepError::Io(check, error))
    }

    /// Checks that both volumes are open after a boot.
    pub(super) fn check_volumes_open(&mut self, user: &Guest) -> Result<(), StepError> {
        self.volumes(user, "recovered")?;
        println!("vm: both volumes are open");
        Ok(())
    }

    /// Waits until the swap file is an active swap, which hibernation writes to.
    pub(super) fn wait_for_swap_file(&mut self, user: &Guest) -> Result<(), StepError> {
        println!("vm: waiting until {SWAP_FILE} is an active swap");
        self.wait_until(
            &format!("{SWAP_FILE} is an active swap"),
            SWAP_TIMEOUT,
            || {
                let names = user.run("swapon --noheadings --show=NAME")?;
                Ok(if names.lines().any(|name| name.trim() == SWAP_FILE) {
                    Ok(())
                } else {
                    Err(names.trim().replace('\n', ", "))
                })
            },
        )
    }

    /// Starts a `sleep` with an argument no other process has, outside the SSH session.
    pub(super) fn start_marker(&mut self, user: &Guest) -> Result<Marker, StepError> {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|time| time.subsec_nanos())
            .unwrap_or_default();
        let argument = format!("100000.{unique}");
        user.run(&format!(
            "setsid -f sleep {argument} </dev/null >/dev/null 2>&1"
        ))?;
        let pid = user.run(&format!("sleep 1; pgrep -fx 'sleep {argument}'"))?;
        let boot_id = user.run("cat /proc/sys/kernel/random/boot_id")?;
        println!(
            "vm: the marker process sleep {argument} runs as {}",
            pid.trim()
        );
        Ok(Marker {
            argument,
            pid: pid.trim().to_owned(),
            boot_id: boot_id.trim().to_owned(),
        })
    }

    /// Hibernates the machine and waits until QEMU stops.
    pub(super) fn hibernate(&mut self, user: &Guest) -> Result<(), StepError> {
        println!("vm: hibernating");
        self.power_off_within(user, &sudo("systemctl hibernate"), HIBERNATE_TIMEOUT)
    }

    /// Checks that the system resumed: the marker process runs with its pid in the same boot,
    /// and the HibernateLocation variable is gone.
    pub(super) fn check_resumed(&mut self, user: &Guest, marker: &Marker) -> Result<(), StepError> {
        println!("vm: checking that the system resumed");
        let boot_id = user.run("cat /proc/sys/kernel/random/boot_id")?;
        let pid = user.run(&format!("pgrep -fx 'sleep {}' || true", marker.argument))?;
        if boot_id.trim() != marker.boot_id || pid.trim() != marker.pid {
            return Err(StepError::Check(format!(
                "the system started fresh instead of resuming: boot id {} and marker pid {:?}, before hibernating {} and {}",
                boot_id.trim(),
                pid.trim(),
                marker.boot_id,
                marker.pid
            )));
        }
        if user
            .run(&format!("test ! -e {HIBERNATE_LOCATION}"))
            .is_err()
        {
            return Err(StepError::Check(format!(
                "the system resumed, but {HIBERNATE_LOCATION} is still there"
            )));
        }
        println!("vm: the system resumed with the marker process, and HibernateLocation is gone");
        Ok(())
    }

    /// On the live system of a hibernated disk: checks that the firmware variable shows, saves
    /// the checksums of the disk's heads, and runs the installer with reinstall answers twice:
    /// once with the variable, and once without it, which leaves only the swap file's
    /// hibernation signature to refuse the disk. Each run must refuse in the checks stage
    /// without writing to the disk. The variable is put back afterwards, so the installed
    /// system still resumes.
    pub(super) fn check_hibernated_disk_refused(&mut self, live: &Guest) -> Result<(), StepError> {
        if live.run(&format!("test -e {HIBERNATE_LOCATION}")).is_err() {
            return Err(StepError::Check(format!(
                "the live system does not see {HIBERNATE_LOCATION}: either the machine did not \
                 hibernate, or the live system's systemd-hibernate-clear.service cleared it"
            )));
        }
        let script = self.write("disk-heads.sh", HEADS_SCRIPT)?;
        live.copy_to(&script, "/root/disk-heads.sh")?;
        let heads = format!("bash /root/disk-heads.sh {DEVICE}");
        let before = live.run(&heads)?;
        self.write("disk-heads-before.txt", &before)?;

        println!("vm: installing on the hibernated disk, with the firmware variable");
        let reinstall = DiskChoice::ReinstallKeepHome {
            device: DEVICE,
            efi_partition: "/dev/vda1",
            system_partition: "/dev/vda2",
            home_partition: "/dev/vda3",
        };
        let refusal = Refusal {
            disk: &reinstall,
            message: HIBERNATED_MESSAGE,
            heads: &heads,
            before: &before,
        };
        self.check_install_refused(live, "reinstall-hibernated", &refusal)?;

        println!("vm: installing on the hibernated disk, with only the swap file's signature");
        // The saved file keeps the variable's 4 attribute bytes, which a write needs back.
        live.run(&format!(
            "cp {HIBERNATE_LOCATION} {SAVED_HIBERNATE_LOCATION} \
             && chattr -i {HIBERNATE_LOCATION} && rm {HIBERNATE_LOCATION}"
        ))?;
        let refused = self.check_install_refused(live, "reinstall-hibernated-swap-file", &refusal);
        let restored = live.run(&format!(
            "cat {SAVED_HIBERNATE_LOCATION} > {HIBERNATE_LOCATION}"
        ));
        refused?;
        restored?;
        println!("vm: the installer refused the hibernated disk both times and left it unchanged");
        Ok(())
    }

    /// Runs the installer with the answers of `refusal` on the live system and checks that its
    /// checks stage refused the disk with the expected message, that it ran no command that
    /// writes to the disk, and that the disk is unchanged and unmounted.
    pub(super) fn check_install_refused(
        &mut self,
        live: &Guest,
        label: &str,
        refusal: &Refusal,
    ) -> Result<(), StepError> {
        let result = self.run_installer(live, refusal.disk, label)?;
        if result.is_ok() {
            return Err(StepError::Check(format!(
                "{label}: the installer finished on a disk it should refuse"
            )));
        }
        let read = |name: String| {
            let path = self.log(&name);
            fs::read(&path)
                .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
                .map_err(|error| StepError::Io(path, error))
        };
        let output = read(format!("installer-{label}"))?;
        let file_log = read(format!("installer-{label}-file"))?;
        if let Some(problem) =
            checks::refusal_problem(&format!("{output}\n{file_log}"), refusal.message)
        {
            return Err(StepError::Check(format!("{label}: {problem}")));
        }
        let writes = checks::disk_writes(&file_log);
        if !writes.is_empty() {
            return Err(StepError::Check(format!(
                "{label}: the refused install ran commands that write to the disk: {}",
                writes.join("; ")
            )));
        }
        self.check_disk_unchanged(live, label, refusal)
    }

    /// Checks that the disk's heads are still as `refusal.before` lists them and that nothing
    /// of the disk is mounted, after the step named `label`.
    pub(super) fn check_disk_unchanged(
        &mut self,
        live: &Guest,
        label: &str,
        refusal: &Refusal,
    ) -> Result<(), StepError> {
        let after = live.run(refusal.heads)?;
        self.write(&format!("disk-heads-after-{label}.txt"), &after)?;
        let changed = checks::changed_heads(refusal.before, &after);
        if !changed.is_empty() {
            return Err(StepError::Check(format!(
                "{label}: the refused install changed the start of {}",
                changed.join(", ")
            )));
        }
        let device = refusal.disk.device();
        let mounted = live.run(&format!("findmnt -rn -o SOURCE | grep '^{device}' || true"))?;
        if !mounted.trim().is_empty() {
            return Err(StepError::Check(format!(
                "{label}: partitions of the refused disk are still mounted: {}",
                mounted.trim()
            )));
        }
        Ok(())
    }

    /// Checks that the volumes have a temporary TPM2 token after the guard prepared them for a
    /// firmware update.
    pub(super) fn prepare_firmware_update(&mut self, user: &Guest) -> Result<(), StepError> {
        println!("vm: preparing the volumes for a firmware update");
        user.run_logged(
            &sudo(&format!("{GUARD} prepare-firmware-update")),
            &self.log("guard-prepare-firmware-update"),
        )?;
        for volume in self.volumes(user, "prepared")? {
            if !checks::has_temporary_token(&volume.tokens) {
                return Err(StepError::Check(format!(
                    "{}: prepare-firmware-update added no temporary TPM2 token",
                    volume.device
                )));
            }
        }
        Ok(())
    }

    /// Adds a certificate to the Secure Boot database, which changes PCR 7 at the next boot.
    pub(super) fn update_secure_boot_db(&mut self, user: &Guest) -> Result<(), StepError> {
        println!("vm: adding certificates to the Secure Boot database");
        let sum = format!("sha256sum {DB_VARIABLE}");
        let before = user.run(&sudo(&sum))?;
        user.run_logged(&sudo(TEST_CERTIFICATE), &self.log("db-test-certificate"))?;
        user.run_logged(&sudo(DB_UPDATE), &self.log("db-update"))?;
        let after = user.run(&sudo(&sum))?;
        if before == after {
            return Err(StepError::Check(format!(
                "{DB_UPDATE} left the Secure Boot database unchanged"
            )));
        }
        Ok(())
    }
}

/// The volumes of `after` whose PCR 7 tokens open no keyslot that `before` had: check-boot
/// enrolled them again.
pub(super) fn missing_reenrollment(before: &[Volume], after: &[Volume]) -> Vec<String> {
    after
        .iter()
        .filter(|volume| {
            let old = before
                .iter()
                .find(|old| old.device == volume.device)
                .map(|old| checks::pcr7_keyslots(&old.tokens))
                .unwrap_or_default();
            checks::pcr7_keyslots(&volume.tokens)
                .iter()
                .all(|slot| old.contains(slot))
        })
        .map(|volume| volume.device.clone())
        .collect()
}
