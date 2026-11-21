//! The steps of the Windows scenarios: Windows itself over SSH, the install beside Windows and
//! its checks, the boots through rEFInd that start Windows from its firmware entry with a key or
//! a mouse click, and the installs that a BitLocker or Fast Startup Windows refuses.
//!
//! Windows and InayOS accept the same SSH key for the same user name, so after a boot through
//! rEFInd the scenario asks which system answered: only the Windows command prompt has `ver`.

use std::fs;
use std::path::PathBuf;
use std::thread;
use std::time::Duration;

use super::answers_file::DiskChoice;
use super::encryption::{HEADS_SCRIPT, Refusal};
use super::windows_checks::{self as checks, Partition, System, WINDOWS_BOOT_MANAGER};
use super::{Scenario, StepError, sudo, user_power_off};
use crate::machine::{Media, SSH_PORT};
use crate::os::{GUEST_USER, Os};
use crate::ppm;
use crate::ssh::{Guest, SshError};

/// The Windows guest's SATA disk, as Linux names it.
const WINDOWS_DEVICE: &str = "/dev/sda";
const BOOT_CURRENT_SCRIPT: &str = include_str!("../../scenario/windows/boot-current.ps1");
pub(super) const BITLOCKER_SCRIPT: &str = include_str!("../../scenario/windows/bitlocker-on.ps1");
pub(super) const FAST_STARTUP_SCRIPT: &str =
    include_str!("../../scenario/windows/fast-startup-on.ps1");
/// Prints one line per partition of the disk given as argument, in the form that
/// `windows_checks::partitions` reads: start sector, size in bytes, name, filesystem,
/// partition GUID and filesystem UUID, with `-` for an empty field.
const PARTITIONS_SCRIPT: &str = r#"set -e
for partition in /sys/class/block/"${1#/dev/}"[0-9]*; do
    name=${partition##*/}
    filesystem=$(lsblk -dnro FSTYPE "/dev/$name")
    partition_uuid=$(lsblk -dnro PARTUUID "/dev/$name")
    filesystem_uuid=$(lsblk -dnro UUID "/dev/$name")
    printf '%s %s %s %s %s %s\n' "$(cat "$partition/start")" "$(( $(cat "$partition/size") * 512 ))" \
        "$name" "${filesystem:--}" "${partition_uuid:--}" "${filesystem_uuid:--}"
done
"#;
/// Windows shuts down more slowly than Linux, and a hybrid shutdown writes its memory first.
const WINDOWS_POWER_OFF_TIMEOUT: Duration = Duration::from_secs(10 * 60);
/// rEFInd starts its 20-second wait a few seconds after the firmware. An input every second
/// for this many seconds stops that wait in its first seconds, and an input before rEFInd
/// runs does nothing.
const HOLD_INPUTS: u32 = 15;
const HOLD_INTERVAL: Duration = Duration::from_secs(1);
/// rEFInd's first row has at most nine shortcut digits.
const MAX_DIGIT: usize = 9;
/// How many times the scenario starts Windows through `BootNext` after the shrink, since
/// each restart after Windows' disk check ends in InayOS.
const WINDOWS_CHECK_ATTEMPTS: u32 = 3;
/// The time rEFInd takes to draw the selection after the pointer moved.
const POINTER_SETTLE: Duration = Duration::from_millis(700);

/// What the scenario does in rEFInd's menu.
#[derive(Clone, Copy, Debug)]
enum Pick {
    /// Nothing: rEFInd boots its default entry after its wait.
    Default,
    /// Presses the shortcut digit of the first row's entry, which starts it at once.
    Digit(usize),
    /// Clicks the tag of the first row at this index from the left.
    Click(usize),
}

impl Scenario {
    /// Boots the Windows disk and waits until Windows accepts SSH.
    pub(super) fn boot_windows(&mut self) -> Result<Guest, StepError> {
        println!("vm: booting Windows");
        self.start(Media::default())?;
        let guest = Guest::new(self.key.clone(), GUEST_USER, SSH_PORT);
        self.wait_for_any_system(&guest)?;
        match self.booted_system(&guest) {
            System::Windows => Ok(guest),
            System::InayOs => Err(StepError::Check(
                "the Windows disk booted a system that is not Windows".to_owned(),
            )),
        }
    }

    /// Copies the PowerShell script `text` into the user's home folder in Windows as
    /// `<name>.ps1` and runs it, logging its output, which it returns.
    pub(super) fn run_windows_script(
        &mut self,
        windows: &Guest,
        name: &str,
        text: &str,
    ) -> Result<String, StepError> {
        let file = format!("{name}.ps1");
        let local = self.write(&file, text)?;
        windows.copy_to(&local, &file)?;
        println!("vm: running {file} in Windows");
        let log = self.log(&format!("windows-{name}"));
        windows.run_logged(
            &format!("powershell -NoProfile -ExecutionPolicy Bypass -File {file}"),
            &log,
        )?;
        fs::read_to_string(&log).map_err(|error| StepError::Io(log, error))
    }

    /// Runs `command`, which shuts Windows down, and waits until QEMU stops.
    pub(super) fn shut_down_windows(
        &mut self,
        windows: &Guest,
        command: &str,
    ) -> Result<(), StepError> {
        println!("vm: shutting Windows down with {command}");
        self.power_off_within(windows, command, WINDOWS_POWER_OFF_TIMEOUT)
    }

    /// Boots the USB stick on the Windows disk and installs beside Windows. Checks the
    /// partitions afterwards on the live system and returns the filesystem UUID of the EFI
    /// system partition, which the installed system must mount at `/efi`.
    pub(super) fn install_beside_windows(&mut self) -> Result<String, StepError> {
        let live = self.boot_live()?;
        self.wait_for_stick_installer(&live)?;
        let before = self.partitions(&live, "before")?;
        let efi_uuid = before
            .iter()
            .find(|partition| partition.filesystem == "vfat")
            .map(|partition| partition.filesystem_uuid.clone())
            .ok_or_else(|| {
                StepError::Check("the Windows disk has no EFI system partition".to_owned())
            })?;
        self.install(
            &live,
            &DiskChoice::BesideWindows {
                device: WINDOWS_DEVICE,
            },
            "beside-windows",
        )?;
        let after = self.partitions(&live, "after")?;
        let shrunk = checks::beside_windows_problem(&before, &after).map_err(StepError::Check)?;
        println!("vm: {shrunk}, and InayOS is beside it");
        self.power_off(&live, "systemctl poweroff")?;
        Ok(efi_uuid)
    }

    /// Checks that the installed system mounts Windows' EFI system partition at `/efi`.
    pub(super) fn check_efi_reused(
        &mut self,
        user: &Guest,
        efi_uuid: &str,
    ) -> Result<(), StepError> {
        let mounted = user.run("findmnt -no UUID /efi")?;
        if mounted.trim() != efi_uuid {
            return Err(StepError::Check(format!(
                "/efi has filesystem UUID {:?}, not Windows' EFI system partition's {efi_uuid:?}",
                mounted.trim()
            )));
        }
        Ok(())
    }

    /// Boots the disk through rEFInd without input, and checks that its default entry starts
    /// InayOS. `label` names the boot.
    pub(super) fn boot_inayos_by_default(&mut self, label: &str) -> Result<Guest, StepError> {
        println!("vm: booting rEFInd's default entry ({label})");
        let (guest, system) = self.boot_through_refind(Pick::Default, label)?;
        if system != System::InayOs {
            return Err(StepError::Check(format!(
                "rEFInd's default entry started Windows instead of InayOS ({label}), so Windows \
                 put its boot manager before rEFInd in the firmware's boot order"
            )));
        }
        Ok(guest)
    }

    /// Starts Windows once through its firmware entry, before the boots through rEFInd.
    /// Shrinking NTFS marks the volume for a check, so Windows checks it on this boot and may
    /// restart afterwards. That restart follows the firmware's boot order into rEFInd, whose
    /// default entry starts InayOS; the scenario then asks the firmware for Windows again.
    /// Ends with a clean Windows shutdown, after which Windows boots without restarting.
    pub(super) fn finish_windows_disk_check(&mut self, user: Guest) -> Result<(), StepError> {
        let mut inayos = user;
        for attempt in 1..=WINDOWS_CHECK_ATTEMPTS {
            let listing = inayos.run(&sudo("efibootmgr"))?;
            self.write(&format!("efibootmgr-disk-check-{attempt}.txt"), &listing)?;
            let number = checks::boot_entry_number(&listing, WINDOWS_BOOT_MANAGER)
                .ok_or_else(|| {
                    StepError::Check(format!(
                        "the firmware has no {WINDOWS_BOOT_MANAGER:?} boot entry:\n{listing}"
                    ))
                })?
                .to_owned();
            println!("vm: starting Windows through BootNext {number} so it checks its disk");
            inayos.run(&sudo(&format!("efibootmgr --quiet --bootnext {number}")))?;
            self.power_off(&inayos, &user_power_off())?;
            self.start(Media::default())?;
            let guest = Guest::new(self.key.clone(), GUEST_USER, SSH_PORT);
            self.wait_for_any_system(&guest)?;
            match self.booted_system(&guest) {
                System::Windows => {
                    return self.shut_down_windows(&guest, "shutdown /s /t 0");
                }
                System::InayOs => {
                    println!("vm: Windows restarted into rEFInd after its disk check");
                    inayos = guest;
                }
            }
        }
        Err(StepError::Check(format!(
            "Windows did not reach its desktop in {WINDOWS_CHECK_ATTEMPTS} starts through \
             BootNext after the shrink: each start ended in InayOS"
        )))
    }

    /// Starts each entry of rEFInd's first row with its shortcut digit, from the first, until
    /// one starts Windows. Checks that Windows came from its own firmware entry, shuts it
    /// down, and returns the index of that entry in the first row.
    pub(super) fn find_windows_entry_with_keys(&mut self) -> Result<usize, StepError> {
        for digit in 1..=MAX_DIGIT {
            println!("vm: starting rEFInd's entry {digit} with its shortcut key");
            let label = format!("key-{digit}");
            let (guest, system) = match self.boot_through_refind(Pick::Digit(digit), &label) {
                Ok(booted) => booted,
                Err(StepError::Ssh(SshError::Timeout(timeout))) => {
                    return Err(StepError::Check(format!(
                        "no system answered within {timeout:?} after the key {digit}, so rEFInd's \
                         first row has fewer entries and none of them started Windows; see \
                         refind-{label}.png"
                    )));
                }
                Err(error) => return Err(error),
            };
            match system {
                System::InayOs => {
                    println!("vm: entry {digit} starts InayOS");
                    self.power_off(&guest, &user_power_off())?;
                }
                System::Windows => {
                    println!("vm: entry {digit} starts Windows");
                    self.check_started_by_firmware(&guest, &label)?;
                    self.shut_down_windows(&guest, "shutdown /s /t 0")?;
                    return Ok(digit - 1);
                }
            }
        }
        Err(StepError::Check(format!(
            "none of the first {MAX_DIGIT} entries of rEFInd started Windows"
        )))
    }

    /// Boots through rEFInd, clicks the tag at `index` of its first row with the USB tablet,
    /// checks that Windows starts from its own firmware entry, and shuts it down.
    pub(super) fn start_windows_with_click(&mut self, index: usize) -> Result<(), StepError> {
        println!("vm: clicking rEFInd's entry {} with the pointer", index + 1);
        let (guest, system) = self.boot_through_refind(Pick::Click(index), "click")?;
        if system != System::Windows {
            return Err(StepError::Check(
                "the click on rEFInd's Windows entry started InayOS instead of Windows".to_owned(),
            ));
        }
        self.check_started_by_firmware(&guest, "click")?;
        self.shut_down_windows(&guest, "shutdown /s /t 0")
    }

    /// Checks in Windows that the firmware boot entry of this boot, `BootCurrent`, is Windows
    /// Boot Manager, so the firmware started Windows through `BootNext` and rEFInd was not in
    /// its boot path.
    fn check_started_by_firmware(&mut self, windows: &Guest, label: &str) -> Result<(), StepError> {
        let output = self.run_windows_script(windows, "boot-current", BOOT_CURRENT_SCRIPT)?;
        self.write(&format!("boot-current-{label}.txt"), &output)?;
        match checks::boot_current(&output) {
            Some((entry, WINDOWS_BOOT_MANAGER)) => {
                println!("vm: the firmware started Windows from {entry} {WINDOWS_BOOT_MANAGER}");
                Ok(())
            }
            Some((entry, other)) => Err(StepError::Check(format!(
                "Windows started from the firmware entry {entry} {other:?}, not from {WINDOWS_BOOT_MANAGER:?}"
            ))),
            None => Err(StepError::Check(format!(
                "boot-current.ps1 printed no boot entry:\n{output}"
            ))),
        }
    }

    /// On the Windows disk after Windows was changed so it cannot be shrunk: boots the stick,
    /// runs the installer with beside-windows answers, and checks that its checks stage refused
    /// with `message` and left the disk unchanged. Then shows the installer's disk screen,
    /// which gives the reason, and checks once more that the disk is unchanged.
    pub(super) fn check_beside_windows_refused(
        &mut self,
        message: &str,
        label: &str,
    ) -> Result<(), StepError> {
        let live = self.boot_live()?;
        self.wait_for_stick_installer(&live)?;
        let script = self.write("disk-heads.sh", HEADS_SCRIPT)?;
        live.copy_to(&script, "/root/disk-heads.sh")?;
        let heads = format!("bash /root/disk-heads.sh {WINDOWS_DEVICE}");
        let before = live.run(&heads)?;
        self.write("disk-heads-before.txt", &before)?;
        let disk = DiskChoice::BesideWindows {
            device: WINDOWS_DEVICE,
        };
        let refusal = Refusal {
            disk: &disk,
            message,
            heads: &heads,
            before: &before,
        };
        println!("vm: installing beside Windows, which the installer must refuse");
        self.check_install_refused(&live, &format!("beside-windows-{label}"), &refusal)?;
        self.show_disk_screen(&live, &format!("{label}-disk-screen.png"))?;
        self.check_disk_unchanged(&live, &format!("{label}-disk-screen"), &refusal)?;
        println!("vm: the installer refused to install beside Windows and left the disk unchanged");
        self.power_off(&live, "systemctl poweroff")
    }

    /// Lists the partitions of the Windows disk on a running Linux system and keeps the
    /// listing in the layer as `partitions-<label>.txt`.
    fn partitions(&mut self, linux: &Guest, label: &str) -> Result<Vec<Partition>, StepError> {
        let script = self.write("partitions.sh", PARTITIONS_SCRIPT)?;
        linux.copy_to(&script, "/tmp/inayos-partitions.sh")?;
        let listing = linux.run(&format!("bash /tmp/inayos-partitions.sh {WINDOWS_DEVICE}"))?;
        self.write(&format!("partitions-{label}.txt"), &listing)?;
        checks::partitions(&listing).map_err(StepError::Check)
    }

    /// Boots the disk, does `pick` in rEFInd's menu, waits until a system accepts SSH, and
    /// returns it with which system it is.
    fn boot_through_refind(
        &mut self,
        pick: Pick,
        label: &str,
    ) -> Result<(Guest, System), StepError> {
        self.start(Media::default())?;
        match pick {
            Pick::Default => {}
            Pick::Digit(digit) => {
                self.hold_refind(|qmp, _| qmp.send_keys(&["up"]))?;
                self.screenshot(&format!("refind-{label}.png"))?;
                self.vm().qmp()?.send_keys(&[&digit.to_string()])?;
            }
            Pick::Click(index) => self.click_tag(index, label)?,
        }
        let guest = Guest::new(self.key.clone(), GUEST_USER, SSH_PORT);
        self.wait_for_any_system(&guest)?;
        let system = self.booted_system(&guest);
        Ok((guest, system))
    }

    /// Gives rEFInd an input every second for a while, which stops its wait as soon as it
    /// shows its menu. Up does nothing on the first row, and the pointer stays in the bottom
    /// left corner, away from every tag, moving by one position each time so that each move
    /// is an input.
    fn hold_refind(
        &mut self,
        mut input: impl FnMut(&mut crate::qmp::Qmp, u32) -> Result<(), crate::qmp::QmpError>,
    ) -> Result<(), StepError> {
        for count in 0..HOLD_INPUTS {
            thread::sleep(HOLD_INTERVAL);
            self.vm().check_running()?;
            input(&mut self.vm().qmp()?, count)?;
        }
        Ok(())
    }

    /// Finds the tags of rEFInd's first row with the pointer, then clicks the one at `index`.
    ///
    /// rEFInd draws its selection behind the tag under the pointer and none when the pointer is
    /// elsewhere. The pointer visits every place where a tag can have its center; a place
    /// where the screen gains a selection-sized change against the screen with the pointer in
    /// the corner holds a tag.
    fn click_tag(&mut self, index: usize, label: &str) -> Result<(), StepError> {
        self.hold_refind(|qmp, count| qmp.move_pointer(count as usize % 2, 999, 1000, 1000))?;
        let baseline = self.screen_ppm("pointer-baseline.ppm")?;
        let (width, height) = ppm::size(&baseline).map_err(|error| {
            StepError::Check(format!("could not read the screenshot of rEFInd: {error}"))
        })?;
        self.screenshot(&format!("refind-{label}.png"))?;
        let mut tags = Vec::new();
        for column in checks::tag_columns(width) {
            self.vm()
                .qmp()?
                .move_pointer(column, height / 2, width, height)?;
            thread::sleep(POINTER_SETTLE);
            let probe = self.screen_ppm("pointer-probe.ppm")?;
            let changed = ppm::changed_area(&baseline, &probe, checks::tag_rows(height))
                .map_err(|error| StepError::Check(format!("could not compare screens: {error}")))?;
            if checks::is_selection(changed) {
                tags.push(column);
            }
        }
        println!("vm: the pointer found rEFInd tags centered at the columns {tags:?}");
        let Some(&column) = tags.get(index) else {
            return Err(StepError::Check(format!(
                "the pointer selected {} tags in rEFInd's first row, so it cannot reach entry {}: \
                 either the pointer does not work in rEFInd on this firmware or rEFInd draws no \
                 selection under it; see refind-{label}.png",
                tags.len(),
                index + 1
            )));
        };
        self.vm()
            .qmp()?
            .move_pointer(column, height / 2, width, height)?;
        thread::sleep(POINTER_SETTLE);
        self.screenshot(&format!("refind-{label}-pointer-on-windows.png"))?;
        self.vm().qmp()?.click()?;
        Ok(())
    }

    /// Saves the screen as a PPM file named `name` in the layer and returns its bytes.
    fn screen_ppm(&mut self, name: &str) -> Result<Vec<u8>, StepError> {
        let path: PathBuf = self.layer.dir().join(name);
        self.vm().qmp()?.screendump_ppm(&path)?;
        let bytes = fs::read(&path).map_err(|error| StepError::Io(path.clone(), error))?;
        fs::remove_file(&path).map_err(|error| StepError::Io(path, error))?;
        Ok(bytes)
    }

    /// Waits until Windows or InayOS accepts SSH, as long as Windows may take.
    fn wait_for_any_system(&mut self, guest: &Guest) -> Result<(), StepError> {
        println!("vm: waiting for SSH");
        let timeout = Duration::from_secs(Os::Windows.ssh_timeout_secs());
        guest
            .wait_until_ready(self.vm(), timeout)
            .map_err(StepError::from)
    }

    /// Which system answers SSH as `guest`.
    fn booted_system(&self, guest: &Guest) -> System {
        checks::system_from_ver(guest.run("ver").ok().as_deref())
    }
}
