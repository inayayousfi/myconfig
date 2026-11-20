# Snapshots

This document describes the `snapshots` module of the InayOS desktop profile, `crates/modules/src/snapshots.rs`, and its files in `dotfiles/assets/snapshots`.

The module makes snapper snapshot the Btrfs system before and after every pacman transaction, keeps a fixed number of those snapshots, and lets the initramfs start a read-only snapshot. The boot guard's "Previous system" entry starts the newest pre-transaction snapshot that way, as `docs/boot-guard.md` describes.

The rollback from the USB stick's repair workspace puts the system back to one of these snapshots, as `docs/rollback.md` describes.

## Layout

The installer creates the Btrfs subvolumes `@` for the system and `@snapshots` for the snapshots, and mounts `@snapshots` at `/.snapshots`. `/boot` lives inside `@`, so every snapshot holds the kernel and initramfs that match its modules. `@var_cache`, `@var_log` and `@var_lib_docker` are separate subvolumes, so snapshots leave them out.

## Files

Each file is under `dotfiles/assets/snapshots`, installed with a recorded write at the same path under `/`.

- `/etc/snapper/configs/root` is snapper's configuration `root` for `/`, from snapper's default template with these changes: the number cleanup keeps at most 10 snapshots, and 5 marked important; no timeline snapshots are taken or cleaned; and empty pre and post pairs are kept, so the number cleanup is the only cleanup. A snapshot younger than an hour is never cleaned up.
- `/etc/mkinitcpio.conf.d/inayos-volatile.conf` adds the `sd-volatile` hook with `HOOKS+=(sd-volatile)`. mkinitcpio reads its drop-ins after `/etc/mkinitcpio.conf`, so the hook joins the list there and the hooks other modules add to that file stay. `sd-volatile` puts systemd's `systemd-volatile-root.service` and the `overlay` module into the initramfs. With `systemd.volatile=overlay` on the kernel command line, the initramfs mounts the read-only root with a memory-backed overlay on top.

The module adds `root` to the `SNAPPER_CONFIGS` line of `/etc/conf.d/snapper`, which snapper's package ships, keeping any other configuration listed there.

The configuration is written as a file, without `snapper create-config`, because `create-config` creates its own `.snapshots` subvolume and refuses a `/.snapshots` that already exists, as the mounted `@snapshots` does.

## Install

Install first checks that a Btrfs filesystem is mounted at `/.snapshots`. Without it, a running system skips the module with a note, and an installer-mode run fails.

It then installs `snapper` and `snap-pac`, writes the configuration, lists it in `/etc/conf.d/snapper`, rebuilds every initramfs with `mkinitcpio -P` when an image in `/boot` lacks the `sd-volatile` program, which `lsinitcpio` shows, and enables and starts `snapper-cleanup.timer`. In installer mode the timer is only enabled.

From then on, snap-pac's pacman hooks take a snapshot of type `pre` before each transaction, described by the pacman command, and a snapshot of type `post` after it, described by the packages it changed. Both use the number cleanup. snap-pac adds `--no-dbus` by itself inside a chroot, so the transactions of later modules in an installer-mode run are snapshotted too.

`verify` fails when a package is missing, when a file differs from the repository copy, when `/etc/conf.d/snapper` does not list `root`, when an initramfs image in `/boot` lacks the `sd-volatile` program, when `snapper --no-dbus -c root list` fails, or when the cleanup timer is not running. In installer mode it checks that the timer is enabled instead.

## Remove

`remove` disables the timer, puts back `/etc/conf.d/snapper`, deletes the configuration and the drop-in, rebuilds every initramfs without `sd-volatile`, and uninstalls the packages that install added. The snapshots already taken stay in `@snapshots`.

Without `sd-volatile`, the "Previous system" entry cannot start. The boot guard needs this module, so remove takes the guard out first.
