# Installer

This document describes the InayOS installer: the core crate `crates/install-core` (package `inayos-install-core`) and the command line app `apps/installer` (package and binary `inayos-installer`).

The installer runs as root on the live USB system. It reads an answers file, partitions and formats one disk, installs a base Arch system into `/mnt`, creates the user, and runs `inayos install` and `inayos verify` in installer mode inside the new system. It never reboots: whoever started it reboots or powers off.

The core holds every step and has no interface. It reports through the `Sink` trait and changes the machine only through the `Host` trait. The command line app implements `Sink`, and any other interface fills an answers file and implements `Sink` the same way.

## Command line

`inayos-installer install --answers FILE [--log FILE] [--system-binary PATH]`

- `--answers` is the TOML answers file.
- `--log` defaults to `/var/log/inayos-installer.log`. The app appends to it, so earlier attempts stay in it. The install copies it to `/var/log/inayos-installer.log` in the new system before it unmounts.
- `--system-binary` defaults to `/usr/local/lib/inayos/inayos`, the `inayos-arch-desktop` binary on the USB image. The install copies it to `/usr/local/bin/inayos` in the new system.

Steps, commands and their output go to stdout and to the log. Queries, their output and file changes go to the log only. Passwords never appear in either: the Wi-Fi password is a hidden argument and the user password goes to `chpasswd` on its standard input.

The app exits 0 only when every step succeeded. An unreadable or invalid answers file and a failed install both exit 1, with the cause on stderr.

## Answers file

The core rejects unknown keys and checks every value before the install starts.

| Key | Meaning |
| --- | --- |
| `keyboard` | Console keymap. |
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

Each disk choice takes exactly the `[disk]` keys it uses: a missing key and an unused key are both errors. Partitions must be partitions of `device` and differ from each other. `secure_boot`, `encryption` and `beside-windows` fail with "not built yet".

## Stages

The install runs five stages and a cleanup. A failure names its stage, and the stage says what the failure left on the disk.

1. **Checks** change nothing on the disk. They check that the installer runs as root, the machine booted with UEFI, the system binary exists, `/mnt` is not a mount point, the device is a whole disk with nothing of it mounted, and the device does not hold the live medium mounted at `/run/archiso/bootmnt`. They check the locale and time zone, run `inayos list` from the system binary when `skip` is not empty, and read the partition table and filesystems the disk choice needs. For a choice that keeps home, they mount the kept partition read-only and read the owner of the user's home folder. Last, they connect the Wi-Fi and run `pacman -Sy`, which fails when the mirrors cannot be reached.
2. **Disk** partitions, formats and mounts, as described below.
3. **System** runs `pacstrap -K` with `base`, `linux`, `linux-firmware`, the AMD or Intel microcode that `/proc/cpuinfo` calls for, `btrfs-progs`, `e2fsprogs`, `dosfstools`, `sudo` and `networkmanager`. It writes `genfstab -U` to `/etc/fstab`, sets the locale, keymap, time zone and host name with `systemd-firstboot`, enables the locale and runs `locale-gen`, enables NetworkManager, and copies the live system's NetworkManager connections.
4. **User** creates the user in the `wheel` group with `useradd -m`, sets the password, writes `/etc/sudoers.d/10-inayos-wheel` for the wheel group, and writes `/etc/sudoers.d/99-inayos-installer`, which lets the user run sudo without a password. It adds the authorized keys that `~/.ssh/authorized_keys` does not already hold, keeping the keys a kept home already has. The file is owned by the user, with mode 600.
5. **Configuration** copies the system binary, writes the questions as a JSON object to `/var/tmp/inayos-answers.json` owned by the user, and runs `arch-chroot -S -u <user> /mnt env SYSTEMD_OFFLINE=1 TMPDIR=/var/tmp /usr/local/bin/inayos install --installer-mode --answers /var/tmp/inayos-answers.json`, then the same with `verify --installer-mode`. When `skip` is not empty, both commands name the profile modules that remain, in profile order. The file and every temporary file of the run live in `/var/tmp`, because `arch-chroot -S` mounts an empty `/tmp` that only root can write.

The cleanup runs after every install that got past the checks, also after a failure. It deletes the installer's sudo rule and the questions file, copies the log, and runs `umount -R /mnt`. Its own failures come with the failure they follow.

## Disk choices

The Btrfs system partition always gets the subvolumes `@`, `@snapshots`, `@var_cache`, `@var_log` and `@var_lib_docker`, mounted with `compress=zstd:1` at `/mnt`, `/mnt/.snapshots`, `/mnt/var/cache`, `/mnt/var/log` and `/mnt/var/lib/docker`. The EFI partition is mounted at `/mnt/efi` and home at `/mnt/home`. `/boot` stays inside `@`. Partitions start on 1 MiB boundaries.

- **`wipe-disk`** clears the partition table and creates a 2 GiB EFI partition, the system partition and a home partition over the rest of the disk, which must leave at least 1 GiB. It formats them as FAT32 labeled `EFI`, Btrfs labeled `inayos-system` and ext4 labeled `inayos-home`.
- **`reinstall-keep-home`** formats only the system partition. The EFI partition must hold FAT and home must hold ext4.
- **`shrink-ext4-keep-home`** turns the ext4 partition of an older system into home. The partition must be the last one on the disk and have a `home` folder at its top level. Its files must leave 10% of the shrunk filesystem free. The stage runs `e2fsck -f -y`, shrinks the filesystem with `resize2fs`, and recreates the GPT entry with the same first sector, type, unique GUID and name and an earlier last sector. It creates the system partition from the freed space to the end of the disk. Inside the old partition, it moves each folder of `home` to the top level and deletes every other top-level entry except `lost+found`. It deletes the old system's kernels, initramfs images and microcode images from the top of the EFI partition.

Both choices that keep home delete the state records `~/.local/state/inayos/state.json` and `~/.local/state/myconfig/state.json` in the kept home folder, because they describe the erased system and InayOS migrates the `myconfig` one. When the kept partition holds the user's home folder, the user is created with the user and group ids that own it, so its files stay the user's. A folder owned by user id 0 or group id 0 stops the install in the checks. The installer creates a group with the user's name and that group id, unless the new system already has a group with that id, such as `users`.

## Code layout

- `answers.rs` parses and checks the answers file.
- `sizes.rs` holds the size arithmetic: GiB to sectors, 1 MiB alignment, the wiped disk layout and the shrink target.
- `parse.rs` reads facts from the output of `lsblk`, `sgdisk`, `dumpe2fs` and from system files. `disk_names.rs` turns a disk and a number into a partition path and back.
- `host.rs` defines `Command`, `Host` and `RealHost`. `sink.rs` defines `Sink`.
- `install.rs` runs the stages in order and the cleanup. Each stage is a file in `install/`.

The tests in `install/tests.rs` run whole installs against `fake.rs`, a `Host` that returns prepared command outputs and keeps files in memory. They check the exact commands each disk choice runs and the files it leaves, without disks. They do not check that the real tools accept those commands; the VM scenarios of `cargo xtask vm` do.
