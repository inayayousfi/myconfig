# Installer

This document describes the InayOS installer: the core crate `crates/install-core` (package `inayos-install-core`) and the app `apps/installer` (package and binary `inayos-installer`), which has the installer's screens and its command line.

The installer runs as root on the live USB system. It reads an answers file, partitions and formats one disk, installs a base Arch system into `/mnt`, creates the user, and runs `inayos install` and `inayos verify` in installer mode inside the new system. The core never reboots: the done screen has a Reboot button, and whoever started the command line reboots or powers off.

The core holds every step and has no interface. It reports through the `Sink` trait and changes the machine only through the `Host` trait. The command line app implements `Sink`. Any other interface builds the same `Answers` and receives the lines through `ChannelSink`, which sends each one over a channel while `install` runs on another thread.

## Command line

`inayos-installer` with no subcommand opens the screens. `inayos-installer gui [--save-answers FILE] [--log FILE] [--system-binary PATH] [--guard-binary PATH]` opens them with options. With `--save-answers`, the summary writes the answers file, with mode 600, and the app quits instead of installing.

`inayos-installer install --answers FILE [--log FILE] [--system-binary PATH] [--guard-binary PATH]` installs without the screens.

- `--answers` is the TOML answers file.
- `--log` defaults to `/var/log/inayos-installer.log`. The app appends to it, so earlier attempts stay in it. The install copies it to `/var/log/inayos-installer.log` in the new system before it unmounts.
- `--system-binary` defaults to `/usr/local/lib/inayos/inayos`, the `inayos-arch-desktop` binary on the USB image. The install copies it to `/usr/local/bin/inayos` in the new system.
- `--guard-binary` defaults to `/usr/local/lib/inayos/inayos-guard`, the boot guard program on the USB image. The install copies it to `/usr/local/bin/inayos-guard` in the new system, where the `boot-guard` module and its pacman hook run it.

The `install` command sends steps, commands and their output to stdout and to the log. Queries, their output and file changes go to the log only. Passwords never appear in either: the Wi-Fi password is a hidden argument and the user password goes to `chpasswd` on its standard input. The recovery key of an encrypted install never appears in either: its command's output is hidden, and the step names only the file that holds a copy.

The `install` command exits 0 only when every step succeeded. An unreadable or invalid answers file and a failed install both exit 1, with the cause on stderr.

## Screens

The screens are an Iced application drawn by its CPU renderer, tiny-skia, so they show on machines and VMs without Vulkan or OpenGL. They run on Wayland only: winit loads the Wayland libraries at run time, so the app is built for `x86_64-unknown-linux-gnu`, not for a static musl target. The window's app id is `org.inayos.installer`. The look is InayOS Black and Pink: black background, pink accent (RGB 255, 78, 173), white text, large text and large click targets.

The app runs as root inside the live user's Sway session, which starts it with `sudo` keeping `WAYLAND_DISPLAY`, `XDG_RUNTIME_DIR` and `SWAYSOCK`.

The screens come in this order:

1. **Keyboard**: a layout list, a language list and a test field. A chosen layout applies to the session at once with `swaymsg input type:keyboard xkb_layout`, when `SWAYSOCK` is set. Until the user picks a language, the language follows the layout: the locale written like the layout and its country, or the locale a built-in table names for layouts whose code is not their language code. Otherwise the language stays as it is. The answers get the layout as `xkb_layout` and its console keymap from `catalog::console_keymap`.
2. **Wi-Fi**: the networks of `nmcli device wifi list`, a password field and a Connect button. The app skips the screen when `nmcli -t -f STATE general` prints `connected` at start. A network the screen connected to goes into the answers. Each scan also reads the connection state again. After a failed attempt, or once the live system is connected, Enter goes on; choosing a network or typing the password again lets Enter connect again.
3. **Disk**: the disks with their model and size, then the four disk choices of the chosen disk, the unavailable ones disabled with their reason. A choice that takes a size shows a slider within its limits and the presets 50, 100, 150 and 200 GiB, each moved into the limits. The size starts at 150 GiB, moved into the limits. The screen starts on the safest available choice, in this order: reinstall, shrink, beside Windows, erase the whole disk, so Enter never keeps erasing a disk that holds files that can stay.
4. **Security**: the Secure Boot switch, and the encryption switch on a machine with a TPM 2.0. Secure Boot starts off. It can be turned on, with a click or Space, only while the firmware is in Setup Mode; otherwise the screen says that Setup Mode must be turned on in the firmware settings, or why it could not be read. Encryption starts off and can be turned on whenever it shows. With encryption on, Next stays disabled unless the disk choice erases the whole disk. Up and Down outline the other switch, and Space turns the outlined one on or off.
5. **User**: full name, a username that follows the full name until the user types one, the password twice, the computer name, `inayos` by default, and a time zone list. The list starts with the zones of the keyboard's country and selects its main zone until the user picks one. Its search field selects the first match. The core's `check_username` and `check_hostname` give the reason a name is refused. When the live session types with a layout other than US, the screen warns that the installed desktop starts with the US layout, so the password must be typeable on both.
6. **Modules**: a checkbox per profile module from `modules::list`, except `secure-boot` and `encryption`, which the switches of the security screen choose. Locked modules stay ticked. Unticking a module unticks the modules that hard-need it, and ticking one ticks the modules it hard-needs.
7. **Summary**: the disk actions in plain words, naming what gets erased, then every other answer. Install stays disabled until the user types `erase`. The summary shows the reason when `Answers::check` refuses the answers.
8. **Progress**: the steps, commands and output of the install as they come, then **done** with a Reboot button, or **failed** with the error and the log path. After an encrypted install, the done screen shows the recovery key as text and as a QR code, asks the user to store it in a password manager, and names the copy in the home folder. The failed screen shows the same when the install failed after it created the key. The headless `install` command then prints where the copy is.

The screens answer the modules' questions without asking: yes to the SSH server, no to every login.

The Back and Next buttons sit at the top of the window, so the on-screen keyboard at the bottom of the screen never covers them. Every action has a button and a key:

- Enter does the screen's main action: Next, Connect on the Wi-Fi screen as described above, Install on the summary, Reboot when done.
- On the Wi-Fi screen, Space connects to the chosen network and F5 looks for networks again.
- Escape goes back.
- Tab and Shift+Tab move between the fields of a screen, lists included, and give the text focus to a text field.
- Up and Down move the selection in the focused list. On the size, they step through the presets.
- Space ticks or unticks the outlined module.

Enter, Up, Down and Space act only when a text field did not take the key.

The app writes the name of the screen it shows to `/run/inayos-installer/screen` each time it changes, for the VM tests: `keyboard`, `wifi`, `disk`, `security`, `user`, `modules`, `summary`, `progress`, `done` or `failed`, followed by a newline. It replaces the file with a rename, so a reader never sees it half written.

The install runs on its own thread. Every line goes to the log, as with the `install` command, and through `ChannelSink` to the progress screen, which collects the new lines ten times a second. The progress screen leaves out the details, which only the log keeps.

Every command the screens run goes through the core's `Host`. The app reads the disks, the catalogs, the modules, the TPM, the Setup Mode and the connection state once at start.

## Answers file

The core rejects unknown keys and checks every value before the install starts.

| Key | Meaning |
| --- | --- |
| `keyboard` | Console keymap. |
| `xkb_layout` | Optional. The XKB layout of the system, which systemd-localed reports. The Plasma session keeps the US layout that the `kde-plasma` module sets. |
| `language` | Locale. It must have a line in `/etc/locale.gen`. |
| `timezone` | Optional, `UTC` by default. It must exist under `/usr/share/zoneinfo`. |
| `hostname` | 1 to 63 letters, digits and `-`, not starting or ending with `-`. |
| `[network]` `wifi_ssid`, `wifi_password` | Optional. The live system connects with `nmcli` before anything else needs the network. A password needs a network name. |
| `[disk]` `choice` | `wipe-disk`, `reinstall-keep-home`, `shrink-ext4-keep-home` or `beside-windows`. |
| `[disk]` `device` | The whole disk. |
| `[disk]` `system_size_gib` | Btrfs system size for `wipe-disk` and `shrink-ext4-keep-home`, from 20 GiB. |
| `[disk]` `efi_partition` | The EFI partition that `reinstall-keep-home` and `shrink-ext4-keep-home` reuse without formatting. |
| `[disk]` `system_partition` | The partition that `reinstall-keep-home` formats as the system. |
| `[disk]` `home_partition` | The ext4 partition that `reinstall-keep-home` and `shrink-ext4-keep-home` keep as home. |
| `[security]` `secure_boot`, `encryption` | Optional, false by default. |
| `[user]` `full_name`, `username`, `password` | The username starts with a lowercase letter or `_`, holds lowercase letters, digits, `_` and `-`, has at most 32 characters and is not `root`. |
| `[user]` `authorized_keys` | Optional. Written to `~/.ssh/authorized_keys`. |
| `[modules]` `skip` | Optional. Profile modules not to install. |
| `[questions]` | Optional. Each module question's exact text and its yes or no answer. |

Each disk choice takes exactly the `[disk]` keys it uses: a missing key and an unused key are both errors. Partitions must be partitions of `device` and differ from each other. `beside-windows` fails with "not built yet". `secure_boot` needs the firmware in Setup Mode. Without `secure_boot`, the install skips the `secure-boot` module. `encryption` needs `wipe-disk` and a TPM 2.0. Without `encryption`, the install skips the `encryption` module.

## Stages

The install runs five stages, a sixth for an encrypted install, and a cleanup. A failure names its stage, and the stage says what the failure left on the disk.

1. **Checks** change nothing on the disk. They first check the answers with the same rules as the answers file, because an interface can build answers without reading a file. Encryption with a disk choice other than `wipe-disk` stops here. Then they check that the installer runs as root, the machine booted with UEFI, no system on the machine is hibernated, the machine has a TPM 2.0 when the answers turn encryption on, the firmware is in Setup Mode when the answers turn Secure Boot on, the system binary and the guard program exist, `/mnt` is not a mount point, the device is a whole disk with nothing of it mounted, and the device does not hold the live medium mounted at `/run/archiso/bootmnt`. They check the locale and time zone, run `inayos list` from the system binary when `skip` is not empty or Secure Boot is off, and read the partition table and filesystems the disk choice needs. They read the memory size from `/proc/meminfo`, because every home must have room for the hibernation swap file, as large as the memory rounded up to a whole GiB, as described in "Room for the swap file" below. A disk whose InayOS home holds a hibernated swap file stops the install, as described in "Hibernated systems" below. For a choice that keeps home, they mount the kept partition read-only and read the owner of the user's home folder. Last, they connect the Wi-Fi and run `pacman -Sy`, which fails when the mirrors cannot be reached.
2. **Disk** partitions, formats and mounts, as described below.
3. **System** runs `pacstrap -K` with `base`, `linux`, `linux-firmware`, the AMD or Intel microcode that `/proc/cpuinfo` calls for, `btrfs-progs`, `e2fsprogs`, `dosfstools`, `sudo` and `networkmanager`. It writes `genfstab -U` to `/etc/fstab`, sets the locale, keymap, time zone and host name with `systemd-firstboot`, and enables the locale and runs `locale-gen`. With `xkb_layout`, it writes `/etc/X11/xorg.conf.d/00-keyboard.conf` as `localectl set-x11-keymap` does, because no localed runs in the new system during the install. systemd-localed reads that file as the system layout. The Plasma session does not follow it, because the `kde-plasma` module sets each user's layout to US in `kxkbrc`. The stage then enables NetworkManager and copies the live system's NetworkManager connections.
4. **User** creates the user in the `wheel` group with `useradd -m`, sets the password, writes `/etc/sudoers.d/10-inayos-wheel` for the wheel group, and writes `/etc/sudoers.d/99-inayos-installer`, which lets the user run sudo without a password. It adds the authorized keys that `~/.ssh/authorized_keys` does not already hold, keeping the keys a kept home already has. The file is owned by the user, with mode 600.
5. **Configuration** copies the system binary and the guard program, writes the questions as a JSON object to `/var/tmp/inayos-answers.json` owned by the user, and runs `arch-chroot -S -u <user> /mnt env SYSTEMD_OFFLINE=1 TMPDIR=/var/tmp /usr/local/bin/inayos install --installer-mode --answers /var/tmp/inayos-answers.json`, then the same with `verify --installer-mode`. With Secure Boot on and the `secure-boot` module kept, it runs `arch-chroot /mnt sbctl enroll-keys --microsoft` between the two, with `--firmware-builtin` when the firmware has a built-in signature database (`dbDefault`), then fails when the firmware is still in Setup Mode. It then runs `arch-chroot /mnt /usr/local/bin/inayos-guard check-update`, so the guard signs the images it built while Secure Boot was off. Enrolling waits for the install to pass, because from then on the firmware refuses the unsigned installer stick. When modules are skipped, both commands name the profile modules that remain, in profile order. The skipped modules are those of `skip`, and `secure-boot` when Secure Boot is off and the profile has it. The file and every temporary file of the run live in `/var/tmp`, because `arch-chroot -S` mounts an empty `/tmp` that only root can write.
6. **Keys** runs only for an encrypted install, once the configuration has passed, and sets the keys of the volumes as described in "Encryption" below. A failure before the recovery key exists leaves volumes that open with nothing but the installer's temporary key, which is lost when the installer stops, so the install must run again. Once the key exists, every failure, including one in the cleanup, still hands it to the interface, which shows it on the failed screen.

The cleanup runs after every install that got past the checks, also after a failure. It deletes the installer's sudo rule and the questions file, copies the log, and runs `umount -R /mnt`. After an encrypted install it then closes both volumes and deletes the temporary key. Its own failures come with the failure they follow.

## Disk choices

The Btrfs system partition always gets the subvolumes `@`, `@snapshots`, `@var_cache`, `@var_log` and `@var_lib_docker`, mounted with `compress=zstd:1` at `/mnt`, `/mnt/.snapshots`, `/mnt/var/cache`, `/mnt/var/log` and `/mnt/var/lib/docker`. The EFI partition is mounted at `/mnt/efi` and home at `/mnt/home`. `/boot` stays inside `@`. Partitions start on 1 MiB boundaries.

- **`wipe-disk`** clears the partition table and creates a 2 GiB EFI partition, the system partition and a home partition over the rest of the disk, which must leave at least 20 GiB plus the swap file, because AUR packages build in the user's cache folder during the install. It formats them as FAT32 labeled `EFI`, Btrfs labeled `inayos-system` and ext4 labeled `inayos-home`.
- **`reinstall-keep-home`** formats only the system partition. The EFI partition must hold FAT and home must hold ext4.
- **`shrink-ext4-keep-home`** turns the ext4 partition of an older system into home. The partition must be the last one on the disk and have a `home` folder at its top level. Its files must leave 10% of the shrunk filesystem free. The stage runs `e2fsck -f -y`, shrinks the filesystem with `resize2fs`, and labels it `inayos-home` with `e2label`. It recreates the GPT entry with the same first sector, type and unique GUID, the name `inayos-home` and an earlier last sector. These labels let a later run offer `reinstall-keep-home` on this disk. It creates the system partition from the freed space to the end of the disk. Inside the old partition, it moves each folder of `home` to the top level and deletes every other top-level entry except `lost+found`. It deletes the old system's kernels, initramfs images and microcode images from the top of the EFI partition.

`reinstall-keep-home` also deletes the earlier system's swap file, `/home/swapfile`, from the kept home. The checks found that no system hibernated into it, and the `hibernation` module creates a new one at this machine's memory size.

Both choices that keep home delete the state records `~/.local/state/inayos/state.json` and `~/.local/state/myconfig/state.json` in the kept home folder, because they describe the erased system and InayOS migrates the `myconfig` one. When the kept partition holds the user's home folder, the user is created with the user and group ids that own it, so its files stay the user's. A folder owned by user id 0 or group id 0 stops the install in the checks. The installer creates a group with the user's name and that group id, unless the new system already has a group with that id, such as `users`.

## Encryption

Encryption makes `wipe-disk` format the system and home partitions as LUKS2 volumes. `docs/encryption.md` describes the boot of the installed system. The disk keeps the 2 GiB FAT32 EFI partition, unencrypted.

- The disk stage creates a random 64-byte temporary key in `/run/inayos-installer-keys`, a memory filesystem readable by root only. It formats the system partition with that key as LUKS2 labeled `inayos-system` and opens it as `/dev/mapper/system`, then does the same for home as `/dev/mapper/home`. The Btrfs and ext4 filesystems go inside the opened volumes, with the same labels and subvolumes as without encryption.
- The system stage writes `/etc/crypttab.initramfs`, which opens both volumes by their LUKS UUID with `tpm2-device=auto,discard`, and `/etc/kernel/cmdline`, which mounts the root from `/dev/mapper/system`. The `encryption` module builds the unified kernel image from them during the configuration.
- The keys stage asks `systemd-cryptenroll` for a recovery key on the system volume and adds the same key to home with `cryptsetup luksAddKey`, which reads it on its standard input. It enrolls a TPM 2.0 slot bound to no PCR on both volumes. It then checks that each one opens both volumes: `cryptsetup open --test-passphrase --key-file=-` with the recovery key on its standard input, and `cryptsetup open --test-passphrase --token-only --token-type systemd-tpm2`. Only then does it save the copy of the key and wipe the temporary key's slot on both. The guard of the new system binds the TPM slots to the Secure Boot state at its first boot.
- The recovery key is saved to `/home/<user>/inayos-recovery-key.txt`, mode 600, owned by the user, and `install` returns it to the interface. Its `Debug` form is hidden.

The disk is never opened with a typed password in normal use: the TPM opens it, and the recovery key is for when the TPM refuses.

## Hibernated systems

A hibernated InayOS loses the work it saved when its disk changes before it resumes. The install refuses such a disk with "This disk holds a hibernated InayOS: start it and shut it down properly first", before any disk change. It reads two signs, without mounting anything read-write:

- the `HibernateLocation-8cf2644b-4b0b-428f-9387-6d876050dc67` firmware variable, which systemd writes when it hibernates and clears when it resumes;
- the header of `/swapfile` at the top of each ext4 partition labeled `inayos-home`, and of the home partition that the answers keep. `debugfs` gives the file's first block, and `dd` reads that page, which holds `S1SUSPEND` after a hibernation.

The variable alone covers an encrypted home, whose swap file the installer cannot read. The USB stick masks `systemd-hibernate-clear.service`, which would otherwise delete the variable when the stick starts, as `test/usb/README.md` describes.

## Room for the swap file

Every home keeps room for the swap file of the `hibernation` module, which has the memory size rounded up to a whole GiB. `wipe-disk` needs 20 GiB of home plus the swap file. `shrink-ext4-keep-home` counts the swap file with the files that must leave 10% of the shrunk filesystem free. `reinstall-keep-home` needs free space for the swap file on the kept home, from `dumpe2fs -h`, counting the earlier swap file that the disk stage deletes, whose size `debugfs -R "stat /swapfile"` gives.

## What an interface reads

The core gives an interface the facts it needs to fill the answers. These functions only read the machine.

- `inventory::disks` lists the whole disks from one `lsblk` call, which reads each device's path, type, filesystem, label and mount points. It leaves out zram devices, and the USB stick: a disk with a partition mounted at `/run/archiso/bootmnt` or a filesystem label starting with `INAYOS_`. For each disk it gives one offer per disk choice. An offer holds either the partitions the answers name or the plain-words reason the choice does not fit. A choice that takes a system size also gets the smallest and largest size the checks accept on that disk.
- `wipe-disk` fits a disk that holds the 2 GiB EFI partition, the smallest system and 20 GiB of home plus the swap file.
- `reinstall-keep-home` fits a disk with partitions labeled `inayos-system` and `inayos-home`, the home one holding ext4, and an EFI partition, found by its GPT type and FAT filesystem. It does not fit when the kept home has no room for the swap file; a home that `dumpe2fs` cannot read stays offered, and the checks stop on it.
- `shrink-ext4-keep-home` takes the EFI partition and the ext4 partition that starts last. It does not fit when a partition starts after it or when it is labeled `inayos-home`, which holds no `home` folder. Its largest size comes from `dumpe2fs -h`, so the files and the swap file keep 10% of the shrunk home free.
- `beside-windows` never fits yet.
- A disk with a mounted partition fits no choice.
- While the firmware variable or a swap file says that an InayOS is hibernated, every offer of the disk is unavailable with the hibernation message.
- `reinstall-keep-home` does not fit a disk whose `inayos-home` partition is a LUKS volume, because it keeps only an unencrypted home.
- `inventory::has_tpm2` reads `/sys/class/tpm/tpm0/tpm_version_major`. The security screen shows the encryption switch only when it is true.
- `inventory::secure_boot_setup_mode` reads the firmware variable `SetupMode-8be4df61-93ca-11d2-aa0d-00e098032b8c` from efivarfs, whose last byte is 1 in Setup Mode. It fails when the variable cannot be read.
- `catalog` reads the keyboard layouts from `/usr/share/X11/xkb/rules/base.lst`, the UTF-8 locales from `/usr/share/i18n/SUPPORTED` and the time zones from `/usr/share/zoneinfo/zone1970.tab`, under a root folder the caller gives. A two-letter layout name is the country code of its layout. `console_keymap` picks the console keymap for a layout from `/usr/share/systemd/kbd-model-map`, as `localectl` does, and falls back to `us`. `time_zones_for` puts the zones of a country first.
- `modules::list` runs `<system binary> list --needs`, a hidden flag of every InayOS binary that prints each profile module as `name: need1 need2` with its hard needs. The modules in `LOCKED_MODULES` and every module they hard-need, through any depth, are locked: the answers never skip them. `modules::dependents` gives the modules that hard-need one module, so leaving it out leaves them out too. `modules::requirements` gives the modules it hard-needs, so including it includes them too.

An interface that builds `Answers` itself checks them with `Answers::check` before it starts the install, and `install` checks them again before the disk changes. `check_username` and `check_hostname` give the reason a typed name is refused, so a screen can show it next to the field.

`Answers::to_toml` and `Answers::write` turn answers back into an answers file that `Answers::read` reads as the same answers. `write` gives the file mode 600, because it holds the passwords.

## Code layout

- `answers.rs` parses, checks and writes the answers file.
- `inventory.rs`, `catalog.rs` and `modules.rs` read what an interface offers.
- `sizes.rs` holds the size arithmetic: GiB to sectors, 1 MiB alignment, the wiped disk layout and the shrink target.
- `parse.rs` reads facts from the output of `lsblk`, `sgdisk`, `dumpe2fs`, `debugfs` and `systemd-cryptenroll` and from system files.
- `hibernation.rs` finds a hibernated system. `disk_names.rs` turns a disk and a number into a partition path and back.
- `host.rs` defines `Command`, `Host` and `RealHost`. `sink.rs` defines `Sink` and `ChannelSink`.
- `install.rs` runs the stages in order and the cleanup. Each stage is a file in `install/`.

The tests in `install/tests.rs` run whole installs against `fake.rs`, a `Host` that returns prepared command outputs and keeps files in memory. They check the exact commands each disk choice runs and the files it leaves, without disks. The tests in `inventory/tests.rs` read the offers from lsblk output of a blank disk, an InayOS disk, a laptop with one ext4 partition, the same laptop after a shrink install, and a Windows disk. They do not check that the real tools accept those commands; the VM scenarios of `cargo xtask vm` do.

## App code layout

- `installer.rs` holds the command line, `console.rs` the `install` command's sink, and `log_file.rs` the log both share.
- `app.rs` is the state machine: the current screen, the keys, the answers the screens hold, and the changes to the live system the screens ask for, which it runs through `Host` on threads.
- `screens/` holds one file per screen. A screen owns its state, turns messages and keys into new state and draws itself. It never acts on the machine: it returns an `Effect` for the app to carry out. `screens/mod.rs` holds the screen order and the Tab focus, and `screens/list.rs` the large-row list the screens share.
- `qr.rs` draws the recovery key's QR code on an Iced canvas.
- `live.rs` runs `swaymsg`, `nmcli` and `systemctl reboot` and writes the screen file. `running.rs` runs the install thread. `questions.rs` holds the questions' answers, and `theme.rs` the look.

The app's tests run the screens and the state machine without a window. They check the screen order, the answers each disk choice produces, the username suggestion, the module cascade, the summary text, and a walk through every screen with Enter that saves an answers file `Answers::read` reads back as the same answers. They do not draw the window or press real keys; the `gui-install` VM scenario does.
