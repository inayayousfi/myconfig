# Rollback

This document describes the rollback from the USB stick's repair workspace: the `rollback` subcommand of `inayos-installer`, with its command line in `apps/installer/src/rollback.rs` and its logic in `crates/install-core/src/rollback.rs`.

The rollback puts an installed InayOS system back to one of the snapper snapshots that the `snapshots` module takes around each pacman transaction, kernel included. The home partition and the subvolumes that snapshots leave out, `@var_cache`, `@var_log` and `@var_lib_docker`, stay as they are.

## Command line

`sudo inayos-installer rollback [--disk DEVICE] [--snapshot N] [--recovery-key-file FILE] [--yes] [--log FILE]` runs as root on the live system. The repair terminal's help text names it.

- `--disk` names the disk of the system. Without it, the command takes the only disk that holds one, and refuses when several do.
- `--snapshot` is the number of the snapshot to go back to. Without it, the command lists the snapshots, oldest first, with their number, type, date in UTC and description, and asks for a number.
- `--recovery-key-file` names a file that holds the recovery key of an encrypted system. Without it, the TPM opens the volumes, and when it refuses, the command asks for the key with `systemd-ask-password`.
- `--yes` skips the confirmation. Without it, the command asks the user to type `yes`.
- `--log` defaults to `/var/log/inayos-rollback.log`, which gets the steps, the commands and their output, and the details.

With `--snapshot`, `--yes` and, for an encrypted system, either a TPM that opens it or `--recovery-key-file`, the command asks nothing. A question with no terminal to answer it fails and names the option that answers it.

The command exits 0 when the rollback finished, and 1 with the cause on stderr otherwise.

## Finding and opening the system

A disk holds an InayOS system when one of its partitions is labeled `inayos-system`. A Btrfs partition is a plain system, and a LUKS2 partition is an encrypted one. The partition labeled `inayos-home` is its home, and the `vfat` partition with the EFI system partition type is its EFI partition.

Opening refuses a system that hibernated, before anything opens read-write. The `HibernateLocation` firmware variable comes first. Then the swap file header at the top of the home partition is read through `debugfs`, as `docs/installer.md` describes for the installer. An encrypted home is opened read-only for that check and closed again.

An encrypted volume opens through its systemd-tpm2 token with `cryptsetup open --token-only`, or with the recovery key on `cryptsetup`'s standard input. The volumes get the mapper names `inayos-rollback-system` and `inayos-rollback-home`.

Opening then mounts the top level of the Btrfs filesystem, subvolume id 5, at `/run/inayos-rollback/top`. The snapshots are the numbered folders of `@snapshots` that hold a `snapshot` subvolume and an `info.xml` that snapper wrote.

## Rolling back

Before changing anything, the rollback refuses a snapshot that would not start the machine:

- When the current `@` holds sbctl's keys in `/var/lib/sbctl/keys`, the firmware has them enrolled and refuses an unsigned kernel. A snapshot without those keys is from before Secure Boot was set up, so the rollback refuses it.
- On an encrypted system, the snapshot's `/etc/mkinitcpio.d/linux.preset` must build `/efi/EFI/Linux/inayos-linux.efi`. A snapshot from before the `encryption` module builds no unified kernel image, so the rollback refuses it.

The rollback then runs these steps:

1. The current `@` becomes `@rollback-<time>`, with the time in UTC as `YYYYMMDD-HHMMSS`.
2. `btrfs subvolume snapshot` makes a writable snapshot of `@snapshots/<n>/snapshot` as the new `@`.
3. The new `@`'s `/etc/fstab` loses its `subvolid=` options. genfstab writes both `subvolid=` and `subvol=`, and the new `@` has a new subvolume id, while `subvol=/@` still names it.
4. On an encrypted system, the new `@` is mounted at `/run/inayos-rollback/root` with the EFI partition at its `/efi`. The current unified kernel image is copied to `/efi/EFI/Linux/inayos-linux.efi.before-rollback`. `arch-chroot` runs `mkinitcpio -P` in the new `@`, which builds the image from the snapshot's kernel. When the current system had sbctl's keys, `arch-chroot` then runs `sbctl sign-all`. The copy is deleted once the image is built and signed.

A plain system needs nothing more: `/boot` lives inside `@`, and rEFInd starts the kernel in `@/boot` with `rootflags=subvol=@`. rEFInd does not look inside `@rollback-<time>` or `@snapshots`.

The system from before stays on the disk as `@rollback-<time>`. Deleting it is left to the user.

Closing unmounts what the rollback mounted and closes the system volume, whatever the result.

## Failures

Each failure says what it left on the disk:

- A hibernated system, a TPM that refuses, a missing snapshot, a `@rollback-<time>` that already exists, and every failure before the rename leave the disk unchanged.
- A snapshot that would not start the machine leaves the disk unchanged, and the error says why.
- When the snapshot, the fstab or the boot image fails, the rollback puts the copied image back, deletes the new `@` and renames `@rollback-<time>` back to `@`, so the system is as before.
- When that undo fails too, the error names `@rollback-<time>`, which must be renamed back to `@` before the system starts.
- When `sbctl sign-all` fails, the new `@` and its boot image stay in place, unsigned. The error says to run `sbctl sign-all` in a chroot of it, because Secure Boot refuses the image until then. The image from before stays as `inayos-linux.efi.before-rollback`.
