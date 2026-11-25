# VM xtask

This document describes the VM xtask in `test/vm/`, the `myconfig-xtask` crate. It builds the installers from the working tree and tests them in throwaway QEMU virtual machines of CachyOS, Ubuntu 26.04 LTS and Windows 11. A commit or a release is not needed. It runs on a Linux host and only by hand.

The CachyOS and Ubuntu cycles have passed on real guests, image builds included. Nothing has run on Windows, the Arch WSL checks included.

## Commands

The cargo alias in `.cargo/config.toml` runs the crate as `cargo xtask`.

```bash
cargo xtask vm image build <os>    # builds the clean image of cachyos, ubuntu or windows
cargo xtask vm cycle <os>          # tests the installer of that system
cargo xtask vm run <os>            # boots a throwaway copy with the fresh installer in it
cargo xtask vm run <os> --failed   # boots the kept disk of the last failed cycle
cargo xtask vm run <os> --resume   # boots the throwaway disk of the previous run again
cargo xtask vm run cachyos --displays 2
```

`--watch` opens a `remote-viewer` window on the guest display for any command. Without it, the guest has no window, and `run` prints the viewer command.

Only one command that starts a VM runs at a time. A lock file in the state folder enforces it.

## Host

Each command first checks that the host has what the chosen system needs. Every system needs the Arch packages `qemu-desktop`, `edk2-ovmf`, `virt-viewer`, `openssh`, `curl` and `libisoburn`, the Rust target `x86_64-unknown-linux-musl`, and read and write access to `/dev/kvm`. Windows also needs `swtpm`, the `cargo-xwin` command with the `clang`, `lld` and `llvm` packages, the Rust target `x86_64-pc-windows-msvc`, and nested virtualization in the `kvm_amd` or `kvm_intel` module. When something is missing, the command lists it with the commands that install it, and stops. It never installs anything.

`cargo xwin` downloads the Microsoft C runtime and Windows SDK into its own cache folder under `~/.cache` the first time it builds, which is about 1 GB.

The xtask starts QEMU directly, without libvirt. QEMU listens for control commands on a unix socket with QMP, the QEMU Machine Protocol, which the xtask uses for screenshots and key presses. The guest display is SPICE on a unix socket.

## Folders

The state folder is `${XDG_DATA_HOME:-~/.local/share}/myconfig/vm`, or `MYCONFIG_VM_STATE_DIR`. It holds the SSH key, the lock file, the sockets and logs of the running VM, and one folder per system with these layers:

- `image` is the clean image that every throwaway disk starts from.

- `work` is the throwaway layer of the current cycle or `run`, kept until the next cycle or `run` without `--resume`.

- `failed` is the layer of the last failed cycle, kept until the next cycle of that system.

A layer holds a qcow2 disk that reads unchanged blocks from the clean image disk, a copy of the UEFI variables, and for Windows a copy of the TPM state.

The download folder is `${XDG_CACHE_HOME:-~/.cache}/myconfig/vm`, or `MYCONFIG_VM_CACHE_DIR`. It holds the verified ISO or cloud image of each system.

## Guests

Every guest boots with UEFI firmware from `edk2-ovmf` and has 4 virtual CPUs. Linux guests have 4 GiB of memory and virtio disk and network devices. Windows has 8 GiB, the Secure Boot firmware build, a TPM 2.0 from `swtpm`, and a SATA disk and an `e1000e` network card, which Windows drives without extra drivers. The host CPU model passes the virtualization extensions to the guest, which WSL in Windows needs.

QEMU forwards host port 2222 to the guest SSH server. Every image accepts the key at `ssh/id_ed25519` in the state folder, which the xtask creates once. Files go into the guest with `scp`; the guest has no access to the repository.

The test account is `tester` with the password `myconfig`, set in `src/os.rs`.

## Images

`image build` finds the current release at its source, downloads it, checks its SHA-256, installs it unattended, logs in once over SSH, and records the release in the image's `release` file. The new image replaces the old one only when it is complete, and the throwaway and failed layers of that system go with the old image.

- CachyOS comes from the current Desktop ISO on the CachyOS download page. The live session runs the CachyOS command-line installer in headless mode from a settings file, and a script then mounts the installed system, adds the SSH key and turns on the SSH server. The xtask starts that script by typing a command into a live session terminal.

- Ubuntu comes from the official 26.04 LTS cloud image, set up by cloud-init from a NoCloud seed disc.

- Windows comes from Microsoft's consumer ISO, downloaded with the requests that the Mido script uses. When Microsoft refuses to give a download link to the host's network address, the build stops and names the page to download the English x64 ISO from by hand. A file saved into the `windows` folder of the download folder is used when its SHA-256 matches the one on that page. An answer file on a seed disc creates a local account that logs on automatically, installs the OpenSSH server and adds the key. A failure of that setup writes the failure line to the first serial port, which stops the build.

The release file holds the CachyOS ISO number, the Ubuntu cloud image build serial, or the Windows version with its build number.

## Cycle

`cycle <os>` runs these steps on a new throwaway layer:

1. It warns when the image's source lists a newer release than the image, or says that it could not check, then continues.

2. It builds the installers from the working tree: `myconfig-cachyos` or `myconfig-ubuntu-devbox` for `x86_64-unknown-linux-musl`, and for Windows `myconfig-windows` with `cargo xwin` plus `myconfig-arch-wsl`.

3. It boots the guest, copies the installers in, and prints the number of pending package updates. On Linux it syncs a temporary copy of the package database and counts with `pacman -Qu` or `apt list --upgradable` against it, so the guest's own package lists stay unchanged.

4. It saves the `before` state lists.

5. It runs `install`, then `verify`, and saves the `first-install` lists.

6. It runs `install` again and saves the `second-install` lists. Any difference from the first install fails the cycle.

7. It reads the module names with `list` and runs one `remove` with all of them, so the installer orders the removal.

8. It saves the `after` lists. Any difference from `before` fails the cycle.

On Windows, the cycle runs only the Windows installer through these steps. Its arch-wsl module creates the Arch WSL distribution on the first install, configures the same distribution again on later installs, and runs the Arch WSL installer in it on every install. The Windows installer runs with `MYCONFIG_ARCH_WSL_BINARY` set to a hook script, `lists/arch-wsl-hook.sh`. The hook saves the state lists of the distribution, then runs the fresh Arch WSL installer. The cycle also saves the state lists inside the distribution after each install and compares them. Before the Windows removal, it runs one Arch WSL `remove` with every Arch WSL module inside the distribution, and compares the lists after it with the lists the hook saved during the first install, before the Arch WSL installer first ran.

A passed cycle deletes its layer. A failed cycle saves the guest screen, shuts the guest down and keeps the layer as `failed`, with the state lists, the comparisons and the log of every installer command in it.

## State lists

The list scripts in `lists/` print the state of the guest as named lists. The lists are compared entry by entry.

On Linux and inside the Arch WSL distribution, the lists are the installed packages, the enabled system and user services, and the user's groups. They also hold every folder, link and file under `/etc`, `/boot`, `/usr/local`, `/opt`, `/var/lib/flatpak` and the home folder, with the checksum of each file and the mode and owner of each folder and file. Other places, such as `/usr` and the rest of `/var`, are not listed.

On Windows, the lists are the installed programs from the uninstall registry keys, the registry keys of the registry preferences and the fonts, the startup registry keys, and the taskbar auto-hide state. They also hold the user and machine environment variables, the Defender exclusions and the apps that Controlled Folder Access allows, the start mode of every service, and the WSL distributions. Last come every folder and file checksum of the user profile and of the Public and Default profiles.

Package and program versions are not listed, because the installer upgrades packages and removal does not downgrade them.

Each system has a file in `allowed-changes/` with the entries that may differ, such as caches and files that upgrades regenerate. Each line names a list, or `*` for every list, and a pattern in which `*` stands for any text. A line that starts with `removal` applies only to the removal comparison, so the second install still checks that entry. A difference outside that file fails the cycle and is printed.

## Installer input

`src/answers.rs` holds the answer table: a pattern for each question that the modules in `crates/modules` ask, and its answer. A question that is not in the table stops the cycle with its text. The table answers yes to the SSH server question, no to the GitHub and Tailscale login questions, and yes to every removal question.

On Linux, the installer runs over SSH in a pseudo-terminal of the guest. The xtask types the password at each `sudo` prompt and the answer at each question.

On Windows, the installer runs unelevated in the logged-on desktop session, started by a scheduled task in a console window, with its output copied to a log file that the xtask reads over SSH. When the elevation prompt `consent.exe` is open, the xtask presses Alt+Y through QMP. It types the answers into the console through QMP. Both key paths have not run against a real guest yet.

## Hands-on mode

`run <os>` boots a throwaway copy of the clean image, the failed layer with `--failed`, or with `--resume` the throwaway layer that the previous `run` left, copies the fresh installer in, prints the SSH command, and keeps the guest running until it shuts down. `--displays 2` gives the CachyOS guest a QXL device with two outputs; the second one turns on in the viewer under View > Displays.
