# Encryption

This document describes the `encryption` module of the InayOS desktop profile, `crates/modules/src/encryption.rs`, and its files in `dotfiles/assets/encryption`.

The module builds the boot of an encrypted InayOS. The InayOS installer creates the LUKS2 volumes and writes the files that open them, as `docs/installer.md` describes. The module turns those files into a boot: an initramfs that opens both volumes with the TPM, inside a unified kernel image on the EFI system partition. rEFInd starts that image, because it cannot read the encrypted root.

The disk is never opened with a typed password in normal use. The TPM opens it at every start. The recovery key opens it when the TPM refuses, such as after a firmware update that ran without the boot guard, a cleared TPM, or on another computer.

## Layout

- The EFI system partition stays unencrypted and is mounted at `/efi`.
- The system partition is a LUKS2 volume labeled `inayos-system`, opened as `/dev/mapper/system`. It holds the Btrfs system with its subvolumes.
- The home partition is a LUKS2 volume, opened as `/dev/mapper/home`. It holds the ext4 home labeled `inayos-home`, and the hibernation swap file.
- Each volume has the recovery key and a TPM2 slot. The installer binds the TPM2 slot to no PCR, and the boot guard binds it to PCR 7, the Secure Boot state, at the first boot, as `docs/boot-guard.md` describes.

The installer writes two files that the module reads:

- `/etc/crypttab.initramfs`, mode 600, which opens `system` and `home` by their LUKS UUID with `tpm2-device=auto,discard`. The initramfs opens home too, so the swap file is reachable before resume.
- `/etc/kernel/cmdline`, which mounts the root from `/dev/mapper/system` with `rootflags=subvol=@`. The bootvine module adds its own options to this line.

## Files

Each file is under `dotfiles/assets/encryption`, installed with a recorded write at the same path under `/`.

- `/etc/mkinitcpio.conf.d/inayos-encryption.conf` puts the `sd-encrypt` hook right before `filesystems`, or last when the list has no `filesystems`. mkinitcpio sources its drop-ins as bash after `/etc/mkinitcpio.conf`, in name order. The drop-in rebuilds the `HOOKS` array from the list it receives, so the hooks of the configuration file and of the drop-ins read before it stay, and `inayos-volatile.conf`, read after it, appends `sd-volatile` to the result.
- `/etc/mkinitcpio.d/linux.preset` replaces the preset of the `linux` package. It builds the initramfs in `/boot`, which stays inside `@` so each snapshot holds the initramfs that matches its kernel, and the unified kernel image `/efi/EFI/Linux/inayos-linux.efi`. mkinitcpio builds the image with ukify and embeds `/etc/kernel/cmdline` in it.
- `/etc/systemd/user/inayos-recovery-key-reminder.service` shows a critical desktop notification at each login while `~/inayos-recovery-key.txt` exists, so the user moves the key into a password manager and deletes the copy.

## Install

Install first checks that `/etc/crypttab.initramfs` and `/etc/kernel/cmdline` exist and that a FAT filesystem is mounted at `/efi`. Without them, a running system skips the module with a note, and an installer-mode run fails. It then fails when the `root=` option of the kernel options names no volume that the crypttab opens.

It installs `systemd-ukify`, which builds the image, `tpm2-tss`, which the initramfs needs to reach the TPM, and `libnotify` for the reminder. It writes the files and creates `/efi/EFI/Linux`. It runs `mkinitcpio -P` when the drop-in or the preset changed, when the image is missing, or when an initramfs image in `/boot` lacks `systemd-cryptsetup`, which `lsinitcpio` shows. It enables the reminder for every user.

The secure-boot module signs the image when the run includes it, and sbctl signs it again each time mkinitcpio builds it.

## Verify

`verify` fails when a package is missing, when a file differs from the repository copy, when the kernel options do not mount the root from an opened volume, when the image is missing, when an initramfs image lacks `systemd-cryptsetup`, or when the reminder is not enabled. Without the installer's two files, a running system skips the check with a note.

## Remove

Remove refuses while `/etc/crypttab.initramfs` exists: without the hook and the preset, the next kernel update would build no image that opens the volumes. The volumes stay encrypted, because only a reinstall can undo the layout.
