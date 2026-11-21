# Boot guard

This document describes the boot guard of the InayOS desktop profile: the `boot-guard` module, `crates/modules/src/boot_guard.rs`, with its files in `dotfiles/assets/boot-guard`, and the guard program `inayos-guard`, which the `inayos` package installs with its pacman hook (see `docs/repository.md`).

The guard checks every boot file that rEFInd starts after each update of those files, and keeps a working way back: a rEFInd entry that starts the system as it was before the last update.

## The module

The module needs `inayos-repository`, `refind` and `snapshots`, and `secure-boot` when the run includes it. The guard comes only from the `inayos` package, so on a running system without that package the module runs only once `inayos-repository` can install it, which needs a binary that carries the release key and a release that holds the repository. It comes after `refind`, `secure-boot` and `bootvine` in the profile, so its first check sees the signed files and the rebuilt initramfs.

### Install

Install first looks for rEFInd's configuration on the EFI system partition, as the refind module does. Without it, a running system skips the module with a note, and an installer-mode run fails.

It then:

1. installs the `inayos` package and `libnotify`, which provides `notify-send`;
2. checks that `/usr/bin/inayos-guard` is executable and that the package's hook is in place, and fails before writing anything when either is missing;
3. writes its files under `/`, each a recorded write;
4. enables `inayos-guard-boot.service` and the user unit `inayos-guard-notify.path`, and starts the path unit when a graphical session runs;
5. runs `inayos-guard check-update` once, so `inayos-guard.conf` and the record exist before the first boot. It records `inayos-guard.conf` and the folder `/var/lib/inayos-guard` first, so remove deletes them.

The refind module's managed block in `refind.conf` includes `inayos-guard.conf` last, after the theme, so the guard's `default_selection` wins. The refind module writes a placeholder `inayos-guard.conf` holding one comment when none exists, so the include always finds a file.

### Files

The `inayos` package installs the hook, from `packaging/inayos`:

- `/usr/share/libalpm/hooks/zzz-inayos-guard.hook` runs `inayos-guard check-update` after every transaction that installs, upgrades or removes a kernel, a file under `/boot`, rEFInd's package files, the mkinitcpio configuration, `sbctl`, `snapper` or the guard itself, so an update of the `inayos` package checks the boot files with the new guard. It also has every trigger of `90-mkinitcpio-install.hook`, which rebuilds the initramfs: its paths, such as firmware, DKMS sources, systemd and udev, and an install or upgrade of the `mkinitcpio` package. Its name sorts after `90-mkinitcpio-install.hook` and sbctl's `zz-sbctl.hook`, so the guard checks the rebuilt initramfs and the new signatures. The guard exits with 0 when a check fails, so the hook never aborts the transaction. The hook runs the guard only while `/var/lib/inayos-guard` exists, which this module creates: before that, the boot files the guard checks may not exist yet, as during the install of the packages that precede rEFInd. It also skips the transaction that removes the guard.

The module writes the units, from `dotfiles/assets/boot-guard`:

- `/etc/systemd/system/inayos-guard-boot.service` runs `inayos-guard check-boot` once at each boot.
- `/etc/systemd/system/fwupd.service.d/inayos-guard.conf` runs `inayos-guard prepare-firmware-update` when fwupd stops after it changed what the TPM measures in PCR 7. When fwupd starts, the drop-in saves a checksum of the Secure Boot variables `PK`, `KEK`, `db` and `dbx` in `/run/inayos-guard-fwupd`. When fwupd stops, the guard runs if that checksum changed, if the record is missing, or if a UEFI capsule waits on the EFI partition (`/efi/EFI/*/fw/fwupd-*.cap`), which the firmware applies at the next boot. fwupd also starts for queries, such as Plasma Discover's at login and the daily metadata refresh; those change nothing, so they add no TPM slot bound to no PCR, which would open the disk under any boot chain. The `+` prefix runs both commands with full privileges outside fwupd's sandbox, because the guard changes the LUKS headers. fwupd stops when it has been idle for a while and at shutdown, so the slots are in place before the next boot.
- `/etc/systemd/user/inayos-guard-notify.path` starts `inayos-guard-notify.service` while `/var/lib/inayos-guard/failed` exists. It is wanted by `graphical-session.target`, so the notification has a desktop to show on.
- `/etc/systemd/user/inayos-guard-notify.service` shows a critical desktop notification with the output of `inayos-guard status`. It then stays active until the failed file is gone, so the path unit does not start it again in a loop, and the next failure notifies again.

### Verify

`verify` fails when the `inayos` package or `libnotify` is missing, when the guard or the hook is missing, when a file differs from the repository copy, when the boot service or the user path unit is not enabled, when `inayos-guard.conf` is missing, or when the record says that the last check failed. The failure then lists the guard's failures.

### Remove

`remove` disables the units, deletes the files, `inayos-guard.conf` and `/var/lib/inayos-guard`, and uninstalls `libnotify` when install added it. The `inayos` package stays while the `inayos-repository` module needs it. When the refind module wrote the placeholder first, remove puts the placeholder back, so the include line in rEFInd's managed block still finds a file.

## The guard program

`inayos-guard` is a small program apart from `inayos`, so a kernel update never depends on the large program working. Its logic is the library `crates/guard`, and `apps/guard` is the command line around it. It is built for `x86_64-unknown-linux-musl`, has no interface and no network access, and depends only on `serde`, `serde_json` and `thiserror`. Every program it runs and every file it reads or writes goes through its `System` trait, so its tests run against a fake machine.

### Commands

| Command | Run by | Exit status |
| --- | --- | --- |
| `inayos-guard check-update` | root, from the pacman hook after a transaction that touches the boot files | 0 when it recorded a result, failed checks included; 3 when it could not write rEFInd's file or the record |
| `inayos-guard check-boot` | root, from `inayos-guard-boot.service` at each boot | 0 when the check passed, 1 when it failed, 3 when it could not record the result |
| `inayos-guard prepare-firmware-update` | root, before a firmware update and before the secure-boot module changes the Secure Boot keys | 0 when every encrypted volume has a temporary TPM2 slot, or the system is not encrypted; 1 when a volume has none; 3 when it could not record the result |
| `inayos-guard rebind` | root, by hand, when a failure says no TPM2 slot opens a volume | 0 when every volume opens with a TPM2 slot afterwards and the boot check passed, or the system is not encrypted; 1 when it refused or a step failed; 3 when it could not read the key, write the key file or record the result |
| `inayos-guard status` | anyone | 0 when the last recorded results passed, 1 when one failed or no check has run |

Wrong arguments exit with 2. Failures go to standard error, one per line and prefixed with `inayos-guard:`, so pacman shows them. The changes the commands make to the encrypted volumes go there too.

Every command except `status` holds an exclusive lock on `/run/inayos-guard.lock` while it runs, so no command acts on TPM2 slots or a record that another one is changing. A command that finds the lock taken waits for it.

### Encrypted systems

The guard treats the system as encrypted when `/etc/crypttab.initramfs` exists. Each of its lines names a LUKS2 volume by its mapper name and its device, such as `system UUID=<luks uuid>`, and the guard runs the LUKS tools on that device's link under `/dev/disk`.

rEFInd cannot read the encrypted system partition. It starts InayOS from the unified kernel image `EFI/Linux/inayos-linux.efi` on the EFI system partition, which holds the kernel, the initramfs and the kernel options. The kernels and boot images in `/boot` stay on the Btrfs root, so each snapshot holds its own.

Each volume opens with a TPM2 slot bound to PCR 7, the Secure Boot state, or with the recovery key. A temporary slot is a TPM2 slot bound to no PCR: the TPM opens the volume whatever the machine booted. The installer leaves one for the first boot, and `prepare-firmware-update` adds one before an update that changes PCR 7.

The guard reads the slots from `cryptsetup luksDump --dump-json-metadata <device>`. A slot is a token of type `systemd-tpm2` whose `keyslots` names its key slot. Its `tpm2-pcrs` list is `[7]` for the normal slot and `[]` for a temporary one. The guard leaves alone any TPM2 slot with a PIN, a pcrlock policy or a signed PCR policy, and any slot that is not a TPM2 slot, such as the recovery key.

### check-update

When `/proc/cmdline` holds `systemd.volatile=overlay`, the system started from a snapshot, and `/boot` and `/.snapshots` show that snapshot instead of the system subvolume. The command then checks nothing, leaves `inayos-guard.conf`, the record and the failed file as they are, and exits with 0.

Otherwise it reads what rEFInd starts:

- rEFInd's folder, `EFI/refind` on the first of `/efi`, `/boot/efi` and `/boot` that holds `EFI/refind/refind.conf`;
- `refind_x64.efi`, the drivers in `drivers_x64` and the EFI tools in `EFI/tools`;
- each `/boot/vmlinuz-<name>` with its `/boot/initramfs-<name>.img` and, when present, `initramfs-<name>-fallback.img`;
- the kernel options of each entry in `/boot/refind_linux.conf`, on a system that is not encrypted;
- on an encrypted system, the unified kernel image `EFI/Linux/inayos-linux.efi` and the kernel options of `/etc/kernel/cmdline`, which mkinitcpio puts in the next image.

It then checks:

- **Signatures.** On an encrypted system, the files checked are rEFInd's EFI binaries and the unified kernel image, because rEFInd starts no kernel from `/boot`. The check runs when Secure Boot is on, or when `sbctl status --json` reports sbctl's keys installed and the firmware out of Setup Mode, which is the state right after the keys are enrolled and before the next boot. Secure Boot is on when the last byte of `/sys/firmware/efi/efivars/SecureBoot-8be4df61-93ca-11d2-aa0d-00e098032b8c` is 1. Each EFI binary and each kernel goes through their own `sbctl verify --json --disable-landlock <file>`. Landlock is off because it hides files outside the EFI system partition that sbctl's database does not list. A file passes when sbctl reports `"is_signed": 1`.
- **Boot images.** The kernel version comes from the kernel image's x86 boot header. `lsinitcpio` lists each boot image, and the image passes when it holds a kernel module file (`.ko`, compressed or not) under `usr/lib/modules/<version>/`.
- **Root disk.** On a system that is not encrypted, the `root=` of every entry names a disk that exists: its link under `/dev/disk/by-uuid`, `by-label`, `by-partuuid` or `by-partlabel`, or a `blkid` lookup. On an encrypted system, the `root=` of `/etc/kernel/cmdline` and that of the image's own kernel options are `/dev/mapper/<name>`, with `<name>` a volume of `/etc/crypttab.initramfs`.
- **Unified kernel image.** On an encrypted system, the image is a PE file with a `.linux`, a `.cmdline` and an `.initrd` section. The guard reads the sections from the PE section table itself. The kernel version comes from the `.linux` section's boot header. The `.initrd` section goes to `/run/inayos-guard/initrd.img` for the time `lsinitcpio` lists it, and passes when it holds a module of that version.
- **rEFInd.** `refind_x64.efi` exists.

It writes `EFI/refind/inayos-guard.conf` in rEFInd's folder in one step, through a temporary file and a rename. The file holds:

- A "Previous system" entry for the newest snapper "pre" snapshot that can start. The guard reads snapper's `info.xml` files in `/.snapshots`, not snapper's output. A snapshot can start when its `/boot` holds a kernel and its boot image, the boot image holds that kernel's modules, and, when signatures are checked, its kernel is signed. On an encrypted system the snapshot's kernel signature is not checked, because the entry starts an image the guard signs. These checks only choose the snapshot: a snapshot that fails them is left out and named on standard error, and it never counts as a failure of the normal entry.

  On a system that is not encrypted, the entry starts the snapshot's kernel from the Btrfs volume labelled `inayos-system`, with the options `root=<the normal entry's root> rootflags=subvol=@snapshots/<n>/snapshot systemd.volatile=overlay rw` followed by the other options of the normal entry, such as `quiet` and the console colors. A microcode image in the snapshot's `/boot` goes first, as an `initrd=` option, because rEFInd passes one `initrd` per entry after the options.

  On an encrypted system, the entry starts the unified kernel image `EFI/inayos/previous.efi`, which the guard builds for the snapshot. The stanza has only a `loader`, so rEFInd loads it from the EFI system partition it started from. The guard builds it with `ukify build --linux=<snapshot>/boot/vmlinuz-linux --initrd=<microcode> --initrd=<snapshot>/boot/initramfs-linux.img --cmdline=<options> --output=EFI/inayos/previous.efi.new`, with the microcode only when the snapshot's `/boot` holds it. The options are those of `/etc/kernel/cmdline` with the snapshot's root, as above. When that file's `root=` is not a volume of `/etc/crypttab.initramfs`, the guard takes the options this system started with from `/proc/cmdline`. When signatures are checked, `sbctl sign` signs the new image and `sbctl verify` must report it signed. The guard then renames it to `previous.efi`, so the entry never loads a half-built image. A snapshot whose image cannot be built or signed is left out and named on standard error, like one that fails its checks.
- `default_selection`. It names the normal entry, as `"vmlinuz-linux from"`, which matches rEFInd's title for that kernel and not for `vmlinuz-linux-lts`. On an encrypted system it names the image, as `"inayos-linux.efi"`. It names `"Previous system"` when a check failed and the entry exists. Without a snapshot that can start, the normal entry stays the default, so a failure only ever makes an entry that passed its own checks the default.

After writing the file, it marks the entry's snapshot important with `snapper --no-dbus -c root modify --userdata important=yes <n>`. Snapper's number cleanup then counts it against `NUMBER_LIMIT_IMPORTANT` instead of `NUMBER_LIMIT`, so ordinary pacman transactions, which do not run the guard, do not delete it as soon. A failure to mark it is printed and is not a failed check.

Then it records the result.

### check-boot

A system started from a snapshot, whose `/proc/cmdline` holds `systemd.volatile=overlay`, is not checked and its volumes are left alone. On a normal entry:

- `/sys/kernel/security/lockdown` must read `[none]`, because lockdown turns off hibernation. A kernel without that file has no lockdown support and passes.
- On an encrypted system, each volume with a temporary slot is bound to PCR 7 again.
- On an encrypted system, at least one TPM2 slot must open each volume.

The new PCR 7 slot opens the volume only in the Secure Boot state of this boot. When `sbctl status --json` reports sbctl's keys installed and Secure Boot is off, the boot does not have the state the machine was set up with. The guard then leaves the slots as they are and records a failure.

When `prepare-firmware-update` ran during this boot, its temporary slots protect an update that has not changed PCR 7 yet. The guard compares the `boot_id` of the `firmware` result with `/proc/sys/kernel/random/boot_id`, and when they match it leaves the slots as they are and records a note.

Otherwise, for each volume with a temporary slot, in this order:

1. `systemd-cryptenroll --unlock-tpm2-device=auto --tpm2-device=auto --tpm2-pcrs=7 <device>` enrolls a new slot. systemd-cryptenroll tries each TPM2 slot in turn to unlock the volume, so the temporary slot unlocks it when PCR 7 changed.
2. The LUKS2 header must hold a new token bound to PCR 7.
3. `cryptsetup open --test-passphrase --token-only --token-id <token> <device>` must open the volume with that token alone.
4. `systemd-cryptenroll --wipe-slot=<slots> <device>` wipes the temporary slot and every earlier PCR 7 slot, which a changed PCR 7 makes useless.

When a step fails, the volume keeps every slot it had, so the temporary slot still opens it at the next boot, and the next `check-boot` tries again.

Then the guard tests each of its TPM2 slots with `cryptsetup open --test-passphrase --token-only --token-id <token> <device>`. A cleared TPM, or a PCR 7 change made without `prepare-firmware-update`, leaves slots that no longer open the volume. When no TPM2 slot opens a volume, the initramfs asks for the recovery key at every start, so the guard records a failure that names `sudo inayos-guard rebind`.

The result records each change, each note, and the TPM2 slots of each volume, with the slots that do not open it marked. On a normal boot of an encrypted system, a `firmware` result from an earlier boot leaves the record: this boot used its temporary slots, or the boot result says why they stay.

### rebind

On each volume that no TPM2 slot opens, `rebind` binds the TPM to PCR 7 again with the recovery key. It asks for the key once with `systemd-ask-password` and keeps it in `/run/inayos-guard/recovery-key`, mode 600, while it works. For each such volume it runs `systemd-cryptenroll --unlock-key-file=<file> --tpm2-device=auto --tpm2-pcrs=7 --wipe-slot=<dead slots> <device>`. systemd-cryptenroll wipes the dead slots only after the new slot is in, and never the new slot. The guard then removes the key file and runs `check-boot`, which tests the new slots and records the result with the changes.

It asks nothing and changes nothing on a system started from a snapshot, when Secure Boot is off while sbctl holds the machine's keys, or when every volume already opens with a TPM2 slot.

### prepare-firmware-update

A firmware update and a change to the Secure Boot keys or databases change PCR 7, so the PCR 7 slot no longer opens the volumes at the next boot. Before such a change, the command adds a temporary slot to each volume of `/etc/crypttab.initramfs` that has none, with `systemd-cryptenroll --unlock-tpm2-device=auto --tpm2-device=auto --tpm2-pcrs= <device>`, and checks that the header holds the new token. The next normal boot's `check-boot` binds the volume to PCR 7 again and removes the temporary slot. Until then, the TPM opens the volumes whatever the machine boots.

On a system that is not encrypted it does nothing, records nothing, and exits with 0.

### The record

`/var/lib/inayos-guard/status.json`, readable by every user, holds the last result of each checking command and their sum:

```json
{
  "time": "2026-11-19T10:00:00Z",
  "passed": false,
  "failures": ["/boot/initramfs-linux.img holds no kernel modules of 7.3.4-arch1-2, the version of /boot/vmlinuz-linux"],
  "update": { "time": "2026-11-19T10:00:00Z", "passed": false, "failures": ["..."], "previous_system_default": true },
  "boot": {
    "time": "2026-11-19T09:58:00Z", "passed": true, "failures": [],
    "tpm": ["system: TPM2 slot 2 bound to PCR 7", "home: TPM2 slot 2 bound to PCR 7"],
    "changes": ["system: enrolled TPM2 slot 2 bound to PCR 7", "system: wiped the replaced TPM2 slots 1", "..."]
  },
  "firmware": { "time": "2026-11-18T17:00:00Z", "passed": true, "failures": [], "tpm": ["..."], "changes": ["system: added temporary TPM2 slot 1 bound to no PCR", "..."], "boot_id": "..." }
}
```

Times are UTC. `previous_system_default`, in the `check-update` result only, says whether rEFInd now starts the "Previous system" entry by default. `inayos-guard status` states it after a failure. `tpm`, `changes` and `notes` exist on an encrypted system only. `tpm` holds one line per volume with its TPM2 slots after the command, `changes` one line per slot the command enrolled or wiped, and `notes` what it left alone on purpose. `firmware` holds the last `prepare-firmware-update` with the `boot_id` of its boot, until a normal boot after it runs `check-boot`. `inayos-guard status` prints the changes, the notes and the slots under each result.

The top-level `failures` lists the failures of `check-update`, then those of `check-boot`, then those of `prepare-firmware-update`. A record that cannot be read is replaced at the next check.

`/var/lib/inayos-guard/failed` exists while the top-level `passed` is false. Every command that records a result creates or removes it to match.
