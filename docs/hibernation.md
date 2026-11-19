# Hibernation

This document describes the `hibernation` module of the InayOS desktop profile, `crates/modules/src/hibernation.rs`, and its file in `dotfiles/assets/hibernation`.

The module gives the system a swap file to hibernate into: `/home/swapfile` on the ext4 home, as large as the memory rounded up to a whole GiB. A swap unit turns it on at boot with priority 0, below zram's priority of 100, so the system swaps to zram first.

systemd hibernates into the swap file and writes its location to the `HibernateLocation` firmware variable. The `systemd` hook of the initramfs reads that variable and resumes, so the kernel needs no `resume=` option. On an encrypted system, the initramfs opens the home volume before it resumes, because `/etc/crypttab.initramfs` lists it, as `docs/encryption.md` describes.

The InayOS installer locks the module, so every install has it. The installer and the USB stick's live terminals refuse to write to a disk whose InayOS is hibernated, as `docs/installer.md` describes. The installer also checks that every home it creates or keeps has room for the swap file. The USB stick masks `systemd-hibernate-clear.service`, which would otherwise delete `HibernateLocation` when the stick starts on a hibernated machine. The variable is the only sign of an encrypted hibernated InayOS that the stick can read, and the hibernated system needs it to resume, as `test/usb/README.md` describes.

## Files

`/etc/systemd/system/home-swapfile.swap`, under `dotfiles/assets/hibernation`, is installed with a recorded write at the same path. systemd requires that name, which it derives from the path of the swap file.

`/etc/systemd/system/systemd-logind.service.d/inayos-hibernation.conf` sets `ProtectHome=read-only` for logind. Before it hibernates, logind reads where the swap file lies on disk, and logind's own unit hides `/home` otherwise. A running logind keeps its earlier setting until the next boot.

## Firmware limit

The kernel resumes only when the firmware gives the same memory map as at hibernation. Some firmware moves a region between boots, and the kernel then starts the system fresh, which loses the hibernated session but not the disk. OVMF with a TPM does this on some boots.

## Install

Install first checks that an ext4 filesystem is mounted at `/home`. Without it, a running system skips the module with a note, and an installer-mode run fails.

It reads the memory size from `MemTotal` in `/proc/meminfo`. The swap file is ready when it has that size rounded up to a whole GiB, mode 600, owner root, and a swap signature, which `blkid` reads. When the file is missing, the module records it and creates it with `mkswap --file --size <bytes>`, which allocates every block, as the kernel needs to write the memory image. When the file exists but is not ready, such as after a memory change, install fails and leaves the file in place, because install never undoes an earlier run. When the module made the file, the message names `inayos remove hibernation`, which deletes it, after which install creates it at the new size. Otherwise it says to turn the file off with `swapoff` and delete it. A reinstall that keeps home does not reach this case: the installer deletes the earlier system's swap file from the kept home.

It then writes the unit, enables it and starts it. In installer mode the unit is only enabled.

## Verify

`verify` fails when the unit differs from the repository copy, when the swap file is missing or not ready, when an initramfs image in `/boot` lacks `systemd-hibernate-resume`, or when the unit is not active. In installer mode it checks that the unit is enabled instead.

## Remove

`remove` stops the unit while the swap file is on, then disables the unit and deletes it and the swap file.
