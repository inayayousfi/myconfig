//! The scenarios of the InayOS installer. Most boot the USB stick image with a new blank
//! target disk and a seed disc, drives the live system over SSH as root, installs, powers off,
//! boots the installed disk without the stick, and checks the result over SSH as the new user
//! and through QMP screenshots. The Basics scenarios run `inayos-installer install` with an
//! answers file; `gui-install` goes through the installer's screens with QMP key presses.
//! The boot chain scenarios check Secure Boot and the boot guard: `secure-boot` installs with
//! Secure Boot on, and `broken-kernel` starts from the installed disk that the last passed
//! `blank-disk` run kept, instead of installing again.
//!
//! A scenario runs on its own layer. A passed scenario copies its screenshots to its
//! `last-pass` folder and deletes the layer. A failed one keeps the layer, with its logs,
//! screenshots, answers files and last screen, as the failed layer of that scenario until its
//! next run.

mod answers_file;
mod boot_chain;
mod boot_checks;
mod checks;
mod gui;
mod installed;

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, Instant};

use answers_file::DiskChoice;

use crate::binary::{self, Binary, BinaryError};
use crate::disk::{self, DiskError};
use crate::host::{self, HostError};
use crate::lock::{LockError, VmLock};
use crate::machine::{Machine, Media, SSH_PORT, Sockets};
use crate::os::Boot;
use crate::os::{GUEST_PASSWORD, GUEST_USER, Os};
use crate::paths::{Layer, Paths, PathsError};
use crate::qmp::QmpError;
use crate::seed::{self, SeedError, SeedFile};
use crate::ssh::{Guest, SshError, SshKey};
use crate::vm::{Vm, VmError};

/// The guests of the scenarios are Arch guests: 8 GiB of memory and virtio devices. Their
/// firmware is UEFI without Secure Boot unless the scenario chooses another.
const OS: Os = Os::Arch;
const TARGET_DISK_SIZE: &str = "80G";
const DEVICE: &str = "/dev/vda";
/// The Btrfs system size. The default of 150 GiB does not fit the 80 GB target disk.
const SYSTEM_SIZE_GIB: u32 = 40;
/// The system size of the shrink scenario, which leaves the old system room on its partition.
const SHRINK_SIZE_GIB: u32 = 30;
/// cloud-init on the live system looks for a NoCloud seed on a disc with this label.
const SEED_LABEL: &str = "CIDATA";
const LIVE_USER_DATA: &str = include_str!("../../scenario/live-user-data");
const LAPTOP_DISK_SCRIPT: &str = include_str!("../../scenario/laptop-disk.sh");
/// The installer reads its answers here on the live system.
const LIVE_ANSWERS: &str = "/root/inayos-answers.toml";
const LIVE_LOG: &str = "/var/log/inayos-installer.log";
const LIVE_CHECKSUMS: &str = "/root/home-checksums.txt";
const LIVE_EFI_UUID: &str = "/root/efi-uuid.txt";
/// The uid and gid of the user on the disk like the laptop's. A new system never picks them by
/// itself, so the shrink checks see whether the installer gave the new user the kept owner.
const OLD_USER_ID: u32 = 1234;
const MARKER: &str = "inayos-marker";
const MARKER_TEXT: &str = "kept across the reinstall";

const SSH_TIMEOUT: Duration = Duration::from_secs(15 * 60);
const POWER_OFF_TIMEOUT: Duration = Duration::from_secs(3 * 60);
const LOGIN_SCREEN_TIMEOUT: Duration = Duration::from_secs(5 * 60);
/// The login screen draws its first frame a moment after the on-screen keyboard starts.
const LOGIN_SCREEN_SETTLE: Duration = Duration::from_secs(15);
const SESSION_TIMEOUT: Duration = Duration::from_secs(2 * 60);
const DESKTOP_TIMEOUT: Duration = Duration::from_secs(3 * 60);
const POLL: Duration = Duration::from_secs(5);
/// How often the password is typed at the login screen when no session starts. The first
/// keys can arrive before the password field has the focus.
const LOGIN_ATTEMPTS: u32 = 2;

/// A scenario of the InayOS installer.
#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum)]
pub enum Name {
    /// Wipes the blank disk with the default layout and checks the finished desktop.
    BlankDisk,
    /// Installs on the blank disk, writes a file in the home folder, reinstalls keeping home,
    /// and checks that the file survived.
    ReinstallKeepHome,
    /// Builds a disk like the laptop's, shrinks its ext4 partition and keeps it as home, and
    /// checks that the home files survived.
    ShrinkExt4,
    /// Installs on the blank disk through the installer's screens, driven by key presses, and
    /// checks the finished desktop.
    GuiInstall,
    /// Installs on the blank disk with Secure Boot on, from firmware in Setup Mode, and checks
    /// the signatures, the guard and the finished desktop.
    SecureBoot,
    /// Starts from the installed disk of the last passed blank-disk run, breaks the boot files
    /// in a pacman transaction, and checks that the guard makes the pre-update snapshot the
    /// default and that the machine boots it.
    BrokenKernel,
}

impl Name {
    pub fn name(self) -> &'static str {
        match self {
            Name::BlankDisk => "blank-disk",
            Name::ReinstallKeepHome => "reinstall-keep-home",
            Name::ShrinkExt4 => "shrink-ext4",
            Name::GuiInstall => "gui-install",
            Name::SecureBoot => "secure-boot",
            Name::BrokenKernel => "broken-kernel",
        }
    }

    /// The firmware of the scenario's guest. Secure Boot needs the Secure Boot firmware build;
    /// its TPM is unused until encryption needs one.
    fn boot(self) -> Boot {
        match self {
            Name::SecureBoot => Boot::SecureUefiWithTpm,
            Name::BrokenKernel => installed::BOOT,
            Name::BlankDisk | Name::ReinstallKeepHome | Name::ShrinkExt4 | Name::GuiInstall => {
                Boot::Uefi
            }
        }
    }

    /// Whether the scenario starts from the kept installed disk instead of a blank one.
    fn starts_installed(self) -> bool {
        self == Name::BrokenKernel
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ScenarioError {
    #[error(transparent)]
    Paths(#[from] PathsError),
    #[error(transparent)]
    Host(#[from] HostError),
    #[error(transparent)]
    Lock(#[from] LockError),
    #[error(transparent)]
    Ssh(#[from] SshError),
    #[error(transparent)]
    Disk(#[from] DiskError),
    #[error("no USB stick image in {0}; build it with: cargo xtask vm usb build")]
    NoIso(PathBuf),
    #[error(transparent)]
    NoInstalledDisk(#[from] installed::NoInstalledDisk),
    #[error(transparent)]
    Binary(#[from] BinaryError),
    #[error(
        "the {name} scenario failed: {error}\nthe disk, logs, screenshots and answers files are kept in {kept}"
    )]
    Failed {
        name: &'static str,
        error: Box<StepError>,
        kept: PathBuf,
    },
    #[error(
        "the {name} scenario failed: {error}\nkeeping the failed disk failed too, so it is still in {work}: {keep}"
    )]
    NotKept {
        name: &'static str,
        error: Box<StepError>,
        work: PathBuf,
        keep: io::Error,
    },
    #[error(
        "the {name} scenario passed, but its screenshots could not be copied to {last_pass}: {error}\nthe layer is still in {work}"
    )]
    PassNotKept {
        name: &'static str,
        last_pass: PathBuf,
        work: PathBuf,
        error: io::Error,
    },
    #[error(
        "the blank-disk scenario passed, but its installed disk could not be kept in {installed}: {error}"
    )]
    InstalledNotKept {
        installed: PathBuf,
        error: io::Error,
    },
}

/// A failure of one step of a running scenario.
#[derive(Debug, thiserror::Error)]
pub enum StepError {
    #[error(transparent)]
    Vm(#[from] VmError),
    #[error(transparent)]
    Ssh(#[from] SshError),
    #[error(transparent)]
    Qmp(#[from] QmpError),
    #[error(transparent)]
    Seed(#[from] SeedError),
    #[error("could not write {0}: {1}")]
    Io(PathBuf, io::Error),
    #[error("the guest did not power off within {0:?}")]
    NoPowerOff(Duration),
    #[error("{what} within {timeout:?}{}", last.as_ref().map(|last| format!("; last: {last}")).unwrap_or_default())]
    NotReached {
        what: String,
        timeout: Duration,
        last: Option<String>,
    },
    #[error("{0}")]
    Check(String),
    #[error("could not copy {package} into the live system: {error}")]
    Copy {
        package: &'static str,
        error: SshError,
    },
    #[error(transparent)]
    Binary(#[from] BinaryError),
}

/// Runs the scenario `name` on a new layer. With `watch`, a viewer window shows the guest
/// display.
pub fn run(name: Name, watch: bool, repository: &Path) -> Result<(), ScenarioError> {
    let paths = Paths::from_env()?;
    host::check_boot(OS, name.boot())?;
    let _lock = VmLock::acquire(&paths.lock())?;
    let iso = paths.usb_iso();
    let installed = if name.starts_installed() {
        Some(installed::find(&paths.scenario_installed())?)
    } else if !iso.is_file() {
        return Err(ScenarioError::NoIso(iso));
    } else {
        None
    };
    let key = SshKey::ensure(&paths.ssh_key())?;
    let binaries = binary::build(OS, repository)?;
    let work = paths.scenario_work(name.name());
    let failed = paths.scenario_failed(name.name());
    disk::discard(&failed)?;
    match &installed {
        Some(installed) => {
            println!(
                "vm: creating a throwaway layer over the installed disk in {}, {}",
                installed.dir().display(),
                installed::describe(installed)
            );
            disk::layer_over(installed, &work, name.boot())?;
        }
        None => {
            println!("vm: creating a blank {TARGET_DISK_SIZE} target disk");
            disk::blank(&work, TARGET_DISK_SIZE, name.boot())?;
        }
    }
    let installed_copy = (name == Name::BlankDisk).then(|| paths.scenario_installed_copy());

    let mut scenario = Scenario {
        watch,
        public_key: key.public()?,
        key,
        iso,
        layer: work.clone(),
        runtime: paths.runtime(),
        binaries,
        vm: None,
        boots: 0,
        boot: name.boot(),
        installed_copy: installed_copy.clone(),
    };
    let result = scenario.steps(name);
    let vm = scenario.vm.take();
    match result {
        Ok(()) => {
            if let Some(vm) = vm {
                vm.shut_down(POWER_OFF_TIMEOUT);
            }
            let last_pass = paths.scenario_last_pass(name.name());
            let kept = keep_screenshots(work.dir(), &last_pass).map_err(|error| {
                ScenarioError::PassNotKept {
                    name: name.name(),
                    last_pass: last_pass.clone(),
                    work: work.dir().to_owned(),
                    error,
                }
            });
            if kept.is_ok() {
                disk::discard(&work)?;
                println!(
                    "vm: the {} scenario passed; its screenshots are in {}",
                    name.name(),
                    last_pass.display()
                );
            }
            if let Some(copy) = &installed_copy {
                let installed = paths.scenario_installed();
                if let Err(error) = installed::promote(copy, &installed) {
                    if let Err(remove_error) = installed::remove(copy) {
                        eprintln!(
                            "vm: could not remove the installed disk copy {}: {remove_error}",
                            copy.dir().display()
                        );
                    }
                    return Err(ScenarioError::InstalledNotKept {
                        installed: installed.dir().to_owned(),
                        error,
                    });
                }
                println!(
                    "vm: the installed disk is kept in {} for the boot chain scenarios",
                    installed.dir().display()
                );
            }
            kept
        }
        Err(error) => {
            if let Some(copy) = &installed_copy
                && let Err(remove_error) = installed::remove(copy)
            {
                eprintln!(
                    "vm: could not remove the installed disk copy {}: {remove_error}",
                    copy.dir().display()
                );
            }
            if let Some(vm) = vm {
                let screen = work.dir().join("screen.png");
                if let Err(screen_error) = vm.qmp().and_then(|mut qmp| qmp.screendump(&screen)) {
                    eprintln!("vm: could not save the last screen: {screen_error}");
                }
                vm.shut_down(POWER_OFF_TIMEOUT);
            }
            match fs::rename(work.dir(), failed.dir()) {
                Ok(()) => Err(ScenarioError::Failed {
                    name: name.name(),
                    error: Box::new(error),
                    kept: failed.dir().to_owned(),
                }),
                Err(keep) => Err(ScenarioError::NotKept {
                    name: name.name(),
                    error: Box::new(error),
                    work: work.dir().to_owned(),
                    keep,
                }),
            }
        }
    }
}

/// One scenario in progress, with the guest that runs now.
struct Scenario {
    watch: bool,
    key: SshKey,
    public_key: String,
    iso: PathBuf,
    layer: Layer,
    runtime: PathBuf,
    /// The installers built from the working tree, which replace the copies on the USB stick
    /// image, so a scenario tests the working tree without a new image.
    binaries: Vec<Binary>,
    vm: Option<Vm>,
    /// The number of boots so far, which names the serial log of each boot.
    boots: u32,
    /// The firmware of the guest, which the install also takes as the Secure Boot answer.
    boot: Boot,
    /// Where a `blank-disk` run copies the installed disk before its first boot.
    installed_copy: Option<Layer>,
}

impl Scenario {
    fn steps(&mut self, name: Name) -> Result<(), StepError> {
        match name {
            Name::BlankDisk => {
                self.install_on_blank_disk()?;
                self.copy_installed_disk()?;
                let user = self.boot_installed()?;
                self.check_desktop(&user)
            }
            Name::SecureBoot => {
                self.install_on_blank_disk()?;
                let user = self.boot_installed()?;
                self.check_secure_boot(&user)?;
                self.check_guard_after_install(&user)?;
                self.check_desktop(&user)
            }
            Name::BrokenKernel => {
                let user = self.boot_installed()?;
                self.put_fresh_guard(&user)?;
                let snapshot = self.break_boot_files(&user)?;
                self.check_guard_caught(&user, snapshot)?;
                self.power_off(&user, &user_power_off())?;
                let user = self.boot_installed_showing_refind()?;
                self.check_booted_snapshot(&user, snapshot)
            }
            Name::ReinstallKeepHome => {
                self.install_on_blank_disk()?;
                let user = self.boot_installed()?;
                println!("vm: writing ~/{MARKER} in the installed system");
                user.run(&format!("printf '%s\\n' '{MARKER_TEXT}' > ~/{MARKER}"))?;
                let owner_before = user.run("stat -c '%u %g' ~")?;
                self.power_off(&user, &user_power_off())?;

                let live = self.boot_live()?;
                self.install(
                    &live,
                    &DiskChoice::ReinstallKeepHome {
                        device: DEVICE,
                        efi_partition: "/dev/vda1",
                        system_partition: "/dev/vda2",
                        home_partition: "/dev/vda3",
                    },
                    "reinstall",
                )?;
                self.power_off(&live, "systemctl poweroff")?;
                let user = self.boot_installed()?;
                let kept = user.run(&format!("cat ~/{MARKER} && stat -c %U ~/{MARKER}"))?;
                if kept != format!("{MARKER_TEXT}\n{GUEST_USER}\n") {
                    return Err(StepError::Check(format!(
                        "~/{MARKER} after the reinstall is not the file written before it, owned by {GUEST_USER}: {kept:?}"
                    )));
                }
                let owner_after = user.run("stat -c '%u %g' ~ && id -u && id -g")?;
                let ids = owner_before.trim().replace(' ', "\n");
                if owner_after != format!("{}\n{ids}\n", owner_before.trim()) {
                    return Err(StepError::Check(format!(
                        "the home folder was owned by uid and gid {}, but after the reinstall the home owner, uid and gid are {owner_after:?}",
                        owner_before.trim()
                    )));
                }
                println!(
                    "vm: ~/{MARKER} survived the reinstall, and the user kept its uid and gid"
                );
                self.check_desktop(&user)
            }
            Name::GuiInstall => {
                self.install_through_gui()?;
                let user = self.boot_installed()?;
                self.check_desktop(&user)
            }
            Name::ShrinkExt4 => {
                let live = self.boot_live()?;
                println!("vm: building a disk like the laptop's");
                let script = self.write("laptop-disk.sh", LAPTOP_DISK_SCRIPT)?;
                live.copy_to(&script, "/root/laptop-disk.sh")?;
                live.run_logged(
                    &format!(
                        "bash /root/laptop-disk.sh {DEVICE} {GUEST_USER} {OLD_USER_ID} {LIVE_CHECKSUMS} {LIVE_EFI_UUID}"
                    ),
                    &self.log("laptop-disk"),
                )?;
                let checksums = self.layer.dir().join("home-checksums.txt");
                live.copy_from(LIVE_CHECKSUMS, &checksums)?;
                let efi_uuid = live.run(&format!("cat {LIVE_EFI_UUID}"))?.trim().to_owned();
                self.install(
                    &live,
                    &DiskChoice::ShrinkExt4KeepHome {
                        device: DEVICE,
                        system_size_gib: SHRINK_SIZE_GIB,
                        efi_partition: "/dev/vda1",
                        home_partition: "/dev/vda2",
                    },
                    "shrink",
                )?;
                self.power_off(&live, "systemctl poweroff")?;
                let user = self.boot_installed()?;
                self.check_shrunk_disk(&user, &checksums, &efi_uuid)?;
                self.check_desktop(&user)
            }
        }
    }

    /// Boots the stick and installs on the blank target disk with the default layout.
    fn install_on_blank_disk(&mut self) -> Result<(), StepError> {
        let live = self.boot_live()?;
        self.install(
            &live,
            &DiskChoice::WipeDisk {
                device: DEVICE,
                system_size_gib: SYSTEM_SIZE_GIB,
            },
            "wipe-disk",
        )?;
        self.power_off(&live, "systemctl poweroff")
    }

    /// Copies the installed disk and its UEFI variables, before the installed system first
    /// boots, to the place that replaces the kept installed disk when the scenario passes.
    fn copy_installed_disk(&mut self) -> Result<(), StepError> {
        let Some(copy) = self.installed_copy.clone() else {
            return Ok(());
        };
        println!("vm: copying the installed disk to {}", copy.dir().display());
        installed::copy(&self.layer, &copy).map_err(|error| StepError::Io(copy.disk(), error))
    }

    /// Boots the USB stick image with the seed disc and waits until root accepts SSH.
    fn boot_live(&mut self) -> Result<Guest, StepError> {
        let seed_iso = self.layer.dir().join("seed.iso");
        if !seed_iso.is_file() {
            let key = serde_json::Value::from(self.public_key.as_str()).to_string();
            let files = [
                SeedFile {
                    name: "user-data",
                    content: LIVE_USER_DATA.replace("@KEY@", &key),
                    executable: false,
                },
                SeedFile {
                    name: "meta-data",
                    content: "instance-id: inayos-live\nlocal-hostname: archiso\n".to_owned(),
                    executable: false,
                },
            ];
            seed::build(
                SEED_LABEL,
                &files,
                &self.layer.dir().join("seed"),
                &seed_iso,
            )?;
        }
        println!("vm: booting the USB stick");
        self.start(Media {
            boot: Some(self.iso.clone()),
            attached: vec![seed_iso],
        })?;
        let guest = Guest::new(self.key.clone(), "root", SSH_PORT);
        self.wait_for_ssh(&guest)?;
        Ok(guest)
    }

    /// Boots the target disk without the stick and waits until the new user accepts SSH.
    fn boot_installed(&mut self) -> Result<Guest, StepError> {
        println!("vm: booting the installed system");
        self.start(Media::default())?;
        let guest = Guest::new(self.key.clone(), GUEST_USER, SSH_PORT);
        self.wait_for_ssh(&guest)?;
        Ok(guest)
    }

    fn start(&mut self, media: Media) -> Result<(), StepError> {
        self.boots += 1;
        let machine = Machine {
            os: OS,
            boot: self.boot,
            layer: self.layer.clone(),
            sockets: Sockets::in_dir(&self.runtime),
            media,
            displays: 1,
            serial: Some(
                self.layer
                    .dir()
                    .join(format!("serial-boot-{}.log", self.boots)),
            ),
        };
        self.vm = Some(Vm::start(&machine, &self.runtime, self.watch)?);
        Ok(())
    }

    /// Whether the install turns Secure Boot on, which the Secure Boot firmware is there for.
    fn secure_boot(&self) -> bool {
        self.boot == Boot::SecureUefiWithTpm
    }

    fn vm(&mut self) -> &mut Vm {
        self.vm.as_mut().expect("a guest runs during this step")
    }

    fn wait_for_ssh(&mut self, guest: &Guest) -> Result<(), StepError> {
        println!("vm: waiting for SSH");
        guest
            .wait_until_ready(self.vm(), SSH_TIMEOUT)
            .map_err(StepError::from)
    }

    /// Writes the answers file for `disk` into the layer, copies it into the live system, and
    /// runs the installer with it. `label` names the answers file and the logs.
    fn install(&mut self, live: &Guest, disk: &DiskChoice, label: &str) -> Result<(), StepError> {
        let answers = self.write(
            &format!("answers-{label}.toml"),
            &answers_file::answers_file(disk, &self.public_key, self.secure_boot()),
        )?;
        live.copy_to(&answers, LIVE_ANSWERS)?;
        self.copy_binaries(live)?;
        println!("vm: running the installer with {}", answers.display());
        let result = live.run_logged(
            &installer_command(),
            &self.log(&format!("installer-{label}")),
        );
        // The installer's own log holds more than its output, and helps most after a failure.
        if let Err(error) = live.copy_from(LIVE_LOG, &self.log(&format!("installer-{label}-file")))
        {
            eprintln!("vm: could not copy {LIVE_LOG}: {error}");
        }
        result?;
        Ok(())
    }

    /// Checks that the installers built from the working tree run with the live system's C
    /// library, then puts them in place of the copies on the stick.
    ///
    /// The Sway session of the stick runs the installer's screens from its place, and Linux
    /// refuses a write to a running program. So each binary goes to a file next to its place
    /// first, and a rename replaces the old one, which leaves a running copy untouched.
    fn copy_binaries(&self, live: &Guest) -> Result<(), StepError> {
        binary::check_glibc(live, &self.binaries)?;
        for binary in &self.binaries {
            let place = live_place(binary.executable);
            let staged = format!("{place}.new");
            let copy_error = |error| StepError::Copy {
                package: binary.package,
                error,
            };
            live.copy_to(&binary.local, &staged).map_err(copy_error)?;
            live.run(&format!("chmod 755 {staged} && mv -f {staged} {place}"))
                .map_err(copy_error)?;
        }
        Ok(())
    }

    /// Runs `command`, which powers the guest off, and waits until QEMU stops.
    fn power_off(&mut self, guest: &Guest, command: &str) -> Result<(), StepError> {
        println!("vm: powering off");
        // The connection may drop while the guest goes down, so only the power-off counts.
        let _ = guest.run(command);
        let deadline = Instant::now() + POWER_OFF_TIMEOUT;
        loop {
            match self.vm().check_running() {
                Ok(()) => {}
                Err(VmError::Exited { .. }) => break,
                Err(error) => return Err(error.into()),
            }
            if Instant::now() >= deadline {
                return Err(StepError::NoPowerOff(POWER_OFF_TIMEOUT));
            }
            thread::sleep(POLL);
        }
        self.vm = None;
        Ok(())
    }

    /// The checks of a finished desktop: the login screen with the on-screen keyboard, then a
    /// login through QMP, the user services, plasmashell and the panels of the default layout.
    fn check_desktop(&mut self, user: &Guest) -> Result<(), StepError> {
        println!("vm: waiting for the login screen with Axidev OSK");
        self.wait_until(
            "plasmalogin.service is active with an Axidev OSK process",
            LOGIN_SCREEN_TIMEOUT,
            || {
                run_succeeds(
                    user,
                    // The bracket keeps the pattern from matching the shell that runs this command.
                    "systemctl is-active --quiet plasmalogin.service && pgrep -f '[a]xidev-osk'",
                )
            },
        )?;
        thread::sleep(LOGIN_SCREEN_SETTLE);
        self.screenshot("login-screen.png")?;

        let mut attempt = 0;
        loop {
            attempt += 1;
            let mut qmp = self.vm().qmp()?;
            if attempt > 1 {
                // A session that started just after the wait gave up needs no second password.
                if run_succeeds(user, &format!("pgrep -u {GUEST_USER} -x plasmashell"))?.is_ok() {
                    break;
                }
                // Keys of the first attempt that did reach the field are cleared first.
                println!("vm: clearing the password field");
                qmp.send_keys(&["ctrl", "a"])?;
                qmp.send_keys(&["backspace"])?;
            }
            println!("vm: typing the password at the login screen");
            qmp.type_text(&format!("{GUEST_PASSWORD}\n"))?;
            drop(qmp);
            match self.wait_until("plasmashell runs for the user", SESSION_TIMEOUT, || {
                run_succeeds(user, &format!("pgrep -u {GUEST_USER} -x plasmashell"))
            }) {
                Ok(()) => break,
                Err(error) if attempt >= LOGIN_ATTEMPTS => return Err(error),
                Err(_) => {}
            }
        }

        println!("vm: checking the Kanata and Handy user services");
        self.wait_until(
            "the user services inayos-kanata-tray, inayos-kanata and inayos-handy are active",
            DESKTOP_TIMEOUT,
            || {
                run_succeeds(
                    user,
                    "systemctl --user is-active --quiet inayos-kanata-tray.service inayos-kanata.service inayos-handy.service",
                )
            },
        )?;

        println!("vm: checking the panels of the default layout");
        let appletsrc_copy = self
            .layer
            .dir()
            .join("plasma-org.kde.plasma.desktop-appletsrc");
        self.wait_until(
            "the panels of the default layout exist",
            DESKTOP_TIMEOUT,
            || {
                let text = match user.run("cat ~/.config/plasma-org.kde.plasma.desktop-appletsrc") {
                    Ok(text) => text,
                    Err(SshError::Failed { stderr, .. }) => return Ok(Err(stderr)),
                    Err(error) => return Err(error.into()),
                };
                fs::write(&appletsrc_copy, &text)
                    .map_err(|error| StepError::Io(appletsrc_copy.clone(), error))?;
                Ok(match checks::missing_layout(&checks::panels(&text)) {
                    None => Ok(()),
                    Some(missing) => Err(missing),
                })
            },
        )?;
        self.screenshot("desktop.png")?;
        println!("vm: the desktop is finished");
        Ok(())
    }

    /// The checks of the shrink scenario: the home files survived with their owner, whose uid
    /// and gid the new user has; the partitions are the EFI partition, then the old ext4
    /// partition as home, then the Btrfs system; the top level of the home partition holds only
    /// `lost+found` and the home folder; and the EFI partition is the old one, not formatted,
    /// without the old system's boot files.
    fn check_shrunk_disk(
        &mut self,
        user: &Guest,
        checksums: &Path,
        efi_uuid: &str,
    ) -> Result<(), StepError> {
        println!("vm: checking the home files that the old partition held");
        user.copy_to(checksums, "/tmp/home-checksums.txt")?;
        user.run("cd /home && sha256sum --quiet --check /tmp/home-checksums.txt")
            .map_err(|error| StepError::Check(format!("the home files changed: {error}")))?;
        let foreign = user.run(&format!(
            "find ~/Documents/inayos-test ~/Pictures/inayos-test ! -user {GUEST_USER}"
        ))?;
        if !foreign.trim().is_empty() {
            return Err(StepError::Check(format!(
                "these home files are not owned by {GUEST_USER}: {foreign}"
            )));
        }
        let ids = user.run("id -u && id -g && stat -c '%u %g' ~/Documents/inayos-test/note.txt")?;
        let expected = format!("{OLD_USER_ID}\n{OLD_USER_ID}\n{OLD_USER_ID} {OLD_USER_ID}\n");
        if ids != expected {
            return Err(StepError::Check(format!(
                "{GUEST_USER} and the kept files should have uid and gid {OLD_USER_ID}, but uid, gid and the files' owner are {ids:?}"
            )));
        }
        let top_level = user.run("ls -A1 /home")?;
        if top_level != format!("lost+found\n{GUEST_USER}\n") {
            return Err(StepError::Check(format!(
                "the top level of the home partition should hold only lost+found and {GUEST_USER}, but holds:\n{top_level}"
            )));
        }
        let mounted_efi_uuid = user.run("findmnt -no UUID /efi")?;
        if mounted_efi_uuid.trim() != efi_uuid {
            return Err(StepError::Check(format!(
                "/efi has filesystem UUID {:?}, not the old EFI partition's {efi_uuid:?}: it was formatted",
                mounted_efi_uuid.trim()
            )));
        }

        let listing = user.run(&format!(
            "for partition in /sys/class/block/{name}[0-9]*; do name=${{partition##*/}}; \
             printf '%s %s %s\\n' \"$(cat \"$partition/start\")\" \"$name\" \"$(lsblk -dnro FSTYPE /dev/$name)\"; done",
            name = DEVICE.trim_start_matches("/dev/")
        ))?;
        let order = checks::partition_order(&listing);
        if order != ["vfat", "ext4", "btrfs"] {
            return Err(StepError::Check(format!(
                "the partitions are {order:?} on the disk, not vfat, ext4, btrfs:\n{listing}"
            )));
        }
        let mounts = user.run("findmnt -no FSTYPE / && findmnt -no FSTYPE /home")?;
        if mounts != "btrfs\next4\n" {
            return Err(StepError::Check(format!(
                "/ and /home are {mounts:?}, not Btrfs and ext4"
            )));
        }
        let old_boot_files = user.run(&format!(
            "printf '%s\\n' '{GUEST_PASSWORD}' | sudo -S -p '' find /efi -maxdepth 1 \
             \\( -name 'vmlinuz-*' -o -name 'initramfs-*.img' -o -name '*-ucode.img' \\)"
        ))?;
        if !old_boot_files.trim().is_empty() {
            return Err(StepError::Check(format!(
                "the old system's boot files are still on the EFI partition: {old_boot_files}"
            )));
        }
        println!(
            "vm: the home files survived with their owner, the old system is gone, and the partitions are in order"
        );
        Ok(())
    }

    /// Calls `check` until it returns `Ok(Ok(()))`, the running guest stops, or `timeout`
    /// passes. `Ok(Err(text))` means not yet, with `text` saying what was seen.
    fn wait_until(
        &mut self,
        what: &str,
        timeout: Duration,
        mut check: impl FnMut() -> Result<Result<(), String>, StepError>,
    ) -> Result<(), StepError> {
        let deadline = Instant::now() + timeout;
        loop {
            self.vm().check_running()?;
            let last = match check()? {
                Ok(()) => return Ok(()),
                Err(last) => last,
            };
            if Instant::now() >= deadline {
                return Err(StepError::NotReached {
                    what: format!("not reached: {what}"),
                    timeout,
                    last: (!last.is_empty()).then_some(last),
                });
            }
            thread::sleep(POLL);
        }
    }

    fn screenshot(&mut self, name: &str) -> Result<(), StepError> {
        let path = self.layer.dir().join(name);
        self.vm().qmp()?.screendump(&path)?;
        println!("vm: saved {}", path.display());
        Ok(())
    }

    fn write(&self, name: &str, text: &str) -> Result<PathBuf, StepError> {
        let path = self.layer.dir().join(name);
        fs::write(&path, text).map_err(|error| StepError::Io(path.clone(), error))?;
        Ok(path)
    }

    fn log(&self, name: &str) -> PathBuf {
        self.layer.dir().join("logs").join(format!("{name}.log"))
    }
}

/// Replaces the folder `last_pass` with a copy of the PNG screenshots at the top of `layer`.
fn keep_screenshots(layer: &Path, last_pass: &Path) -> io::Result<()> {
    match fs::remove_dir_all(last_pass) {
        Err(error) if error.kind() != io::ErrorKind::NotFound => return Err(error),
        _ => {}
    }
    fs::create_dir_all(last_pass)?;
    for entry in fs::read_dir(layer)? {
        let path = entry?.path();
        if path.is_file() && path.extension().is_some_and(|extension| extension == "png") {
            let name = path.file_name().expect("a listed file has a name");
            fs::copy(&path, last_pass.join(name))?;
        }
    }
    Ok(())
}

/// Runs `remote` as a check that has not passed yet when it fails in the guest.
fn run_succeeds(guest: &Guest, remote: &str) -> Result<Result<(), String>, StepError> {
    match guest.run(remote) {
        Ok(_) => Ok(Ok(())),
        Err(SshError::Failed { stderr, .. }) => Ok(Err(stderr)),
        Err(error) => Err(error.into()),
    }
}

/// Where the USB stick image keeps the program `executable`, as test/vm/usb/build-iso.sh places
/// it.
fn live_place(executable: &str) -> &'static str {
    match executable {
        "inayos-installer" => "/usr/local/bin/inayos-installer",
        "inayos-guard" => "/usr/local/lib/inayos/inayos-guard",
        _ => "/usr/local/lib/inayos/inayos",
    }
}

/// The installer command on the live system, which places the InayOS desktop installer of
/// the stick in the new system.
fn installer_command() -> String {
    format!("inayos-installer install --answers {LIVE_ANSWERS}")
}

/// Powers the installed system off as the new user, who needs the password for sudo.
fn user_power_off() -> String {
    sudo("systemctl poweroff")
}

/// Runs `command` as root through sudo, as the new user, who needs the password for it.
fn sudo(command: &str) -> String {
    format!("printf '%s\\n' '{GUEST_PASSWORD}' | sudo -S -p '' {command}")
}
