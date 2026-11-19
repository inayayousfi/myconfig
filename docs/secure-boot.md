# Secure Boot

This document describes the `secure-boot` module of the InayOS desktop profile, `crates/modules/src/secure_boot.rs`.

The module sets up Secure Boot with the machine's own keys, through `sbctl`: it signs every file that the firmware or rEFInd starts, then enrolls the keys in the firmware together with Microsoft's keys and the firmware's built-in keys. Microsoft's keys keep Windows and the option ROMs of add-in cards starting. The installer runs the module only when its Secure Boot answer is on. It needs `refind`, and `encryption` when the run includes it, so the unified kernel image exists when it signs.

Enrolling writes into the firmware and cannot be undone. On a running system the module asks before it. In installer mode the module creates the keys and signs, and the InayOS installer enrolls once `inayos install` has passed, as `docs/installer.md` describes. A failed install therefore leaves the firmware in Setup Mode, and the unsigned installer stick still starts.

## Install

Install first looks for rEFInd's configuration on the EFI system partition, as the refind module does. Without it, a running system skips the module with a note, and an installer-mode run fails.

It then installs `sbctl` and reads `sbctl status --json`:

1. When sbctl has no keys and the firmware is not in Setup Mode, the firmware cannot take new keys. An installer-mode run fails with "turn on Setup Mode in the firmware settings". A running system skips the module with a note that says the same.
2. While the firmware is in Setup Mode, a running system asks whether to enroll the keys. On a no, the module records the decline and changes nothing more, and `verify` passes. Nobody answering skips the module. Installer mode asks nothing.
3. When sbctl has no keys, `sbctl create-keys` creates them in `/var/lib/sbctl`.
4. It signs each file with `sbctl sign -s`, which also saves it in sbctl's list of files to sign:
   - rEFInd and its drivers in `EFI/refind`, from their copies in `/usr/share/refind`, with `-o <file on the EFI system partition>`, so sbctl signs the package's new copy into place after a rEFInd update;
   - every EFI tool in `EFI/tools`, which rEFInd shows, in place;
   - every unified kernel image in `EFI/Linux`, which an encrypted system starts, in place;
   - every kernel `/boot/vmlinuz-*`, in place.
5. It runs `sbctl verify --json` on those files and fails when one is not signed with the machine's key.
6. On a running system that answered yes, it runs `sbctl enroll-keys --microsoft`, with `--firmware-builtin` when the firmware has a built-in signature database (`dbDefault`). This step comes last: from the next boot on, the firmware starts only signed files, and every boot file is signed by then. On an encrypted system, which has `/etc/crypttab.initramfs`, it first runs `inayos-guard prepare-firmware-update`, because new keys change the Secure Boot state that the TPM checks before it opens the disk. It fails before enrolling when the guard program is missing.

When sbctl already has keys and the firmware is out of Setup Mode, the module takes the keys as enrolled and only signs.

sbctl's own pacman hook, `zz-sbctl.hook`, signs the saved files again after each transaction that changes a kernel or an EFI file. sbctl's mkinitcpio post hook signs a unified kernel image again each time mkinitcpio builds it. A kernel in a snapshot was signed while it was the system's kernel, so the boot guard's "Previous system" entry starts a signed kernel too. The boot guard checks the signatures after every such transaction.

Without encryption, the initramfs is an unsigned file next to the kernel, so Secure Boot alone does not protect it. With encryption, the kernel, the initramfs and the kernel options are one signed unified kernel image.

## Verify

`verify` passes after a decline. A running system with no keys and no Setup Mode skips the check with a note. Otherwise `verify` fails when `sbctl` is missing, when sbctl has no keys, or when `sbctl verify --json` does not report every rEFInd file, EFI tool, unified kernel image and kernel as signed. On a running system it also fails while the firmware is still in Setup Mode. In installer mode that check is left to the installer, which runs `verify` after enrolling and reads the Setup Mode variable itself.

## Remove

Remove refuses while Secure Boot is on: without sbctl and its list, the next kernel update would leave an unsigned kernel that the firmware refuses to start. Secure Boot is turned off in the firmware settings first.

Once it is off, `remove` takes the files that install added out of sbctl's list with `sbctl remove-file`, and uninstalls `sbctl` when install added it. The signed files stay signed, which changes nothing without Secure Boot.

The keys stay in `/var/lib/sbctl`, and the firmware keeps them. Remove cannot take keys out of the firmware: only the firmware settings can clear them or bring back Setup Mode. Deleting the keys would leave the firmware trusting keys that nothing on the machine holds, so install records neither the keys nor their enrollment.
