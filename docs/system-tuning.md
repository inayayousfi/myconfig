# System tuning

This document describes the `system-tuning` module of the InayOS desktop profile, `crates/modules/src/system_tuning.rs`, and its files in `dotfiles/assets/system-tuning`.

The module owns the InayOS desktop's settings for swap, networking and DNS, kernel values, open-file limits and the `ntsync` module. Each setting is a file under `dotfiles/assets/system-tuning`, installed with a recorded write at the same path under `/`. Every file except the zram configuration has `inayos-system-tuning` in its name.

## Settings

- **zram swap.** `/etc/systemd/zram-generator.conf` gives `zram-generator` one device, `zram0`, as large as RAM, compressed with zstd, at swap priority 100, so the kernel swaps to it before any swap on disk. A `tmpfiles.d` rule turns zswap off at every boot, because zswap would compress each page on its way to swap and zram would compress it again.
- **DNS.** A NetworkManager drop-in sets `dns=systemd-resolved`, so NetworkManager hands each connection's DNS servers to `systemd-resolved`. A rule in the module's `tmpfiles.d` file makes `/etc/resolv.conf` a link to resolved's stub file, `/run/systemd/resolve/stub-resolv.conf`, at every boot, so programs that read it ask resolved's local resolver. `systemd-resolvconf` provides the `resolvconf` command on top of `systemd-resolved`.
- **Swap and console.** A sysctl file sets `vm.swappiness = 100` and `vm.page-cluster = 0`, which zram's documentation recommends, and `kernel.printk = 3 3 3 3`, so the console shows only critical kernel messages and worse.
- **Open files.** Drop-ins for the system and user service managers set `DefaultLimitNOFILE=2048:2097152`. The kernel's system-wide limit, `fs.file-max`, keeps its default, which exceeds that hard limit of 2097152 on machines with more than about 20 GB of RAM.
- **ntsync.** A `modules-load.d` file loads the `ntsync` kernel module at boot, which Wine and Proton use for thread synchronization.

## Install

The module installs `zram-generator`, `systemd-resolvconf` and NetworkManager, writes the files, and enables `systemd-resolved`. It starts `systemd-resolved` before it writes the NetworkManager drop-in, so name lookups never point at a resolver that is not running.

It then enables NetworkManager without starting it, so a running system keeps its current network and the installed system starts NetworkManager at boot.

On a running system it then loads the sysctl file, loads `ntsync`, and asks a running NetworkManager to read its configuration again with `nmcli general reload`. The zram device, the zswap switch, the `/etc/resolv.conf` link and the service manager limits take effect at the next boot, in both modes. In installer mode none of these steps run, and the installed system applies every file when it boots.

`verify` fails when a package is missing, when a file differs from the repository copy, when `systemd-resolved` is not running, or when NetworkManager is not enabled. In installer mode it checks that `systemd-resolved` is enabled instead of running.

## Remove

`remove` deletes the files and uninstalls the packages that install added. Install records `/etc/resolv.conf` before it writes the `tmpfiles.d` rule, so `remove` deletes the rule first and then puts back the file or link that was there before the install. In installer mode, `arch-chroot` mounts the installer's own `/etc/resolv.conf` over the new system's file, so the recorded file is the installer's copy. It deletes the NetworkManager drop-in first, then reloads a running NetworkManager, which writes `/etc/resolv.conf` with the connection's own DNS servers again, and only then stops and disables `systemd-resolved`. The kernel values and the loaded `ntsync` module stay in the running kernel until the next boot.
