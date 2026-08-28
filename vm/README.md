# CachyOS Virtual Machine

This harness installs CachyOS interactively once, preserves that installation as a clean base, and runs disposable full-system tests against the current myconfig working tree. A commit or release is not required.

## Host Requirements

The host needs QEMU with SPICE support, access to `/dev/kvm`, `qemu-img`, `curl`, `sha256sum`, `grep`, `sort`, `flock`, `ssh`, `ssh-keygen`, and the x64 EDK2 UEFI firmware under `/usr/share/edk2/x64/`. The script installs the Arch `virt-viewer` package when its `remote-viewer` command is absent.

Sealing also needs `virt-cat`, `guestfish`, and a host kernel image to run the libguestfs helper VM. On an Arch Linux host, the script installs the `linux`, `libguestfs`, and `guestfs-tools` packages with `sudo pacman` when any of these requirements are absent. WSL does not boot this Arch kernel; libguestfs uses it only for offline disk access.

The VM uses 8 GiB RAM, 6 virtual CPUs, and a 100 GiB sparse disk. QEMU opens `remote-viewer` through its local SPICE display, uses the SPICE-native QXL virtual graphics device, and exposes the standard guest-agent channel. CachyOS includes `spice-vdagent`, so resizing the viewer requests a matching guest resolution instead of scaling a low-resolution framebuffer. VM state is stored under `${XDG_DATA_HOME:-$HOME/.local/share}/myconfig/cachyos-vm`. Downloaded ISOs are cached under `${XDG_CACHE_HOME:-$HOME/.cache}/myconfig/cachyos-vm`.

## Install CachyOS

Run:

```bash
./vm/cachyos.sh install
```

The command discovers the current CachyOS Desktop ISO from the official download page, downloads its adjacent SHA-256 file, and verifies the ISO before starting QEMU. It then creates the base disk and opens the graphical CachyOS installer.

Complete the installation manually onto the 100 GiB virtual disk. Use UEFI boot and do not enable disk encryption because sealing must read and modify the stopped guest disk. Shut down the guest when installation finishes. If installation was interrupted, run `install` again to resume with the existing base disk.

## Seal The Clean Base

Run:

```bash
./vm/cachyos.sh seal
```

The script mounts the default CachyOS Btrfs `@` and `@home` subvolumes, detects the single installed desktop user, generates a dedicated SSH key under VM state, enables the installed OpenSSH service, injects the public key, and adds a one-time boot service that allows TCP port 22 through UFW. It then boots the installed base without the ISO or repository share, forwards local port 2222 to guest SSH, and waits for SSH to become ready.

Log in and confirm that CachyOS reaches its desktop with the intended display layout, then shut down normally. After QEMU exits, the command asks whether the desktop booted successfully. It copies the desktop-generated QXL output layout to the Plasma Login greeter so the login screen uses the same crisp resolution, then marks the base ready. This modifies only the VM base. After sealing, the harness never boots the base disk directly again.

## Test This Working Tree

Run:

```bash
./vm/cachyos.sh run
```

The first run creates a disposable disk backed by the sealed base. Later runs continue from that same test disk until it is reset.

The repository is attached through QEMU 9p with write access. **Guest root can modify or delete files in the host checkout as the host user running QEMU.** Commit or back up work that must survive before running guest commands.

The harness forwards local port 2222 to guest SSH and waits for the guest. It then mounts the repository at `/mnt/myconfig` and starts `bash /mnt/myconfig/cachyos/install.sh` in an interactive SSH terminal. The desktop remains open for graphical checks until you shut it down.

The CachyOS profile reads the live working tree, including unstaged and untracked files. During Axidev OSK setup, select the installed login manager from the terminal menu. After the profile completes, reboot the test guest and verify that Axidev OSK appears on the login screen, emits input there, starts again in the desktop session, and types into a focused application.

## Test Multiple Displays

Run:

```bash
./vm/cachyos.sh run-multi-display
```

This uses the same disposable test disk as `run` but allows two QXL outputs. In the SPICE viewer, enable **View > Displays > Display 2** after login; QXL's second output is only connected when the viewer enables it. MyConfig Plasma Panels then reconciles the new display automatically. Verify that each display gets its own KDE Plasma top panel and application dock, that the full-width 8-pixel top and bottom activation zones reveal the compact panels, that Overview can switch the displays' virtual desktops independently, and that each Icons-only Task Manager shows windows from its own display across all virtual desktops. Reset first when the check must start from the sealed base.

## Reset The Test System

Shut down QEMU, then run:

```bash
./vm/cachyos.sh reset
```

Reset removes only the disposable disk and its UEFI variables. It preserves the manually installed base, dedicated SSH key, detected guest user, and downloaded ISO. The next `run` starts from clean CachyOS without repeating the graphical installation.

## Limits

The VM can validate the CachyOS installer, package installation, login-manager integration, desktop startup, and virtual keyboard input. It does not reproduce physical GPU behavior, touch hardware, machine firmware, device drivers, or native performance.
