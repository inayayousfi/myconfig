# InayOS USB image

This folder holds the `mkarchiso` profile of the InayOS USB stick: the live system that runs the installer and the repair tools. `cargo xtask vm usb build` turns it into `<state>/usb/inayos.iso` inside a throwaway Arch Linux VM (see `test/vm/README.md`).

## How the profile is built

`profile/` is an overlay on archiso's own `releng` profile, read from `/usr/share/archiso/configs/releng` of the archiso installed where the image is built. The xtask builds the final profile in four steps:

1. Copy `releng`.
2. Copy `profile/` over it. Each overlay file replaces or adds the file at the same path, except `packages.x86_64`, whose lines are appended to releng's list.
3. Place the built binaries at `airootfs/usr/local/bin/inayos-installer` and `airootfs/usr/local/lib/inayos/inayos`, copy the local InayOS repository that the xtask built on the host into `airootfs/usr/local/lib/inayos/repo` (see `docs/repository.md`), and extract the latest Axidev OSK Linux release into `airootfs/opt/`, which gives `/opt/axidev-osk/bin/axidev-osk`.
4. Run `mkarchiso -v -w <work> -o <out> <profile>`.

The overlay holds only regular files and folders, so any copy keeps it intact. An overlay can replace or add files, never delete them. A releng unit that the live system must not run is masked with an empty file at `airootfs/etc/systemd/system/<unit>`, which systemd treats like a link to `/dev/null`. A unit the live system starts is pulled in by a `Wants=` drop-in on its target instead of a `.wants` link.

`mkarchiso` copies `airootfs` without its file modes. Every file that needs a mode other than 644, including the binaries the xtask places, has an entry in `file_permissions` in `profiledef.sh`.

`profiledef.sh` replaces releng's. It sources releng's `profiledef.sh` from `/usr/share/archiso/configs/releng`, so the image takes the settings and file modes of the archiso installed where it is built. It then names the image InayOS, with `iso_name` `inayos` and `iso_label` `INAYOS_<year><month>`, and adds the modes of its own files.

## The live system

### Session

- getty logs the user `live` in on the first console. `~/.zprofile` then starts Sway, with its output in the journal under the `sway` identifier.
- `live` belongs to `wheel` and `uinput` through `/etc/sysusers.d/inayos-live.conf`, and runs `sudo` without a password.
- Workspace 1, "Installer", runs the installer's screens. Sway starts `inayos-installer` through `sudo`, which keeps `WAYLAND_DISPLAY`, `XDG_RUNTIME_DIR` and `SWAYSOCK`, so the installer runs as root and draws in the session of `live`. The session switches to workspace 1 before it starts the installer, because Sway places a window on the workspace that was focused when its command started.
- Workspace 2, "Repair", runs a Ghostty terminal titled "InayOS repair" that lists the repair tools, the rollback command (`docs/rollback.md`) and the command that installs from an answers file without the screens.
- Each terminal that `inayos-live-welcome` opens first runs `inayos-hibernation-check`. It warns when an InayOS on the machine is hibernated, because writing to its disk loses the work it saved. It reads only: the `HibernateLocation` firmware variable, and the swap file header of each unencrypted InayOS home partition through `debugfs`. The installer refuses such a disk with the same checks.
- systemd's `systemd-hibernate-clear.service` is masked. On every boot that does not resume, it deletes the `HibernateLocation` firmware variable. On the stick, that would hide a hibernated InayOS from the checks, and that InayOS would no longer resume when it starts again, because nothing else tells it where its saved memory is.
- Axidev OSK starts with the session. It is a layer-shell overlay, so it shows on both workspaces.
- Super+1 and Super+2 switch workspaces, and the bar's workspace buttons do the same with a click. Super+Return opens a terminal, and Super+B opens the Arch wiki in Epiphany.
- Super+I starts the installer's screens again on workspace 1 when none run, for example after they closed or failed to start. The repair terminal's help text and the motd say so.
- Without a GPU driver, as in a VM, Sway, Ghostty and Epiphany draw through Mesa's software renderer.

### Axidev OSK

Axidev OSK emits keys through `/dev/uinput`. The image carries the same two files that `axidev-osk linux setup-permissions` writes: `/etc/modules-load.d/axidev-osk-uinput.conf` loads the module at boot, and `/etc/udev/rules.d/70-axidev-io-uinput.rules` gives the device to the `uinput` group.

Its runtime comes from the Arch packages its Linux release names: `python`, `pyside6`, `qt6-wayland`, `layer-shell-qt`, `libinput`, `systemd` and `libxkbcommon`.

### Network and remote access

- NetworkManager configures every interface. Releng's `iwd` and `systemd-networkd` units are masked. `systemd-resolved` stays, and NetworkManager sends its DNS servers there.
- polkit lets `live`, the user of the active local session, change connections with `nmtui` and `nmcli` without `sudo`.
- The NetworkManager dispatcher has no D-Bus alias, so it never starts. The image carries no dispatcher scripts.
- `sshd` runs as in releng and accepts root logins.
- cloud-init runs as in releng. `/etc/cloud/cloud.cfg.d/90-inayos-live.cfg` makes the keys of a NoCloud seed disc (label `CIDATA`) reach root, creates no default user, keeps the hostname `inayos-live`, and leaves the network to NetworkManager.

### Packages

`profile/packages.x86_64` adds to releng's list:

- NetworkManager and polkit;
- Sway, Ghostty, Mesa and a font for the session;
- the libraries that the installer's screens load when they start: `wayland` for the Wayland client and cursor libraries, and `libxkbcommon`;
- the Axidev OSK runtime;
- the repair tools: `testdisk` (with `photorec`), `ddrescue`, `smartmontools`, `efibootmgr`, `sbctl`, `refind`, `snapper`, `mkinitcpio`, `arch-install-scripts`, `btrfs-progs`, `e2fsprogs`, `dosfstools` and `gptfdisk`;
- `cryptsetup` and `tpm2-tss`, with which the installer formats the encrypted volumes and enrolls the TPM;
- `ntfs-3g`, whose `ntfs-3g.probe` and `ntfsresize` the installer uses to check and shrink the Windows partition;
- Epiphany, the browser for the Arch wiki.

Releng already lists some of the repair tools. The overlay lists them again so that the repair workspace does not depend on releng's choice, and pacman skips the duplicates.

The installer's screens draw with the CPU renderer and carry their font, so they need no GPU driver and no font package.

The installer downloads every Arch package of the new system from the Arch mirrors, so the image carries none of them. It carries only the `inayos` and `inayos-keyring` packages, in its local repository.

## Status

The profile was written against archiso 92's `releng`, and every added package exists in the Arch repositories. Images built from it boot and install in the Basics scenarios of `test/vm/README.md`. The Sway session that starts the installer's screens, and the screens' run-time libraries, have not run on a real guest.
