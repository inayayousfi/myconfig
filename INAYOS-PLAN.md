# InayOS plan

This file is the plan for turning this repository into InayOS: an Arch Linux distribution made of plain Arch packages, this repository's configuration, and its own installer on a USB stick. It records every decision taken so far, the facts behind them, the open checks, and the order of the work. Nothing in it is built yet.

## Principles

- **Pure and stable underneath.** The system under the daily apps rests on stable, documented interfaces with few moving parts: plain Arch packages, systemd unit files, configuration files, and standard tools. Breaking changes may come from the apps used every day, not from the system under them.
- **Accessibility is not negotiable.** The on-screen keyboard (Axidev OSK), dictation (Handy) and key remapping (Kanata) must work on the installed system from the first boot, the login screen included. A feature that endangers them does not ship.
- **Everything a virtual machine can test goes in.** There is no first version and later version. A feature is in the plan when a VM scenario can prove it. Real hardware cannot be tested this way, so the real firmware, TPM, GPU and disk stay outside what the tests prove.
- **Every check before every reboot.** Anything that can stop the machine from booting is checked while the system is still running, and a working way back always exists.

## Name

InayOS names the whole project: the repository, the command (`inayos`), the crates, and every profile, the Windows, Arch WSL and Ubuntu Devbox ones included.

`/etc/os-release` keeps `ID=arch`, so tools that check for Arch keep working. `NAME`, `PRETTY_NAME` and `VARIANT` say InayOS.

## Base system

- **Plain Arch repositories only.** No CachyOS repository and no CachyOS package.
  - On the current CachyOS laptop, 1,049 of 1,408 installed packages were CachyOS rebuilds of Arch packages. CachyOS repositories are listed before Arch's in `/etc/pacman.conf`, so their copy wins.
  - CachyOS gives some packages a higher epoch, so they win even when older than Arch's. `mesa` 26.2.6 won over Arch's 26.2.7, and `glibc` and `vulkan-radeon` lagged the same way.
  - CachyOS also ships AUR programs in its own repository (`vscodium`, `paru`, `oh-my-zsh-git`, `zsh-theme-powerlevel10k`, `helium-browser-bin`). paru prefers the repository copy, so AUR updates did not arrive until CachyOS repackaged them. InayOS takes them from the AUR.
- **Kernel:** Arch's stock `linux`. No `linux-lts`: the fallback is the snapshot entry described under "Boot guard".
- **Settings taken over from `cachyos-settings`**, each owned by an InayOS module:
  - zram swap: zstd, as large as RAM, swap priority 100.
  - A swap file on the ext4 home partition, as large as RAM, used only to hibernate. It is built with hibernation in step 6, because its resume options depend on the boot chain and encryption.
  - zswap turned off at boot by a `tmpfiles.d` rule, because zswap in front of zram works against it.
  - NetworkManager hands DNS to `systemd-resolved` (`dns=systemd-resolved`), with `systemd-resolvconf` installed and `/etc/resolv.conf` linked to resolved's stub file by a `tmpfiles.d` rule, which works the same inside the installer and on a running system.
  - `vm.swappiness = 100` and `vm.page-cluster = 0`, the two values zram's documentation recommends.
  - The higher open-file limit (`DefaultLimitNOFILE=2048:2097152` for system and user managers). `fs.file-max` keeps the kernel default, which is already larger above about 20 GB of RAM.
  - A quiet boot console (`kernel.printk = 3 3 3 3`).
  - The `ntsync` module loaded at boot, for Wine and Proton.
- **Left at the defaults:** service start and stop timeouts, journal size and coredump retention, the SysRq mask, and the time servers.
- **Dropped:** the dirty-page ratios, `vfs_cache_pressure`, the watchdog blacklist, the I/O scheduler, write-back, SATA, HPET and CPU latency udev rules, audio power saving, the `rtkit` override, huge-page tuning, automatic Wi-Fi region, the NVIDIA, old Radeon, X11 touchpad and GNOME files, and CachyOS's helper scripts.

## Disk layout

The default layout on a whole disk:

| Partition | Filesystem | Content |
| --- | --- | --- |
| EFI system partition, 2 GB | FAT32 | rEFInd, its drivers, and the signed boot images when the system is encrypted |
| System, 150 GB by default | Btrfs | The system, snapshotted by snapper |
| Home, the rest | ext4 | `/home` and the hibernation swap file |

- The system size is a default the disk screen lets the user change with a slider and presets.
- The Btrfs partition has separate subvolumes for the root, the snapshots, `/var/cache`, `/var/log` and `/var/lib/docker`. The last three stay out of the snapshots and stop container images and logs from filling them.
- Without encryption, `/boot` lives inside the Btrfs root subvolume. rEFInd reads it with its Btrfs driver, so every snapshot contains the kernel that matches its modules.
- Home on its own ext4 partition lets a reinstall format the system and leave home untouched.

### Disk choices in the installer

1. **Wipe one whole disk** with the default layout.
2. **Install next to Windows.** The installer shrinks the Windows NTFS partition and adds InayOS beside it. It stops with instructions when BitLocker is on or when Windows Fast Startup left the NTFS partition marked dirty. Windows starts through its own firmware boot entry, never from inside rEFInd (see "Firmware entries").
3. **Reinstall, keeping home.** The installer formats the system partition and keeps the ext4 home partition and its files. It deletes the InayOS state record in the kept home, because that record describes the system that was just erased.
4. **Shrink an existing ext4 partition and keep it as home.** For a machine whose single ext4 partition holds both the system and home (the current laptop):
   1. Shrink the ext4 partition from the USB stick, unmounted, by the system size.
   2. Create the Btrfs system partition in the freed space at the end of the disk and install there.
   3. Inside the old partition, move the home folder to its top level and delete the old system folders. A move inside one filesystem renames entries and copies nothing.
   4. Mount that partition as `/home`.

   The final layout is the EFI partition, then the old partition as home, then the Btrfs system. Any other order leaves free space in front of home, and ext4 cannot grow backwards.

## Boot chain

- **Boot manager: rEFInd**, on every InayOS machine. It lets the user pick an entry with the mouse, which the accessibility principle requires.
- **Boot screen: bootvine**, from the official package, with the Black and Pink theme and console palette.
- **Initramfs:** mkinitcpio with the systemd hooks.

### Secure Boot (installer option)

- Separate from encryption. When on:
  - `sbctl` creates the machine's own keys and enrolls them with Microsoft's keys and the firmware's built-in keys. The Microsoft keys keep Windows and option ROMs booting.
  - `sbctl` signs rEFInd, its Btrfs driver, every kernel (the kernels inside snapshots included) and every unified kernel image. rEFInd and its driver are registered with `sbctl sign -s -o <path on the EFI partition> <source in /usr/share/refind>`, so a rEFInd package update signs the copies on the EFI partition again.
  - sbctl's own pacman hook (`zz-sbctl.hook`, from the `extra` repository) runs `sbctl sign-all` after every transaction that touches a kernel or an EFI file, and the guard checks the result.
  - Any other EFI tool rEFInd shows, such as fwupd's `fwupx64.efi`, is signed too or hidden with `dont_scan_tools`.
  - Without encryption, the initramfs is a plain unsigned file next to the kernel, so Secure Boot alone does not protect it. Only the encrypted layout, whose unified kernel image holds the initramfs, covers it.
- Easy Anti-Cheat Linux v2 (announced by Epic in October 2026) requires Secure Boot, a TPM 2.0 measured boot, a kernel signed by a key Epic trusts, lockdown in integrity mode, signed modules only, IOMMU on, kexec off and the BPF security module. Epic says these requirements may change.
  - Keys enrolled by the user are not accepted: CachyOS's guide says EAC rejects anything rEFInd starts, because rEFInd is signed with the user's own key.
  - Arch does not sign its kernel, so no Arch kernel meets the key requirement.
  - An entry running in lockdown integrity mode loses hibernation and locally built (DKMS) modules. Arch's stock kernel (`linux` 7.3.4.arch1-2) does not turn lockdown on under Secure Boot (`CONFIG_LOCK_DOWN_KERNEL_FORCE_NONE=y`), so normal InayOS entries keep hibernation with Secure Boot on.
- InayOS stays ready without installing anything for it:
  - Secure Boot with the Microsoft and built-in keys.
  - TPM 2.0 and IOMMU checked by the installer.
  - Room on the EFI partition for a second boot manager and kernel.
  - Firmware entries started through `BootNext` (below).
  - Games that need it are expected to run on a Windows dual boot.

### Encryption (installer option)

- Offered only when the machine has a TPM 2.0. The disk is never unlocked with a typed password in normal use.
- The system and home partitions are LUKS2 volumes, enrolled with `systemd-cryptenroll` against the TPM, bound to measurement register 7 (the Secure Boot state).
- Register 7 changes when Secure Boot is turned on or off, when PK, KEK, db or dbx change (fwupd updates, and Windows Update on a dual-boot machine, which can write db and dbx through Microsoft's KEK), and when a binary is approved by a different db certificate, so a new signing key changes it. Kernel updates signed with the same key do not change it.
- With encryption on, rEFInd cannot read the system partition. The kernel then goes on the EFI partition as a unified kernel image built by mkinitcpio: kernel, initramfs and kernel options in one signed file.
- Hibernation with encryption needs the home volume unlocked in the initramfs, because the swap file lives there.
- **Recovery key.** `systemd-cryptenroll` generates one. The TPM can refuse to unlock after a Secure Boot database update the guard did not see (Windows Update on a dual-boot machine), after a firmware update that resets the keys, after the TPM is cleared, or when the disk moves to another machine. bootvine answers every systemd password request, so its screen shows systemd-cryptsetup's "passphrase or recovery key" prompt. The installer hands the key over two ways:
  - A file in the new user's home folder. Inside an encrypted disk it is useless when it is needed, so the first login shows a reminder until the user has copied it into a password manager and deleted it.
  - A QR code on the final screen, read with a phone's camera app and copied into the password manager on the phone.

### Firmware entries

- Windows and any future anti-cheat Linux entry start through their own firmware boot entries. rEFInd sets `BootNext` and reboots, and the firmware starts the entry directly.
- That keeps rEFInd's signature out of the TPM's record of that boot:
  - BitLocker does not ask for its recovery key.
  - An anti-cheat check sees a boot rEFInd never touched.
- rEFInd's `scanfor ...,firmware` adds a "Reboot to <label>" entry for each firmware boot entry. A manual stanza with `firmware_bootnum <hex>` adds one chosen entry. Either way, picking it writes `BootNext` and resets the machine, and rEFInd is not in that boot path.
- The rEFInd configuration sets `enable_mouse`, uses `dont_scan_firmware` (or one `firmware_bootnum` stanza) to show only the wanted firmware entries, and `dont_scan_files EFI/Microsoft/Boot/bootmgfw.efi` so rEFInd adds no chain-loading Windows entry of its own.

### Hibernation lock

- Booting another system that writes to a hibernated InayOS disk loses data.
- Before hibernating, systemd writes the `HibernateLocation` firmware variable and clears it on resume. Every program that can boot on the machine sees it.
- The installer, the repair workspace and any future second Linux entry check that variable and the swap file's hibernation signature first. When either says the disk is hibernated, they mount nothing read-write and tell the user to start InayOS and shut it down properly.

## Boot guard

The guard is a small binary, separate from `inayos`, shipped in the same package. A kernel update must never depend on the large program working. It has no interface, no network access and few dependencies.

- **At each kernel or boot update**, from a pacman hook:
  1. snapper takes a snapshot before the transaction;
  2. the new boot files are built;
  3. the guard checks that every file rEFInd will start is signed when Secure Boot is on, that each boot image contains the modules of its kernel, and that the kernel options point at disks that exist;
  4. only then does rEFInd see the new entry.

  When a check fails, the previous entry stays the default and the user gets a desktop notification.

- **"Previous system" entry.** The guard writes a rEFInd entry that boots the last pre-update snapshot: its kernel, modules and system.
  - Snapshots are read-only, so the entry uses systemd's own overlay: mkinitcpio's `sd-volatile` hook is in the hooks from installation on, and the entry passes `systemd.volatile=overlay rootflags=subvol=<snapshot>`. `grub-btrfs` is not used: its hook pulls in GRUB and does not run in a systemd initramfs.
  - With encryption, the guard builds and signs a previous unified kernel image with the snapshot's kernel and that command line.
  - rEFInd does not fall back by itself: when a new kernel fails to boot, the user clicks this entry.
  - The `broken-kernel` scenario showed it working: the entry boots a read-only snapshot on systemd's overlay, with `/var/log` and `/home` mounted.
- **Lockdown check.** The guard checks that `/sys/kernel/security/lockdown` reads `[none]` on normal entries, which catches a future Arch kernel that would disable hibernation.
- **Firmware updates.** Firmware updates go through InayOS (`fwupd`). Before a firmware or Secure Boot database update, the guard adds a temporary TPM slot with no measurement registers. At the next good boot it enrolls register 7 again and removes the temporary slot. During that window the disk unlocks without the Secure Boot check. Routine updates never need the recovery key.

## Snapshots

- snapper manages the Btrfs system partition, with a snapshot before and after each pacman transaction.
- A cleanup keeps a fixed number of snapshots so the system partition never fills.
- A rollback runs from the USB repair workspace and restores the kernel with the system.

## Maintenance

InayOS automates the chores Arch leaves to the user:

- unread Arch news that announces a manual step shows before an update;
- a notification lists `.pacnew` files, with a diff tool to merge them;
- the snapshot cleanup above;
- a timer ranks the mirrors.

Arch already ships the timer that refreshes its keyring.

## Delivery and updates

- InayOS ships from its own pacman repository, hosted on GitHub Releases and signed with an InayOS key, so `pacman -Syu` updates InayOS with the rest of the system.
- Packages:
  - `inayos`: the main binary, the guard, and their pacman hooks.
  - `inayos-keyring`: installs the repository's public key.
- The private signing key lives in a GitHub Actions secret for release signing, with a copy in the user's 1Password vault. The key is generated, stored with the 1Password command-line tool (`op`) and set with `gh secret set`, without passing through an agent's conversation.
- The Windows, Arch WSL and Ubuntu Devbox profiles keep their current `install.sh` and `install.ps1` downloads.

## The USB stick

- Built with `mkarchiso`, Arch's official image tool, from a profile folder in this repository, driven by the VM xtask.
- The installer needs a network connection. Packages always come from the Arch mirrors, so the image stays near 2 GB.
- The live session is Sway, which runs Axidev OSK through layer-shell and uinput. Axidev OSK needs Python, PySide6, Qt and LayerShellQt on the image.
- Workspace 1 runs the installer. Workspace 2 is the repair workspace:
  - Ghostty and the on-screen keyboard;
  - disk recovery: `testdisk`, `photorec`, `ddrescue`, `smartctl`;
  - boot repair: `efibootmgr`, `sbctl`, rEFInd, and `mkinitcpio` through a chroot;
  - snapshot rollback: snapper and a guided rollback that restores the kernel too;
  - a browser for the Arch wiki.
- Not on the USB stick: Kanata and Handy dictation. Dictation does not fit sizes, usernames or passwords, and the installer uses the plain keyboard layout.

## The installer

- **Two layers.**
  - A core crate holds all disk and install logic and reads an answers file: keyboard, Wi-Fi, disk choice, sizes, Secure Boot, encryption, user and modules. It has no interface.
  - The interface is an Iced application. Iced has a CPU renderer for machines and VMs without Vulkan. The screens only fill the answers file.
  - The installer binary runs from the USB stick and is never installed. Its settings are built into it.
- **Screens:**
  1. Keyboard and language, applied at once, with a test field.
  2. Wi-Fi, skipped when a cable works.
  3. Disk: each disk with its model and size, then the four disk choices.
  4. Secure Boot and encryption, as two separate options. Encryption is shown only with a TPM 2.0.
  5. User: name, username suggested from the name, password, computer name. The time zone comes from a searchable list that starts at the keyboard's country. Nothing asks an outside service for it.
  6. Modules: a checkbox per module of the InayOS profile, with the base modules locked on. The current Ratatui screen already lists modules with checkboxes.
  7. Summary: the exact disk actions in plain words, the disk that gets erased, and a typed confirmation.
  8. Progress with a live log, then the recovery key handoff when encryption is on, then reboot.
- **Typing as little as possible.** Sizes come from sliders and presets, the filesystems are fixed by the layout, click targets are large, and the on-screen keyboard is visible from the first screen. The password is the only field that needs real typing.
- **Install steps:**
  1. Partition and format.
  2. `pacstrap`.
  3. Create the user.
  4. Run `inayos install` as that user inside the new system, in installer mode: `arch-chroot -S -u <user> /mnt env SYSTEMD_OFFLINE=1 inayos install ...`.
  5. Set up the parts of the boot chain that no module owns: Secure Boot keys, encryption enrollment and the guard. The refind module installs rEFInd onto the EFI partition and writes `/boot/refind_linux.conf` during step 4, and in installer mode refind and bootvine fail when the EFI partition or that file is missing.
  6. Run the guard's checks.
  7. Reboot.

  No service runs at first login to finish the install.
  - `arch-chroot` version 32 bind-mounts the live system's `/run` into the new system by default. Commands inside could then talk to the live session's systemd and D-Bus, and the Plasma steps could change the installer's own desktop.
  - `-S` runs the chroot through `systemd-run` and binds `/run/udev` read-only. `-u` switches to the user with no PAM login session and a cleared environment, so no `XDG_RUNTIME_DIR` or `DBUS_SESSION_BUS_ADDRESS` reaches the run. Installer mode never sets them.
  - `-N` (unshare mode) is not used: it needs a subordinate id range for root, and it maps the user's id into that range, so files in the new home would get the wrong owner on disk.

- **Logins** (GitHub, Tailscale and any other) are left to the user after the install. The `login` module does not run from the installer.

## Installer mode in `Context`

CachyOS's installer (Calamares, configuration in the CachyOS `calamares-config` repository) follows one rule: at install time it only writes files and enables services. Services start at first boot. The desktop configuration is a set of files in `/etc/skel` copied into the new home. InayOS follows the same rule inside `Context`, so most modules need no change:

- `Context` sets `SYSTEMD_OFFLINE=1` for every command. Inside the new system, run as the user, `systemctl --user enable` then creates the links in `~/.config/systemd/user` with no running user manager. `systemctl --user --root=<dir>` from outside does not work (systemd 262 refuses user scope with `--root`).
- `enable_service` enables without starting.
- Starting, stopping, restarting, reloading the user manager, "is it active" and `active_group` report a deferred result. `service_state` reports a service as not active.
- Modules stop calling runtime `systemctl` commands directly and go through `Context`. A repository test forbids raw runtime `systemctl` calls in modules.
- `verify` checks that services are enabled, not running. `inayos verify --installer-mode` runs it inside the new system before its first boot.
- In installer mode the user name and home come from the passwd entry of the running user, not from the environment, which `arch-chroot -u` clears.
- `ctx.confirm` answers come from the installer's answers file.
- Commands that exit 0 without doing their work need explicit handling, because the exit status hides the failure:
  - `gsettings set` without a session bus saves nothing. It runs under `dbus-run-session`, with `XDG_CONFIG_HOME` pointing into the new user's home.
  - `plasma-apply-colorscheme --accent-color` rebuilds light Breeze colors unless it sees Breeze Dark, which only a Plasma session puts in `XDG_CONFIG_DIRS`. The `plasma-apply-*` tools run with `XDG_CONFIG_DIRS=$HOME/.config/kdedefaults:/etc/xdg`.

The current profile, run inside a never-booted system, gives:

| Module | Result in installer mode | Fix |
| --- | --- | --- |
| plasma-version, base, media, cli, runtimes, flatpak, appimage, zsh, terminal-tools, ghostty, agents-packages, android-phone, vscodium, chrome, math, refind, bootvine, printing, t3code-desktop, git-config, environment-inventory | Works | None |
| ssh | Refuses because `/run/systemd/system` is missing | Drop that check in installer mode |
| kanata, handy, agent-config | `modprobe uinput` targets the USB stick's running kernel, and `systemctl --user daemon-reload` fails | Skip `modprobe`, since the boot-time `modules-load.d` file covers it, and use the deferred `Context` calls |
| kanata-kde, pipewire | User manager calls fail | Deferred `Context` calls |
| kde-plasma | `systemctl --user daemon-reload` fails, `gsettings` saves nothing without a session bus, and the accent color rebuilds light Breeze colors | Deferred `Context` calls, `gsettings` under `dbus-run-session`, `plasma-apply-*` with the `kdedefaults` folder in `XDG_CONFIG_DIRS`, the KWin reconfigure and layout run skipped, and the panel layout as Plasma's default layout |
| t3code-desktop, and the T3 Code server module if the InayOS profile takes it | The server module calls `loginctl` and `is-active`; `setting/plasma_panels.rs` calls plasmashell | To check when the InayOS profile list is set |
| docker | Start, `is-active` and `docker info` need the running daemon | Enable and set the Docker contexts at install; the daemon starts at first boot |
| tailscale | Setting the operator needs `tailscaled` running | Enable at install; verify reports the operator as waiting for the user's Tailscale login, not as a failure |
| axidev-osk | Its login-screen setup asks the running systemd whether Plasma Login exists, and its permission setup runs `modprobe uinput` | Fix in the `axidev-osk` repository: check the unit file instead, and rely on the boot-time module file |
| login | A login | Left to the user |
| cachyos-setup | `lsblk` as a normal user sees no filesystems and would remove `btrfs-progs` | Module deleted with the CachyOS profile |

The Axidev fix is required: the on-screen keyboard must appear on the login screen at first boot.

The panel layout ships as the default layout of an InayOS look-and-feel package:

- When plasmashell starts with no containments in `plasma-org.kde.plasma.desktop-appletsrc`, it creates one desktop per screen and runs `contents/layouts/org.kde.plasma.desktop-layout.js` of the look-and-feel package named in `kdeglobals` `[KDE] LookAndFeelPackage`, through the same scripting engine as `evaluateScript`. Every global `layout.js` uses exists there, and its own two variables fall back to a first run, which builds the bars, the dock and its default pins (plasma-workspace 6.8.3 sources).
- The package holds `metadata.json`, the layout script, and a copy of Breeze Dark's `contents/defaults`, because a package without them falls back to light Breeze. The kde-plasma module applies it, so `LookAndFeelPackage` names it before the first login.
- The installer never creates `plasma-org.kde.plasma.desktop-appletsrc`: a file with containments skips the default layout.
- `print()` only reaches Plasma's debug log there. When a widget is missing or the script fails, the default-layout version falls back to `loadTemplate("org.kde.plasma.desktop.defaultPanel")`, so the user never gets a desktop with no panels.
- The `blank-disk` scenario showed it working: at the first login after an install, the default layout built the bars and the dock.
- The layout command and its user service stay for displays plugged in later: Plasma adds desktops, not panels, for a new display.

## Repository changes

1. **Rename to InayOS first**, before new work, so all new code uses the new names. Names that already exist on installed machines migrate:
   - the state record moves from `~/.local/state/myconfig/state.json` (and `%LOCALAPPDATA%\myconfig`), with a `format` increase and a migration;
   - the layout script recognizes the old `myconfig*` panel markers, so existing docks keep their pins;
   - files installed under the old names (units such as `myconfig-kanata.service`, the `myconfig-*` scripts, `/etc/libinput/plugins/90-myconfig-*.lua`, APT sources, Windows Startup shortcuts, the rEFInd block) stay in place next to the new ones after an install. Left alone, they double the pointer speed (both libinput plugins multiply by 4), break `apt-get update` on the Ubuntu Devbox (two sources with different `Signed-By`), and run Kanata, Handy, the tray icons and AutoHotkey twice. Install never undoes an earlier install, so a state migration (format 3) puts every pending record that names an old file into a `myconfig-leftovers` group. One `inayos remove myconfig-leftovers`, run before the first install of the renamed version, undoes only those records, the old stow links included;
   - the release file names change to `inayos-*`. The latest release only has `myconfig-*` files, so the merge into `main` and a new release happen together;
   - the `install.sh` and `install.ps1` download addresses and the GitHub repository name change last. GitHub redirects the old repository address to the new one, not the reverse.
2. **New apps:**
   - the InayOS profile (plain Arch desktop);
   - the installer;
   - the guard.

   The `mkarchiso` profile folder sits next to `test/vm`.

3. **Deleted on this branch:** the CachyOS profile and `cachyos-setup`. `main` keeps them for the current laptop until it moves.
4. **New modules**, each owning its files:
   - system tuning: zram, zswap off, resolved DNS, the sysctl values, the limits, `ntsync`;
   - Secure Boot;
   - encryption;
   - snapshots;
   - the guard and its hooks;
   - maintenance;
   - the InayOS repository and keyring;
   - games.
5. **Installer mode in `Context`**, as described above.
6. **Documents:** `README.md`, `AGENTS.md`, `MODULES-DESIGN.md` and `docs/` follow the rename. Each new module gets its document.

## Tests

- All scenarios run by hand on the local machine with `cargo xtask vm`, like the current cycles.
- Each scenario feeds an answers file to the installer and boots the result. The screens get screenshot checks through the VM's control channel.
- The Windows 11 guest already has Secure Boot firmware and a TPM 2.0 from `swtpm`, but it has never run end to end. Making it run is the first step of the Windows group.

Scenarios:

- **Basics**
  - Blank disk with the defaults: a finished desktop, with Axidev OSK on the login screen, Handy and Kanata running, and the dock layout.
  - Reinstall keeping home: the files survive.
  - Shrink an ext4 partition and keep it as home, on a disk built like the laptop's: the home files survive and the system boots.
- **Boot chain**
  - Secure Boot on: every signature checked, the machine boots.
  - A deliberately broken kernel update: the guard keeps the old entry and notifies.
  - The "Previous system" entry boots the pre-update snapshot.
  - A firmware Secure Boot database update: the TPM still unlocks.
- **Encryption and hibernation**
  - TPM unlock with no password.
  - A wiped TPM: the recovery prompt shows through bootvine, and the recovery key unlocks.
  - Hibernate and resume, with and without encryption.
  - The USB stick refuses to write to a hibernated disk.
- **Windows and life cycle**
  - Install next to Windows 11: both boot, and Windows starts through its firmware entry.
  - BitLocker on, and Fast Startup on: the installer stops with instructions.
  - `pacman -Syu` from a test copy of the InayOS repository updates InayOS.
  - Arch news shows before an update.
  - The snapshot cleanup keeps its limit.
  - A rollback from the repair workspace boots the earlier system with its kernel.

## Checks before building

These are unknowns that sources and local tests could not settle. Each needs a VM, and its result can change the design.

- That a mouse click on a rEFInd firmware entry starts it through `BootNext`, and that the pointer works in rEFInd on the test firmware.
- That rootless Docker can be enabled in installer mode and starts at first boot.
- That sd-encrypt's console password agent does not disturb bootvine's screen when the recovery prompt shows.

## Proposed order of work

1. Rename to InayOS, with the migrations.
2. Installer mode in `Context`, the InayOS profile, the new system modules, and the Axidev fix.
3. The installer core with answers files, the `mkarchiso` image, and the Basics scenarios.
4. The Iced screens.
5. The guard, snapshots, Secure Boot, and the Boot chain scenarios.
6. Encryption, hibernation, and their scenarios.
7. The Windows guest, dual boot, and the Windows scenarios.
8. The InayOS pacman repository, the keyring, the signing key, and the maintenance chores.
9. Moving the laptop, later: with a backup of what cannot be lost, the "shrink an existing ext4 partition and keep it as home" choice.

## Outside this plan

- An anti-cheat Linux boot entry. InayOS keeps the slot ready. No kernel signed by a key Epic trusts exists for Arch.
- `linux-lts`.
- Kanata and dictation on the USB stick.
- Logins at install time.
