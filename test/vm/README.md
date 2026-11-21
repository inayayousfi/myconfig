# VM xtask

This document describes the VM xtask in `test/vm/`, the `inayos-xtask` crate. It builds the installers from the working tree and tests them in throwaway QEMU virtual machines of Ubuntu 26.04 LTS and Windows 11. It also builds the InayOS USB stick image in an Arch Linux virtual machine, and runs the InayOS installer from that image in scenarios. A commit or a release is not needed. It runs on a Linux host and only by hand.

The Ubuntu cycle has passed on a real guest, image build included. Nothing has run on Windows: not the image build, the cycle, the Arch WSL checks or the Windows scenarios. The Arch image build with its glibc check, the USB stick build, the three Basics scenarios and the `gui-install` scenario have passed on a real guest, with the `last-pass` copy of screenshots, and so have the kept installed disk and kept encrypted disk and every other Arch scenario.

## Commands

The cargo alias in `.cargo/config.toml` runs the crate as `cargo xtask`.

```bash
cargo xtask vm image build <os>    # builds the clean image of ubuntu, windows or arch
cargo xtask vm cycle <os>          # tests the installer of ubuntu or windows
cargo xtask vm run <os>            # boots a throwaway copy with the fresh installer in it
cargo xtask vm usb build           # builds the InayOS USB stick image
cargo xtask vm scenario <name>     # installs InayOS from that image and checks the result
cargo xtask vm run <os> --failed   # boots the kept disk of the last failed cycle
cargo xtask vm run <os> --resume   # boots the throwaway disk of the previous run again
```

`--watch` opens a `remote-viewer` window on the guest display for any command. Without it, the guest has no window, and `run` prints the viewer command.

Only one command that starts a VM runs at a time. A lock file in the state folder enforces it.

## Host

Each command first checks that the host has what the chosen system needs. Every system needs the Arch packages `qemu-desktop`, `edk2-ovmf`, `virt-viewer`, `openssh`, `curl` and `libisoburn`, the Rust target `x86_64-unknown-linux-musl`, and read and write access to `/dev/kvm`. Arch also needs the Rust target `x86_64-unknown-linux-gnu`, `readelf` from `binutils`, and `makepkg`, `repo-add`, `fakeroot` and `gpg`, which build the local InayOS repository on the host. The scenarios that build packages also need `makepkg` from `pacman`, and those with a test repository also need `repo-add` from `pacman` and `gpg` from `gnupg`. Windows also needs `swtpm`, the `cargo-xwin` command with the `clang`, `lld` and `llvm` packages, the Rust target `x86_64-pc-windows-msvc`, and nested virtualization in the `kvm_amd` or `kvm_intel` module. When something is missing, the command lists it with the commands that install it, and stops. It never installs anything.

`cargo xwin` downloads the Microsoft C runtime and Windows SDK into its own cache folder under `~/.cache` the first time it builds, which is about 1 GB.

The xtask starts QEMU directly, without libvirt. QEMU listens for control commands on a unix socket with QMP, the QEMU Machine Protocol, which the xtask uses for screenshots and key presses. The guest display is SPICE on a unix socket.

## Folders

The state folder is `${XDG_DATA_HOME:-~/.local/share}/inayos/vm`, or `INAYOS_VM_STATE_DIR`. It holds the SSH key, the lock file, the sockets and logs of the running VM, and one folder per system with these layers:

- `image` is the clean image that every throwaway disk starts from.

- `work` is the throwaway layer of the current cycle or `run`, kept until the next cycle or `run` without `--resume`.

- `failed` is the layer of the last failed cycle or USB stick build, kept until the next one of that system.

The state folder also holds the USB stick image at `usb/inayos.iso`, and a folder per scenario under `scenario/` with the same `work` and `failed` layers and a `last-pass` folder with the screenshots of the last passed run. `scenario/installed` holds the installed disk that the last passed `blank-disk` run kept. `scenario/installed-encrypted` holds the one that the last passed `encrypted` run kept, with its TPM state and the recovery key of its volumes in `recovery-key.txt`. Other scenarios start from these kept disks.

A layer holds a qcow2 disk that reads unchanged blocks from the clean image disk, a copy of the UEFI variables, and the TPM state of a guest with a TPM. A layer over a guest with the Secure Boot firmware copies its TPM state, because that state holds what the TPM sealed.

The download folder is `${XDG_CACHE_HOME:-~/.cache}/inayos/vm`, or `INAYOS_VM_CACHE_DIR`. It holds the verified ISO or cloud image of each system.

## Guests

Every guest boots with UEFI firmware from `edk2-ovmf` and has 4 virtual CPUs. Linux guests have virtio disk and network devices. Ubuntu has 4 GiB of memory, and Arch guests have 8 GiB. Windows has 8 GiB, the Secure Boot firmware build, a TPM 2.0 from `swtpm`, and a SATA disk and an `e1000e` network card, which Windows drives without extra drivers. The host CPU model passes the virtualization extensions to the guest, which WSL in Windows needs.

QEMU forwards host port 2222 to the guest SSH server. Every image accepts the key at `ssh/id_ed25519` in the state folder, which the xtask creates once. Files go into the guest with `scp`; the guest has no access to the repository.

The test account is `tester` with the password `inayos`, set in `src/os.rs`.

## Images

`image build` finds the current release at its source, downloads it, checks its SHA-256, installs it unattended, logs in once over SSH, and records the release in the image's `release` file. The new image replaces the old one only when it is complete, and the throwaway and failed layers of that system go with the old image.

- Ubuntu comes from the official 26.04 LTS cloud image, set up by cloud-init from a NoCloud seed disc.

- Arch Linux comes from Arch's official cloud image in `https://geo.mirror.pkgbuild.com/images/latest/`, set up by cloud-init from a NoCloud seed disc. The build downloads the versioned file of the newest image in that folder listing, with its `.SHA256` file. Its image only hosts the USB stick build: `cycle arch` and `run arch` refuse and point at the scenarios.

- Windows comes from Microsoft's consumer ISO, downloaded with the requests that the Mido script uses. When Microsoft refuses to give a download link to the host's network address, the build stops and names the page to download the English x64 ISO from by hand. A file saved into the `windows` folder of the download folder is used when its SHA-256 matches the one on that page. An answer file on a seed disc installs Windows 11 Pro on a 256 GB GPT disk: a 300 MB EFI system partition, the Microsoft reserved partition and NTFS `C:` over the rest. The disk leaves room beside Windows for the InayOS installer's default 150 GiB system, and the EFI partition has room for rEFInd beside Windows' boot files. The answer file turns Setup's Secure Boot check off, because the variable store has no keys, and sets `PreventDeviceEncryption` before the first logon. It creates a local account that logs on automatically and runs a setup script at the first logon. The script turns hibernation and Fast Startup off, decrypts `C:` when device encryption started anyway, installs the OpenSSH server from Windows Update with up to five attempts, and adds the key. A failure of that setup writes the failure line to the first serial port, which stops the build.

The release file holds the Ubuntu cloud image build serial, the Arch cloud image build version, or the Windows version with its build number.

## Cycle

`cycle <os>` runs these steps on a new throwaway layer:

1. It warns when the image's source lists a newer release than the image, or says that it could not check, then continues.

2. It builds the installers from the working tree: `inayos-ubuntu-devbox` for `x86_64-unknown-linux-musl`, and for Windows `inayos-windows` with `cargo xwin` plus `inayos-arch-wsl`.

3. It boots the guest, copies the installers in, and prints the number of pending package updates. On Ubuntu it refreshes a temporary copy of the package lists and counts with `apt list --upgradable` against it, so the guest's own package lists stay unchanged.

4. It saves the `before` state lists.

5. It runs `install`, then `verify`, and saves the `first-install` lists.

6. It runs `install` again and saves the `second-install` lists. Any difference from the first install fails the cycle.

7. It reads the module names with `list` and runs one `remove` with all of them, so the installer orders the removal.

8. It saves the `after` lists. Any difference from `before` fails the cycle.

On Windows, the cycle runs only the Windows installer through these steps. Its arch-wsl module creates the Arch WSL distribution on the first install, configures the same distribution again on later installs, and runs the Arch WSL installer in it on every install. The Windows installer runs with `INAYOS_ARCH_WSL_BINARY` set to a hook script, `lists/arch-wsl-hook.sh`. The hook saves the state lists of the distribution, then runs the fresh Arch WSL installer. The cycle also saves the state lists inside the distribution after each install and compares them. Before the Windows removal, it runs one Arch WSL `remove` with every Arch WSL module inside the distribution, and compares the lists after it with the lists the hook saved during the first install, before the Arch WSL installer first ran.

A passed cycle deletes its layer. A failed cycle saves the guest screen, shuts the guest down and keeps the layer as `failed`, with the state lists, the comparisons and the log of every installer command in it.

## USB stick

`usb build` boots a throwaway layer over the clean Arch image and builds `inayos-installer` for `x86_64-unknown-linux-gnu`, and `inayos-arch-desktop`, the `inayos-guard` binary of the `inayos-guard-app` package and the `inayos-news` binary of the `inayos-news-app` package for `x86_64-unknown-linux-musl`, from the working tree. The installer links to the system C library, because winit, under its Iced screens, loads the Wayland libraries at run time, which a static binary cannot do.

On the host, `test/vm/src/packages.rs` then builds the local InayOS repository: `makepkg` packages `packaging/inayos` with the three musl binaries, through `INAYOS_BINARY_DIR`, and `packaging/inayos-keyring`, and `repo-add` indexes both, unsigned, in `packages/repo` of the layer. While the committed keyring is the placeholder, a throwaway key made with `gpg` in the layer takes its place, through `INAYOS_KEYRING_DIR`, so the installed system trusts that key. The packages build with `CARCH=x86_64` and makepkg's folders in the layer, so the host's `makepkg.conf` and the working tree stay out of it. `docs/repository.md` describes the packages.

The xtask copies the installer, the InayOS desktop installer and the repository into the guest with the profile overlay in `test/usb/profile/` and the script `usb/build-iso.sh`, which runs as root:

1. It upgrades the guest and installs `archiso`.

2. It copies archiso's `releng` profile from `/usr/share/archiso/configs/releng`, then copies the overlay over it. Every overlay file replaces or adds the file at the same path, except `packages.x86_64`, whose lines are appended to releng's list.

3. It places the installer at `airootfs/usr/local/bin/inayos-installer` and the InayOS desktop installer at `airootfs/usr/local/lib/inayos/inayos`, with mode 755, and the repository at `airootfs/usr/local/lib/inayos/repo`. The installer lists the profile's modules with the desktop installer and installs the new system's InayOS packages from the repository.

4. It extracts the latest Axidev OSK release into `airootfs/opt/`, which gives `/opt/axidev-osk/bin/axidev-osk`.

5. It runs `mkarchiso`.

After the script, the xtask checks that the installer runs with the guest's C library: the newest `GLIBC_` symbol version that `readelf --version-info` lists for it must not be newer than the version that `ldd --version` prints in the guest. The script upgraded the guest from the same mirrors as the image's packages, so the guest has the image's glibc. A host with a newer glibc than Arch builds an installer that the check refuses.

The xtask copies the image to `usb/inayos.iso.part` in the state folder and renames it to `usb/inayos.iso`, so that file is always a complete image. A passed build deletes its layer. A failed build keeps it as the failed Arch layer, with the build log and the last screen.

## Scenarios

`scenario <name>` installs InayOS from the USB stick image. Its guest has 8 GiB of memory, UEFI without Secure Boot and without a TPM unless the scenario says otherwise, a new blank 80 GB qcow2 target disk and fresh UEFI variables in the scenario's layer. The image is a CD drive that the firmware starts first. A NoCloud seed disc labeled `CIDATA` gives root on the live system the xtask's SSH key.

The xtask builds `inayos-installer`, `inayos-arch-desktop`, `inayos-guard` and `inayos-news` from the working tree, checks the installer against the live system's glibc as `usb build` does, and puts the installer and `inayos-arch-desktop` in place of the copies on the image. It builds the local InayOS repository from the same binaries as `usb build` does, in `packages/repo` of the layer, and replaces `/usr/local/lib/inayos/repo` of the live system with it, so a scenario tests the working tree without a new image. Each binary goes to a file next to its place and a rename replaces the old one, because the Sway session of the stick runs the installer's screens from that place and Linux refuses a write to a running program. It drives the live system over SSH as root. The Basics scenarios write the answers file in `src/scenario/answers_file.rs`, copies it in, and runs `inayos-installer install --answers` with it. The answers file creates the `tester` account with the password from `src/os.rs` and the xtask's key in `authorized_keys`. It answers yes to the SSH server question and no to the GitHub and Tailscale login questions, as the answer table of the cycle does. The installer's output and its own log go to the `logs` folder of the layer.

After the install, the xtask powers the guest off and boots the target disk without the image. It checks the installed system over SSH as `tester`, which works through the SSH server that the ssh module turns on. It takes QMP screenshots.

The desktop checks are:

1. `plasmalogin.service` is active and an Axidev OSK process runs for the login screen. A screenshot of the login screen is saved.

2. The xtask types the password at the login screen through QMP, and `plasmashell` starts for `tester`. When no session starts within two minutes, it clears the password field with Ctrl+A and Backspace and types the password once more, because the first keys can arrive before the field has the focus.

3. The user services `inayos-kanata-tray`, `inayos-kanata` and `inayos-handy` are active.

4. `plasma-org.kde.plasma.desktop-appletsrc` has one panel marked as the top bar and one marked as the dock by the default layout of `dotfiles/plasma`, each with that layout's widgets. A screenshot of the desktop is saved.

The scenarios are:

- `blank-disk` wipes the target disk with the default layout, with a 40 GiB system, and runs the desktop checks. Right after the install powers the live system off, it copies the target disk and its UEFI variables to `scenario/installed.new`. When the scenario passes, that copy replaces `scenario/installed`; when it fails, the copy is deleted.

- `reinstall-keep-home` installs as `blank-disk` does, writes a marker file into the home folder of the installed system, boots the image again and reinstalls keeping home on `/dev/vda1` to `/dev/vda3`. It checks that the marker survived with its owner and that the home folder kept its uid and gid, then runs the desktop checks.

- `shrink-ext4` builds a disk like the laptop's on the live system with `scenario/laptop-disk.sh`: a 2 GiB EFI partition mounted at `/boot` that holds the kernel and the Intel microcode, and one ext4 partition with an Arch base system and test files in `/home/tester`, whose checksums it saves. That `tester` has uid and gid 1234, which a new system never picks by itself. It installs with the shrink choice and a 30 GiB system. It checks:
  - the files, and that they and the new `tester` still have uid and gid 1234;
  - that the top level of the home partition holds only `lost+found`, the hibernation `swapfile` and `tester`;
  - that the partitions on the disk are the EFI partition, then the ext4 home, then the Btrfs system;
  - that the EFI partition keeps its filesystem UUID, so it was not formatted, and that the old kernel, initramfs and microcode files are gone from it.

  Then it runs the desktop checks.

- `gui-install` installs on the blank disk through the installer's screens, with QMP key presses only, as `src/scenario/gui.rs` plans them. It is described below.

- `secure-boot` and `broken-kernel` test the boot chain. They are described below.

- `encrypted`, `tpm-wiped`, `hibernate`, `hibernate-encrypted`, `hibernated-disk` and `firmware-db-update` test encryption, hibernation and the hibernation lock. They are described below.

- `beside-windows`, `bitlocker-on` and `fast-startup-on` test the install beside Windows. They are described below.

- `pacman-update`, `arch-news`, `snapshot-cleanup`, `rollback` and `rollback-encrypted` test the updates, the maintenance chores and the rollback. They are described below.

The layer keeps the answers files, logs, serial logs of each boot and screenshots. A passed scenario copies the PNG screenshots of its layer to its `last-pass` folder, replacing the previous ones, and deletes its layer. A failed scenario saves the last screen and keeps the layer as that scenario's `failed` layer until its next run.

### The `gui-install` scenario

The installer writes the name of its current screen to `/run/inayos-installer/screen`. The scenario reads that file over SSH as root after each input, until it names the expected screen or two minutes pass. On each screen it saves a screenshot named `gui-<number>-<screen>.png`, and fails when a PPM screendump of the same screen has a single color, because nothing drew on it.

1. It waits up to five minutes until the installer of the stick, which the Sway session starts, shows its keyboard screen. The Sway session is part of the image, so this step needs an image built with the current `test/usb/profile`; when it fails, the error says whether any installer runs, and when none runs it points to `cargo xtask vm usb build`.

2. It stops that installer, puts the binaries built from the working tree in place of the stick's copies, and starts the fresh installer through `swaymsg` with the command of the Sway session. It waits for the keyboard screen again.

3. It goes through the screens with the defaults of each, on the us layout. It presses Enter on the keyboard, disk, security and modules screens. The wired network of the guest is connected, so the Wi-Fi screen does not show. On the user screen it types the full name `Tester`, whose suggested username is `tester`, then Tab twice and the password, then Tab and the password again, then Enter. On the summary screen it types `erase` and presses Enter. The plan relies on each screen giving its first field the focus when it opens.

4. On the progress screen, a `tail -F` over SSH copies the installer's log to `logs/installer-gui.log` in the layer as it grows. The scenario waits up to 90 minutes for the done or the failed screen, then copies the whole log to `logs/installer-gui-file.log`. The failed screen fails the scenario.

5. The screens take no SSH key, so the scenario mounts the partition labeled `inayos-home` and adds the xtask's key to `~/.ssh/authorized_keys` of `tester`, with the owner of the home folder and mode 600. This is the one change to the new system that does not go through the screens.

6. It powers the live system off, boots the installed disk and runs the desktop checks. The module questions got the installer's built-in answers, which turn the SSH server on.

### The boot chain scenarios

These scenarios check Secure Boot, snapper's snapshots of pacman transactions, and the boot guard, `inayos-guard`.

`secure-boot` boots the Secure Boot firmware build `OVMF_CODE.secboot.4m.fd`, with SMM, the secure flash and a TPM 2.0 from `swtpm`, as the Windows guest does. Its fresh variable store `OVMF_VARS.4m.fd` has no keys, so the firmware is in Setup Mode and the unsigned USB stick boots. It installs on the blank disk as `blank-disk` does, with `secure_boot = true` in the answers file, powers off and boots the installed disk. It checks:

1. The `SecureBoot` firmware variable is 1.
2. `sudo sbctl verify` lists signed files and no unsigned one.
3. The guard's status file, `/var/lib/inayos-guard/status.json`, says the last result passed.
4. rEFInd's `refind.conf` has the line `include inayos-guard.conf`. The guard's `inayos-guard.conf` next to it has a "Previous system" entry that is not the default. Its options `rootflags=subvol=@snapshots/<n>/snapshot` and `systemd.volatile=overlay` name a pre snapshot that snapper lists, its loader is in that snapshot's `/boot`, and that kernel file exists. The entry need not name the newest pre snapshot: the guard's pacman hook runs only for transactions that touch the boot files, and the modules after boot-guard install packages that take newer snapshots.

Then it runs the desktop checks.

`broken-kernel` starts from a throwaway layer over `scenario/installed`, with UEFI without Secure Boot, and installs nothing. When no installed disk is kept, it stops and says to run `blank-disk` first. A change to the installer or the modules reaches it only through a new `blank-disk` pass. A new pass replaces the kept disk, and the failed layers made over the old one can no longer boot. The scenario works over SSH as `tester`, with `sudo` reading the guest password from its input:

1. It puts the guard built from the working tree at `/usr/bin/inayos-guard`, runs `inayos-guard check-update`, and checks that the result passed.
2. It runs `scenario/break-boot.sh`. The script writes the mkinitcpio drop-in `/etc/mkinitcpio.conf.d/zz-test-break.conf`, with `HOOKS=(base)` and no modules, so mkinitcpio builds an initramfs without kernel modules and still succeeds. Then it reinstalls the installed `linux` package from the pacman cache, or from the mirror when the cache lacks it.
3. It checks that snapper lists a new pre snapshot whose description names `linux`, that the guard's result failed with failures, that `/var/lib/inayos-guard/failed` exists, and that `inayos-guard.conf` makes "Previous system" the default and boots that snapshot.
4. It powers off and boots the installed disk again. It saves a screenshot every 3 seconds for the first 24 seconds, named `refind-<number>.png`, which covers rEFInd's menu and its 20-second wait.
5. It checks that `/proc/cmdline` boots that snapshot with `systemd.volatile=overlay`, that `/` is an overlay, `/home` is ext4 and `/var/log` is Btrfs, and that `inayos-guard-boot.service` ran in this boot and exited with status 0, waiting while it still runs. SSH answering at all shows that sshd runs.

Both scenarios keep copies of what they read in the layer: the output of `sbctl verify`, the guard's status files, and `refind.conf` and `inayos-guard.conf` at each step.

### The encryption and hibernation scenarios

The guests that start from the encrypted disk or install it have a TPM 2.0 from `swtpm`. `hibernate` and `hibernated-disk` boot the plain kept disk without a TPM: with one, OVMF places a small ACPI table at another address on some boots, and the kernel then refuses to resume into the changed memory map. The scenarios that start from a kept disk work over SSH as `tester`, as `broken-kernel` does, and stop with a message naming the scenario to run first when the kept disk is missing. A change to the installer or the modules reaches them only through a new pass of the scenario that keeps their disk.

A boot counts as unlocked by the TPM when the new user accepts SSH while nothing was typed, since a password prompt stops the boot. The scenario also checks that the serial log of that boot holds neither "passphrase" nor "recovery key", and saves a screenshot named `unlocked-<boot>.png`.

The enrollment checks read `findmnt` and `sudo cryptsetup luksDump --dump-json-metadata` for each LUKS volume on the disk. `/` must come from `/dev/mapper/system` and `/home` from `/dev/mapper/home`, and the disk must hold two LUKS volumes. A finished enrollment has a `systemd-tpm2` token bound to PCR 7 alone and no token with an empty PCR list, which is the temporary slot. The layer keeps each dump as `luks-<step>-<partition>.json`.

`encrypted` boots the Secure Boot firmware in Setup Mode, as `secure-boot` does, and installs on the blank disk with `secure_boot = true` and `encryption = true`. Right after the install, it copies the target disk, the UEFI variables and the TPM state to `scenario/installed-encrypted.new`. It boots the installed disk and checks:

1. The TPM unlocked the disk.
2. `~/inayos-recovery-key.txt` has mode 600 and owner `tester`, and holds a recovery key of eight groups of eight modhex characters. No installer output or log in the layer holds the key. The key goes into `recovery-key.txt` of the copy.
3. The Secure Boot checks of `secure-boot`.
4. `inayos-guard-boot.service` ran and exited with status 0, both volumes have a finished enrollment, and `inayos-guard status` has a line that names PCR 7.

It powers off and boots again, checks the TPM unlock and the enrollment once more, and runs the desktop checks. When the scenario passes, the copy replaces `scenario/installed-encrypted`; when it fails, the copy is deleted.

`tpm-wiped` starts from the kept encrypted disk with an empty TPM state. It waits 45 seconds, past the firmware and rEFInd's wait, then until two screendumps 5 seconds apart are the same and not a single color, or 150 seconds pass. The guest must not answer SSH meanwhile. It saves the screen as `recovery-prompt-<attempt>.png` and types the kept recovery key and Enter through QMP. When SSH does not answer within three minutes, it waits and types again, up to three times, because each volume can ask for the key. Then it checks that both volumes are open.

`hibernate` starts from the plain kept disk, and `hibernate-encrypted` from the kept encrypted disk, where it first checks the TPM unlock and waits for `inayos-guard-boot.service`. Both wait until `/home/swapfile` is an active swap, start `sleep 100000.<number>` outside the SSH session, and note its pid and the boot id. They run `sudo systemctl hibernate` and wait up to ten minutes for QEMU to stop. They boot again and check that the boot id and the marker's pid are unchanged, which only a resume gives, and that the `HibernateLocation` firmware variable is gone.

`hibernated-disk` starts from the plain kept disk and hibernates as `hibernate` does. It then boots the USB stick on the same layer. The live system must see the `HibernateLocation` variable, which it keeps only when the stick's systemd does not clear it at boot. The scenario saves the SHA-256 of the first MiB of the disk and of each partition and of the last MiB of the disk, which hold the partition tables and the superblocks. A script that the scenario copies into the live system as `/root/disk-heads.sh` computes them.

It runs the installer with reinstall answers twice. The first run sees the variable. Before the second run, the scenario saves the variable to `/root/hibernate-location` and removes it, so only the hibernation signature in the header of `/home/swapfile` can refuse the disk. After the second run it writes the variable back. Each run must pass these checks:

1. The installer failed, and its output or log has a line with "This disk holds a hibernated InayOS: start it and shut it down properly first" and "the disk is unchanged", which the checks stage adds.
2. No command in the installer's log writes to a disk: partitioning, formatting, LUKS header changes, filesystem repairs and resizes, swap, a `dd` with an `of=` file, or a mount that is not read-only.
3. The checksums are unchanged, and no partition of the disk is mounted.

Then it powers off, boots the installed disk and checks that it resumed.

`firmware-db-update` starts from the kept encrypted disk. After the TPM unlock and the guard's first boot, it puts the guard built from the working tree in place and runs `check-update`, as `broken-kernel` does. It notes the keyslots bound to PCR 7, runs `sudo inayos-guard prepare-firmware-update`, and checks that each volume has a temporary token. It makes a throwaway DER certificate in `/var/lib/sbctl/keys/custom/db`, runs `sudo sbctl enroll-keys --partial db --append --custom --ignore-immutable`, which adds it to the Secure Boot database signed with the machine's KEK, and checks that the `db` variable changed. It boots again and checks:

1. The TPM unlocked the disk through the temporary slot.
2. `inayos-guard-boot.service` ran and exited with status 0.
3. Both volumes have a finished enrollment, each with a PCR 7 keyslot that was not there before.

A third boot must unlock with the TPM again and keep a finished enrollment.

### The Windows scenarios

The Windows scenarios start from a throwaway layer over the clean Windows image, with its UEFI variables and TPM state. Every boot, the USB stick's included, has the Windows guest's hardware: the Secure Boot firmware build with SMM, a TPM 2.0 from `swtpm`, a SATA disk, an `e1000e` network card and the standard VGA display. Linux names the disk `/dev/sda`. When no Windows image is built, the scenario stops and names the command that builds it.

The firmware of the Windows image has no Secure Boot keys enrolled, because the host has no tool that enrolls Microsoft's keys into an OVMF variable store. The firmware stays in Setup Mode with Secure Boot off, so the unsigned USB stick and Windows both boot, and the scenarios install with `secure_boot = false`.

Windows and InayOS accept the same key for the same user, so after each boot the scenario asks which system answered SSH: only the Windows command prompt has `ver`. Windows runs the scripts of `scenario/windows/` with PowerShell, copied into the user's home folder.

`beside-windows` boots the stick, waits until the stick's installer shows its keyboard screen, and lists the partitions of the disk, installs with the `beside-windows` answers, and lists them again. The Windows partition must keep its first sector and partition GUID and get smaller, the EFI system partition must keep its filesystem UUID, every other earlier partition must stay as it was, and a Btrfs and an ext4 partition must follow Windows. The layer keeps both listings as `partitions-before.txt` and `partitions-after.txt`. It powers off and boots rEFInd's default entry, which must start InayOS, checks that `/efi` is the EFI system partition of Windows, and runs the desktop checks.

Shrinking NTFS marks the volume for a check, so Windows checks its disk on its next boot and may restart afterwards. That restart follows the firmware's boot order into rEFInd, whose default entry starts InayOS. So before any boot through rEFInd, the scenario reads the number of the "Windows Boot Manager" entry with `efibootmgr` in InayOS, sets it as `BootNext`, powers off and boots. When InayOS answers, Windows restarted after its check, and the scenario sets `BootNext` again, up to three starts. When Windows answers, it shuts down with `shutdown /s /t 0`, and later boots of Windows do not restart. The layer keeps each listing as `efibootmgr-disk-check-<start>.txt`.

Then it starts Windows from rEFInd twice:

1. With keys. rEFInd gives the first nine entries of its first row the shortcut digits 1 to 9, and a digit starts its entry at once. The scenario boots, presses Up once a second for 15 seconds, which stops rEFInd's 20-second wait and moves nothing, saves `refind-key-<digit>.png`, and presses the digit. It starts with 1 and powers off and tries the next digit for each entry that starts InayOS, until one starts Windows.
2. With a click, for the plan's open check that the pointer works in rEFInd on the test firmware and that a click on a firmware entry starts it. The scenario boots and moves the USB tablet's pointer once a second for 15 seconds in the bottom left corner. rEFInd's tags of the first row are 144 pixels wide, 152 pixels apart and centered on the screen, so their centers lie on the middle row of the screen, at the middle column plus a multiple of 76 pixels. The pointer visits each of those places. rEFInd draws its selection behind the tag under the pointer, so a place where the screen gains a change of at least 100 by 100 pixels against the screen with the pointer in the corner holds a tag. The scenario clicks the tag in the position of the entry that the digit found, saving `refind-click-pointer-on-windows.png` first. The click holds the button for 300 milliseconds, because rEFInd counts a click only when one of its polls sees the button held and a later one sees it released.

Each time Windows must answer SSH, and `scenario/windows/boot-current.ps1` must print that the firmware boot entry of this boot, `BootCurrent`, is "Windows Boot Manager", which shows that the firmware started Windows through `BootNext` without rEFInd. The layer keeps that output as `boot-current-<label>.txt`. Windows shuts down with `shutdown /s /t 0`. Last, rEFInd's default entry must start InayOS again, which fails when Windows put its own boot manager first in the firmware's boot order.

`bitlocker-on` boots Windows and runs `scenario/windows/bitlocker-on.ps1`, which turns BitLocker on for `C:` with a TPM protector, encrypting the used space only, and waits up to an hour until `C:` is fully encrypted. It shuts Windows down with `shutdown /s /t 0`. `fast-startup-on` boots Windows, runs `scenario/windows/fast-startup-on.ps1`, which turns hibernation and Fast Startup on, and shuts Windows down with `shutdown /s /hybrid /t 0`: Windows uses Fast Startup only for a hybrid shutdown, and a plain `shutdown /s` is a full one.

Both then boot the stick, wait until the stick's installer shows its keyboard screen, save the checksums of the disk's heads with `disk-heads.sh` as `hibernated-disk` does, and run the installer with the `beside-windows` answers. The installer must fail in its checks stage with a line that holds "Turn off BitLocker in Windows" or "Turn off Fast Startup in Windows" and "the disk is unchanged". The checks of `hibernated-disk` follow: no command in the installer's log writes to the disk, the checksums are unchanged and nothing of the disk is mounted. The installer's log counts `ntfsresize` without `--info` or `--no-action`, `ntfsfix` without `--no-action`, and the filesystem relabeling commands as writes. Then the scenario starts the installer built from the working tree in the Sway session, as `gui-install` does, presses Enter on the keyboard screen, and saves the disk screen, which gives the reason, as `bitlocker-disk-screen.png` or `fast-startup-disk-screen.png`, with the single-color check. The disk checks run once more after the screens read the disk.

The stick's installer reads the disks when it starts and mounts a Windows EFI system partition read-only for a moment. Waiting for its keyboard screen before the command-line install keeps that mount from meeting the install's checks, which refuse a mounted disk.

### The maintenance and rollback scenarios

These scenarios start from a kept disk, as `broken-kernel` does, and work over SSH as `tester`. `rollback-encrypted` starts from the kept encrypted disk and the others from the plain kept disk.

The xtask builds packages on the host with `makepkg`, in folders of the layer, so the working tree keeps no build output. makepkg skips its dependency checks, which read the host's packages. The `inayos` package comes from `test/vm/src/packages.rs`, which packages the binaries the xtask built, the same way as the stick's repository, with the pkgrel in `INAYOS_PKGREL` and each pkgrel in its own `inayos-<pkgrel>` folder. The marker package `inayos-test-marker`, whose PKGBUILD is in `src/scenario/test_repository.rs`, holds one file and gives a pacman transaction that needs no Arch mirror. The guest installs it with `pacman -U`.

`pacman-update` and `arch-news` use a test copy of the InayOS repository in the layer's `repository` folder. Each run makes a throwaway GPG key in the layer's `gnupg` folder. That key signs each package with a detached signature and signs the database through `repo-add --sign`. A small HTTP server thread in the xtask serves the folder on a free port of the host's loopback address, which the guest reaches at `10.0.2.2` through QEMU's user network. The scenario adds the public key to the guest's pacman keyring and signs it locally. It then replaces `/etc/pacman.d/inayos.conf` with an `[inayos]` section whose `Server` is that address. When `/etc/pacman.conf` of the kept disk does not include that file, the scenario stops and says to run `blank-disk` again.

`pacman-update` builds `inayos` with pkgrel 1000, installs it with `pacman -Sy`, and checks that `pacman -Q inayos` shows that version. It checks that the installed guard hook, `/usr/share/libalpm/hooks/zzz-inayos-guard.hook`, has a path trigger on `usr/bin/inayos-guard`, so that the update runs it whatever the Arch mirrors bring that day. It builds pkgrel 1001 into the repository, notes the modification time of `/var/lib/inayos-guard/status.json`, installs the newest `archlinux-keyring`, and runs `pacman -Syu --noconfirm`. `pacman -Q inayos` must then show pkgrel 1001, and the status file must have a new modification time, which shows that the guard's pacman hook ran. Last, it boots again and checks the version and that `inayos-guard-boot.service` ran and exited with status 0.

`arch-news` builds `inayos` into the repository and serves `news.xml` next to it: an RSS feed shaped like Arch's, with an item whose title says the update "requires manual intervention" and an item that announces nothing to do. Both carry the time of the run, so they are newer than anything the system read. The scenario writes `url=` with the feed's address to `/etc/inayos-news.conf` and installs `inayos` from the repository. Reinstalling `inayos` is an upgrade transaction, which runs the maintenance module's news hook. The first reinstall must print the manual-step item with its link and leave the other item out. `sudo inayos-news` must print the item, and it marks the news read. A second reinstall must print neither item.

`snapshot-cleanup` reads `NUMBER_LIMIT` and `NUMBER_LIMIT_IMPORTANT` from `/etc/snapper/configs/root`. It runs enough `pacman -U` transactions of the marker package that their pre and post snapshots pass both limits, and checks that snapper then lists more number snapshots than `NUMBER_LIMIT`. The number cleanup leaves snapshots younger than `NUMBER_MIN_AGE`, so the scenario turns NTP off in the guest and moves its clock two hours ahead. It then runs `systemctl start snapper-cleanup.service`. Snapper's cleanup keeps the `NUMBER_LIMIT` newest number snapshots, the `NUMBER_LIMIT_IMPORTANT` newest of those marked important, such as the guard's "Previous system" snapshot, and the pre or post partner of each kept snapshot. Every listed number snapshot must be kept by that rule. The cleanup must also have deleted some snapshots and kept the newest one. The layer keeps snapper's lists before and after the cleanup.

`rollback` and `rollback-encrypted` boot the installed disk. The encrypted one first checks the TPM unlock and waits for the guard's boot check. The scenario takes a snapper snapshot of the system and notes its number. Then it writes `/inayos-rollback-marker` and `~/inayos-rollback-home`, and installs the marker package. It boots the USB stick on the same layer and puts the binaries built from the working tree in place. Then it runs `inayos-installer rollback --snapshot <n> --yes`. The Secure Boot keys of the encrypted disk refuse the unsigned stick, so for `rollback-encrypted` the stick boots with an empty variable store in Setup Mode, and the installed system's variables wait in the layer as `installed-vars.fd`. The stick then measures another PCR 7 than the one the TPM sealed the keys to. The scenario copies the kept recovery key into the live system and adds `--recovery-key-file` with it. After the rollback, the installed variables go back and the installed disk boots, unlocked by the TPM again for the encrypted disk. The checks are:

1. `/inayos-rollback-marker` is gone and `pacman -Q inayos-test-marker` fails.
2. `~/inayos-rollback-home` kept its text, since `/home` is not in the snapshot.
3. `/` is mounted from the `@` subvolume.
4. `btrfs subvolume list /` has one top-level `@rollback-<date>` subvolume, which holds the replaced system. The subvolumes nested in it, such as `var/lib/machines`, are not counted.

The layer keeps the output of the rollback and the installer's log as `logs/rollback.log` and `logs/rollback-file.log`.

## State lists

The list scripts in `lists/` print the state of the guest as named lists. The lists are compared entry by entry.

On Linux and inside the Arch WSL distribution, the lists are the installed packages, the enabled system and user services, and the user's groups. They also hold every folder, link and file under `/etc`, `/boot`, `/usr/local`, `/opt`, `/var/lib/flatpak` and the home folder, with the checksum of each file and the mode and owner of each folder and file. Other places, such as `/usr` and the rest of `/var`, are not listed.

On Windows, the lists are the installed programs from the uninstall registry keys, the registry keys of the registry preferences and the fonts, the startup registry keys, and the taskbar auto-hide state. They also hold the user and machine environment variables, the Defender exclusions and the apps that Controlled Folder Access allows, the start mode of every service, and the WSL distributions. Last come every folder and file checksum of the user profile and of the Public and Default profiles.

Package and program versions are not listed, because the installer upgrades packages and removal does not downgrade them.

Each system has a file in `allowed-changes/` with the entries that may differ, such as caches and files that upgrades regenerate. Each line names a list, or `*` for every list, and a pattern in which `*` stands for any text. A line that starts with `removal` applies only to the removal comparison, so the second install still checks that entry. A difference outside that file fails the cycle and is printed.

## Installer input

`src/answers.rs` holds the answer table: a pattern for each question that the modules in `crates/modules` ask, and its answer. A question that is not in the table stops the cycle with its text. The table answers yes to the SSH server question and to the Windows T3 Code question, no to the GitHub and Tailscale login questions, and yes to every removal question.

On Linux, the installer runs over SSH in a pseudo-terminal of the guest. The xtask types the password at each `sudo` prompt and the answer at each question.

On Windows, the installer runs unelevated in the logged-on desktop session, started by a scheduled task in a console window, with its output copied to a log file that the xtask reads over SSH. When the elevation prompt `consent.exe` is open, the xtask presses Alt+Y through QMP. It types the answers into the console through QMP. Both key paths have not run against a real guest yet.

## Hands-on mode

`run <os>` boots a throwaway copy of the clean image, the failed layer with `--failed`, or with `--resume` the throwaway layer that the previous `run` left, copies the fresh installer in, prints the SSH command, and keeps the guest running until it shuts down. `--displays 2` gives a desktop guest a QXL device with two outputs, and no current guest is one.
