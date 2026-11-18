# VM xtask

This document describes the VM xtask in `test/vm/`, the `inayos-xtask` crate. It builds the installers from the working tree and tests them in throwaway QEMU virtual machines of Ubuntu 26.04 LTS and Windows 11. It also builds the InayOS USB stick image in an Arch Linux virtual machine, and runs the InayOS installer from that image in scenarios. A commit or a release is not needed. It runs on a Linux host and only by hand.

The Ubuntu cycle has passed on a real guest, image build included. Nothing has run on Windows, the Arch WSL checks included. The Arch image build, the USB stick build and the three scenarios have passed on a real guest.

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

Each command first checks that the host has what the chosen system needs. Every system needs the Arch packages `qemu-desktop`, `edk2-ovmf`, `virt-viewer`, `openssh`, `curl` and `libisoburn`, the Rust target `x86_64-unknown-linux-musl`, and read and write access to `/dev/kvm`. Windows also needs `swtpm`, the `cargo-xwin` command with the `clang`, `lld` and `llvm` packages, the Rust target `x86_64-pc-windows-msvc`, and nested virtualization in the `kvm_amd` or `kvm_intel` module. When something is missing, the command lists it with the commands that install it, and stops. It never installs anything.

`cargo xwin` downloads the Microsoft C runtime and Windows SDK into its own cache folder under `~/.cache` the first time it builds, which is about 1 GB.

The xtask starts QEMU directly, without libvirt. QEMU listens for control commands on a unix socket with QMP, the QEMU Machine Protocol, which the xtask uses for screenshots and key presses. The guest display is SPICE on a unix socket.

## Folders

The state folder is `${XDG_DATA_HOME:-~/.local/share}/inayos/vm`, or `INAYOS_VM_STATE_DIR`. It holds the SSH key, the lock file, the sockets and logs of the running VM, and one folder per system with these layers:

- `image` is the clean image that every throwaway disk starts from.

- `work` is the throwaway layer of the current cycle or `run`, kept until the next cycle or `run` without `--resume`.

- `failed` is the layer of the last failed cycle or USB stick build, kept until the next one of that system.

The state folder also holds the USB stick image at `usb/inayos.iso`, and a folder per scenario under `scenario/` with the same `work` and `failed` layers.

A layer holds a qcow2 disk that reads unchanged blocks from the clean image disk, a copy of the UEFI variables, and for Windows a copy of the TPM state.

The download folder is `${XDG_CACHE_HOME:-~/.cache}/inayos/vm`, or `INAYOS_VM_CACHE_DIR`. It holds the verified ISO or cloud image of each system.

## Guests

Every guest boots with UEFI firmware from `edk2-ovmf` and has 4 virtual CPUs. Linux guests have virtio disk and network devices. Ubuntu has 4 GiB of memory, and Arch guests have 8 GiB. Windows has 8 GiB, the Secure Boot firmware build, a TPM 2.0 from `swtpm`, and a SATA disk and an `e1000e` network card, which Windows drives without extra drivers. The host CPU model passes the virtualization extensions to the guest, which WSL in Windows needs.

QEMU forwards host port 2222 to the guest SSH server. Every image accepts the key at `ssh/id_ed25519` in the state folder, which the xtask creates once. Files go into the guest with `scp`; the guest has no access to the repository.

The test account is `tester` with the password `inayos`, set in `src/os.rs`.

## Images

`image build` finds the current release at its source, downloads it, checks its SHA-256, installs it unattended, logs in once over SSH, and records the release in the image's `release` file. The new image replaces the old one only when it is complete, and the throwaway and failed layers of that system go with the old image.

- Ubuntu comes from the official 26.04 LTS cloud image, set up by cloud-init from a NoCloud seed disc.

- Arch Linux comes from Arch's official cloud image in `https://geo.mirror.pkgbuild.com/images/latest/`, set up by cloud-init from a NoCloud seed disc. The build downloads the versioned file of the newest image in that folder listing, with its `.SHA256` file. Its image only hosts the USB stick build: `cycle arch` and `run arch` refuse and point at the scenarios.

- Windows comes from Microsoft's consumer ISO, downloaded with the requests that the Mido script uses. When Microsoft refuses to give a download link to the host's network address, the build stops and names the page to download the English x64 ISO from by hand. A file saved into the `windows` folder of the download folder is used when its SHA-256 matches the one on that page. An answer file on a seed disc creates a local account that logs on automatically, installs the OpenSSH server and adds the key. A failure of that setup writes the failure line to the first serial port, which stops the build.

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

`usb build` boots a throwaway layer over the clean Arch image and builds `inayos-installer` and `inayos-arch-desktop` from the working tree for `x86_64-unknown-linux-musl`. It copies them into the guest with the profile overlay in `test/usb/profile/` and the script `usb/build-iso.sh`, which runs as root:

1. It upgrades the guest and installs `archiso`.

2. It copies archiso's `releng` profile from `/usr/share/archiso/configs/releng`, then copies the overlay over it. Every overlay file replaces or adds the file at the same path, except `packages.x86_64`, whose lines are appended to releng's list.

3. It places the installer at `airootfs/usr/local/bin/inayos-installer` and the InayOS desktop installer at `airootfs/usr/local/lib/inayos/inayos`, with mode 755.

4. It extracts the latest Axidev OSK release into `airootfs/opt/`, which gives `/opt/axidev-osk/bin/axidev-osk`.

5. It runs `mkarchiso`.

The xtask copies the image to `usb/inayos.iso.part` in the state folder and renames it to `usb/inayos.iso`, so that file is always a complete image. A passed build deletes its layer. A failed build keeps it as the failed Arch layer, with the build log and the last screen.

## Scenarios

`scenario <name>` installs InayOS from the USB stick image. Its guest has 8 GiB of memory, UEFI without Secure Boot, a new blank 80 GB qcow2 target disk and fresh UEFI variables in the scenario's layer. The image is a CD drive that the firmware starts first. A NoCloud seed disc labeled `CIDATA` gives root on the live system the xtask's SSH key.

The xtask builds `inayos-installer` and `inayos-arch-desktop` from the working tree and copies them over the copies on the image, so a scenario tests the working tree without a new image. It drives the live system over SSH as root. It writes the answers file in `src/scenario/answers_file.rs`, copies it in, and runs `inayos-installer install --answers` with it. The answers file creates the `tester` account with the password from `src/os.rs` and the xtask's key in `authorized_keys`. It answers yes to the SSH server question and no to the GitHub and Tailscale login questions, as the answer table of the cycle does. The installer's output and its own log go to the `logs` folder of the layer.

After the install, the xtask powers the guest off and boots the target disk without the image. It checks the installed system over SSH as `tester`, which works through the SSH server that the ssh module turns on. It takes QMP screenshots.

The desktop checks are:

1. `plasmalogin.service` is active and an Axidev OSK process runs for the login screen. A screenshot of the login screen is saved.

2. The xtask types the password at the login screen through QMP, and `plasmashell` starts for `tester`. When no session starts within two minutes, it clears the password field with Ctrl+A and Backspace and types the password once more, because the first keys can arrive before the field has the focus.

3. The user services `inayos-kanata-tray`, `inayos-kanata` and `inayos-handy` are active.

4. `plasma-org.kde.plasma.desktop-appletsrc` has one panel marked as the top bar and one marked as the dock by the default layout of `dotfiles/plasma`, each with that layout's widgets. A screenshot of the desktop is saved.

The scenarios are:

- `blank-disk` wipes the target disk with the default layout, with a 40 GiB system, and runs the desktop checks.

- `reinstall-keep-home` installs as `blank-disk` does, writes a marker file into the home folder of the installed system, boots the image again and reinstalls keeping home on `/dev/vda1` to `/dev/vda3`. It checks that the marker survived with its owner and that the home folder kept its uid and gid, then runs the desktop checks.

- `shrink-ext4` builds a disk like the laptop's on the live system with `scenario/laptop-disk.sh`: a 2 GiB EFI partition mounted at `/boot` that holds the kernel and the Intel microcode, and one ext4 partition with an Arch base system and test files in `/home/tester`, whose checksums it saves. That `tester` has uid and gid 1234, which a new system never picks by itself. It installs with the shrink choice and a 30 GiB system. It checks:
  - the files, and that they and the new `tester` still have uid and gid 1234;
  - that the top level of the home partition holds only `lost+found` and `tester`;
  - that the partitions on the disk are the EFI partition, then the ext4 home, then the Btrfs system;
  - that the EFI partition keeps its filesystem UUID, so it was not formatted, and that the old kernel, initramfs and microcode files are gone from it.

  Then it runs the desktop checks.

The layer keeps the answers files, logs, serial logs of each boot and screenshots. A passed scenario deletes its layer. A failed scenario saves the last screen and keeps the layer as that scenario's `failed` layer until its next run.

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
