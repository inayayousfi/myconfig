# Boot guard

This document describes the boot guard of the InayOS desktop profile: the `boot-guard` module, `crates/modules/src/boot_guard.rs`, with its files in `dotfiles/assets/boot-guard`, and the guard program `inayos-guard`.

The guard checks every boot file that rEFInd starts after each update of those files, and keeps a working way back: a rEFInd entry that starts the system as it was before the last update.

## The module

The module needs `refind` and `snapshots`, and `secure-boot` when the run includes it. It comes after `refind`, `secure-boot` and `bootvine` in the profile, so its first check sees the signed files and the rebuilt initramfs.

### Install

Install first looks for rEFInd's configuration on the EFI system partition, as the refind module does. Without it, a running system skips the module with a note, and an installer-mode run fails.

It then:

1. checks that `/usr/local/bin/inayos-guard` exists and is executable. The InayOS installer copies it there next to `/usr/local/bin/inayos`. On a running system without it, the module copies the `inayos-guard` that sits next to the running `inayos` binary. Without either, an installer-mode run fails, and a running system skips the module with a note before writing anything;
3. installs `libnotify`, which provides `notify-send`;
4. writes its files under `/`, each a recorded write;
5. enables `inayos-guard-boot.service` and the user unit `inayos-guard-notify.path`, and starts the path unit when a graphical session runs;
6. runs `inayos-guard check-update` once, so `inayos-guard.conf` and the record exist before the first boot. It records `inayos-guard.conf` and the folder `/var/lib/inayos-guard` first, so remove deletes them.

The refind module's managed block in `refind.conf` includes `inayos-guard.conf` last, after the theme, so the guard's `default_selection` wins. The refind module writes a placeholder `inayos-guard.conf` holding one comment when none exists, so the include always finds a file.

### Files

- `/etc/pacman.d/hooks/zzz-inayos-guard.hook` runs `inayos-guard check-update` after every transaction that installs, upgrades or removes a kernel, a file under `/boot`, rEFInd's package files, the mkinitcpio configuration, `sbctl` or `snapper`. It also has every trigger of `90-mkinitcpio-install.hook`, which rebuilds the initramfs: its paths, such as firmware, DKMS sources, systemd and udev, and an install or upgrade of the `mkinitcpio` package. Its name sorts after `90-mkinitcpio-install.hook` and sbctl's `zz-sbctl.hook`, so the guard checks the rebuilt initramfs and the new signatures. The guard exits with 0 when a check fails, so the hook never aborts the transaction.
- `/etc/systemd/system/inayos-guard-boot.service` runs `inayos-guard check-boot` once at each boot.
- `/etc/systemd/user/inayos-guard-notify.path` starts `inayos-guard-notify.service` while `/var/lib/inayos-guard/failed` exists. It is wanted by `graphical-session.target`, so the notification has a desktop to show on.
- `/etc/systemd/user/inayos-guard-notify.service` shows a critical desktop notification with the output of `inayos-guard status`. It then stays active until the failed file is gone, so the path unit does not start it again in a loop, and the next failure notifies again.

### Verify

On a running system without the guard program, `verify` skips the check with a note. Otherwise it fails when `libnotify` is missing, when a file differs from the repository copy, when the boot service or the user path unit is not enabled, when `inayos-guard.conf` is missing, or when the record says that the last check failed. The failure then lists the guard's failures.

### Remove

`remove` disables the units, deletes the files, `inayos-guard.conf` and `/var/lib/inayos-guard`, and uninstalls `libnotify` when install added it. The guard program stays, because the installer placed it. When the refind module wrote the placeholder first, remove puts the placeholder back, so the include line in rEFInd's managed block still finds a file.

## The guard program

`inayos-guard` is a small program apart from `inayos`, so a kernel update never depends on the large program working. Its logic is the library `crates/guard`, and `apps/guard` is the command line around it. It is built for `x86_64-unknown-linux-musl`, has no interface and no network access, and depends only on `serde`, `serde_json` and `thiserror`. Every program it runs and every file it reads or writes goes through its `System` trait, so its tests run against a fake machine.

### Commands

| Command | Run by | Exit status |
| --- | --- | --- |
| `inayos-guard check-update` | root, from the pacman hook after a transaction that touches the boot files | 0 when it recorded a result, failed checks included; 3 when it could not write rEFInd's file or the record |
| `inayos-guard check-boot` | root, from `inayos-guard-boot.service` at each boot | 0 when the check passed, 1 when it failed, 3 when it could not record the result |
| `inayos-guard status` | anyone | 0 when the last recorded results passed, 1 when one failed or no check has run |

Wrong arguments exit with 2. Failures go to standard error, one per line and prefixed with `inayos-guard:`, so pacman shows them.

### check-update

When `/proc/cmdline` holds `systemd.volatile=overlay`, the system started from a snapshot, and `/boot` and `/.snapshots` show that snapshot instead of the system subvolume. The command then checks nothing, leaves `inayos-guard.conf`, the record and the failed file as they are, and exits with 0.

Otherwise it reads what rEFInd starts:

- rEFInd's folder, `EFI/refind` on the first of `/efi`, `/boot/efi` and `/boot` that holds `EFI/refind/refind.conf`;
- `refind_x64.efi`, the drivers in `drivers_x64` and the EFI tools in `EFI/tools`;
- each `/boot/vmlinuz-<name>` with its `/boot/initramfs-<name>.img` and, when present, `initramfs-<name>-fallback.img`;
- the kernel options of each entry in `/boot/refind_linux.conf`.

It then checks:

- **Signatures.** The check runs when Secure Boot is on, or when `sbctl status --json` reports sbctl's keys installed and the firmware out of Setup Mode, which is the state right after the keys are enrolled and before the next boot. Secure Boot is on when the last byte of `/sys/firmware/efi/efivars/SecureBoot-8be4df61-93ca-11d2-aa0d-00e098032b8c` is 1. Each EFI binary and each kernel goes through their own `sbctl verify --json --disable-landlock <file>`. Landlock is off because it hides files outside the EFI system partition that sbctl's database does not list. A file passes when sbctl reports `"is_signed": 1`.
- **Boot images.** The kernel version comes from the kernel image's x86 boot header. `lsinitcpio` lists each boot image, and the image passes when it holds a kernel module file (`.ko`, compressed or not) under `usr/lib/modules/<version>/`.
- **Root disk.** The `root=` of every entry names a disk that exists: its link under `/dev/disk/by-uuid`, `by-label`, `by-partuuid` or `by-partlabel`, or a `blkid` lookup.
- **rEFInd.** `refind_x64.efi` exists.

It writes `EFI/refind/inayos-guard.conf` in rEFInd's folder in one step, through a temporary file and a rename. The file holds:

- A "Previous system" entry for the newest snapper "pre" snapshot that can start. The guard reads snapper's `info.xml` files in `/.snapshots`, not snapper's output. A snapshot can start when its `/boot` holds a kernel and its boot image, the boot image holds that kernel's modules, and, when signatures are checked, its kernel is signed. These checks only choose the snapshot: a snapshot that fails them is left out and named on standard error, and it never counts as a failure of the normal entry. The entry starts the snapshot's kernel from the Btrfs volume labelled `inayos-system`, with the options `root=<the normal entry's root> rootflags=subvol=@snapshots/<n>/snapshot systemd.volatile=overlay rw` followed by the other options of the normal entry, such as `quiet` and the console colors. A microcode image in the snapshot's `/boot` goes first, as an `initrd=` option, because rEFInd passes one `initrd` per entry after the options.
- `default_selection`. It names the normal entry, as `"vmlinuz-linux from"`, which matches rEFInd's title for that kernel and not for `vmlinuz-linux-lts`. It names `"Previous system"` when a check failed and the entry exists. Without a snapshot that can start, the normal entry stays the default, so a failure only ever makes an entry that passed its own checks the default.

After writing the file, it marks the entry's snapshot important with `snapper --no-dbus -c root modify --userdata important=yes <n>`. Snapper's number cleanup then counts it against `NUMBER_LIMIT_IMPORTANT` instead of `NUMBER_LIMIT`, so ordinary pacman transactions, which do not run the guard, do not delete it as soon. A failure to mark it is printed and is not a failed check.

Then it records the result.

### check-boot

On a normal entry, `/sys/kernel/security/lockdown` must read `[none]`, because lockdown turns off hibernation. A system started from a snapshot, whose `/proc/cmdline` holds `systemd.volatile=overlay`, is not checked. A kernel without that file has no lockdown support and passes.

### The record

`/var/lib/inayos-guard/status.json`, readable by every user, holds the last result of each command and their sum:

```json
{
  "time": "2026-11-19T10:00:00Z",
  "passed": false,
  "failures": ["/boot/initramfs-linux.img holds no kernel modules of 7.3.4-arch1-2, the version of /boot/vmlinuz-linux"],
  "update": { "time": "2026-11-19T10:00:00Z", "passed": false, "failures": ["..."], "previous_system_default": true },
  "boot": { "time": "2026-11-19T09:58:00Z", "passed": true, "failures": [] }
}
```

Times are UTC. `previous_system_default`, in the `check-update` result only, says whether rEFInd now starts the "Previous system" entry by default. `inayos-guard status` states it after a failure. The top-level `failures` lists the failures of `check-update`, then those of `check-boot`. A record that cannot be read is replaced at the next check.

`/var/lib/inayos-guard/failed` exists while the last `check-update` failed. `check-update` creates it on a failure and removes it on a pass. `check-boot` leaves it alone.
