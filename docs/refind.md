# rEFInd

This document describes the `refind` module, `crates/modules/src/refind.rs`.

rEFInd is the boot manager of every InayOS machine. It lets the user pick an entry with the mouse.

## Install

In installer mode, the module installs rEFInd onto the EFI system partition of the system being built, the first FAT filesystem mounted at `/efi`, `/boot/efi` or `/boot`. It writes `/boot/refind_linux.conf` with the kernel options that mount the new root, then runs `refind-install --yes`, which copies rEFInd and the driver for the filesystem of `/boot` and adds the "rEFInd Boot Manager" firmware boot entry. Every check comes before the first change: the EFI system partition, a driver for `/boot`, efivarfs and a root entry in `/etc/fstab` named by UUID or PARTUUID.

On a running system and in installer mode alike, the module then finds `EFI/refind/refind.conf` and writes, in that folder:

- `managed.conf`, the global settings from `dotfiles/assets/refind/refind/global.conf` followed by the firmware settings below;
- the Black and Pink theme in `themes/black-pink`;
- a placeholder `inayos-guard.conf`, when none exists, which the boot guard replaces (see `docs/boot-guard.md`);
- one managed block at the end of `refind.conf` that includes `managed.conf`, the theme and `inayos-guard.conf`, in that order.

A running system without rEFInd skips the module with a note.

## Firmware entries

Windows starts through its own firmware boot entry, never from inside rEFInd. Picking its entry sets the firmware's `BootNext` variable to that entry and resets the machine, so rEFInd is not part of the boot that BitLocker measures, and BitLocker does not ask for its recovery key.

The firmware settings in `managed.conf` are:

- `scanfor internal,external,optical,manual`. Without `firmware` in this list, rEFInd shows none of the firmware's boot entries by itself. `dont_scan_firmware` could only hide entries by label, and the labels of the rEFInd entry, the UEFI shell and the network and disk entries differ between firmwares.
- `dont_scan_files +,EFI/Microsoft/Boot/bootmgfw.efi`, which keeps the list from `refind.conf` and hides Windows Boot Manager, so rEFInd adds no entry that starts it directly. When the EFI system partition holds Windows Boot Manager and the firmware has an active Windows entry, the list also holds `<partition GUID>:EFI/BOOT/bootx64.efi`. Windows puts a copy of its boot manager there, and rEFInd would otherwise show it as a fallback boot loader. The partition GUID, from `findmnt --output PARTUUID`, keeps the fallback loaders of other disks, such as a USB stick, in the menu.
- one manual stanza titled "Windows", with `firmware_bootnum` set to the number of each active firmware entry that `efibootmgr` lists as "Windows Boot Manager". An inactive one is left out: it is a leftover, such as the entry of a wiped disk, and the firmware may skip it. Several active entries get titles that carry their `Boot####` number. A machine without Windows gets no stanza and no extra entry.

When the EFI system partition holds Windows Boot Manager but the firmware has no active Windows entry, rEFInd shows Windows' fallback loader, `EFI/BOOT/bootx64.efi`, as its only way into Windows. That entry starts Windows from inside rEFInd, so BitLocker, when it is on, asks for its recovery key.

`enable_mouse` comes from `global.conf`.

The stanza holds the entry number read at install, and nothing rewrites it at boot. Windows repairs and feature updates, a firmware reset or the user can recreate the Windows entry under another number. The menu entry then sets `BootNext` to a number that no longer exists, and the firmware starts rEFInd again, or to a number another entry now holds, which starts that entry. Nothing warns about it until someone runs `inayos verify`, which fails. Installing the module again writes the new number. Until then, the firmware's own boot menu still starts Windows.

## Verify

`verify` fails when, on the system being built, rEFInd's binary, its driver, its firmware boot entry or a kernel entry that mounts the root is missing. On both kinds of system it fails when `refind.conf` does not end with the managed block, when `managed.conf` differs from what install would write for the current firmware entries, or when a theme or guard file is missing.

## Remove

`remove` undoes what install recorded. When the module installed rEFInd, it first refuses unless the EFI system partition keeps another boot manager that the firmware starts: Windows Boot Manager, systemd-boot or GRUB with a firmware boot entry, or a fallback `EFI/BOOT/BOOTX64.EFI` that is not rEFInd.
