# InayOS

> A modular configuration bank for building reproducible development environments across platforms.

This repository provides installers and dotfiles for the InayOS desktop on Arch Linux, Arch WSL, Ubuntu Devbox, and Windows Workstation. The InayOS desktop installs the complete development profile. Arch WSL installs the shell, terminal and agent tools. Ubuntu Devbox sets up a personal Ubuntu machine for coding and long-running agents.

## Quick Start

Each install script downloads the binary for your platform from the latest release, checks its SHA-256, keeps it as `inayos`, and opens its screen. On the screen, `space` ticks a module, `i` installs, `v` verifies, `r` removes and `q` quits.

### InayOS desktop, Arch WSL, and Ubuntu Devbox

Run this on Arch Linux with KDE Plasma installed, inside the Arch WSL distribution, or on Ubuntu:

```bash
curl -fsSL https://raw.githubusercontent.com/inayayousfi/myconfig/main/install.sh | bash
```

The script detects the platform and asks you to confirm it. To name it yourself, pass `arch-desktop`, `arch-wsl`, or `ubuntu-devbox`:

```bash
curl -fsSL https://raw.githubusercontent.com/inayayousfi/myconfig/main/install.sh | bash -s -- arch-desktop
```

The binary is kept as `~/.local/bin/inayos`.

The InayOS desktop profile requires KDE Plasma 6.7 through 6.x and applies Breeze Dark with a pink accent color and a macOS-style layout: a top bar holding a KDE session menu, the global menu, tray and clock, and an application dock with a one-click Overview icon, on every display. It installs [Neovim](docs/neovim.md) as the terminal editor, VSCodium for text files opened from the desktop, the T3 Code desktop app, whose client settings come from the `t3code-desktop` package, and the [Herdr](docs/herdr.md) terminal workspace in Black & Pink. It installs Google Chrome, and LyX with TeX Live, the Qalculate calculator and Okular as the PDF viewer. The session uses a US keyboard layout with the right Alt key as the Compose key. The boot shows [bootvine](https://codeberg.org/wvhulle/bootvine), which draws the systemd jobs still starting as a live tree on the console, and the profile passes `quiet` only on the first boot entry of `/boot/refind_linux.conf`. The [system tuning](docs/system-tuning.md) module adds zram swap with zswap off, DNS through systemd-resolved, the swap and console kernel settings, a higher open-file limit and the `ntsync` module. It also installs the Black & Pink rEFInd theme, and on a system that the InayOS installer builds, rEFInd itself, installs Axidev OSK, Kanata with an independent KDE tray, and Handy offline dictation with either-side `Ctrl+Shift` push-to-talk. The [wake sources](docs/wake-sources.md) module leaves only keyboards, the power button and the lid able to wake the sleeping system. Log out and back in so new input-device group memberships apply, then restart the selected login manager or reboot to activate login-screen startup.

The Ubuntu Devbox profile installs the development runtimes, the command-line tools, [Neovim](docs/neovim.md), [herdr-core](docs/herdr.md), rootless Docker, Tailscale, the coding agents, and the T3 Code server, which keeps running after logout. It leaves the machine's SSH configuration as it is. VSCodium and VS Code open folders on the machine over SSH with their remote SSH extensions.

When the GitHub CLI or Tailscale is logged out, the Linux installers offer to log in. If you decline, they print the command to run later: `gh auth login` or `sudo tailscale up`.

### Windows Workstation (PowerShell)

```powershell
irm https://raw.githubusercontent.com/inayayousfi/myconfig/main/install.ps1 | iex
```

The binary is kept as `%LOCALAPPDATA%\inayos\inayos.exe`, and that folder is added to your user `PATH`. The script also sets PowerShell's `ExecutionPolicy` to `RemoteSigned` for the current user when it is stricter, so the installed PowerShell profile can load.

The script makes these changes before any module runs, so `remove` leaves them in place, together with the binary. To undo them, delete `%LOCALAPPDATA%\inayos\inayos.exe`, take its folder off your user `PATH` in the environment variable settings, and run `Set-ExecutionPolicy Undefined -Scope CurrentUser`.

### Commands

Arguments after the platform go to the binary instead of opening the screen:

```bash
inayos install              # installs every module, then verifies each
inayos install zsh ghostty  # installs only the named modules
inayos verify               # checks every module without changing anything
inayos remove zsh           # undoes what the zsh module changed
inayos list                 # prints the module names
```

The same commands work through the install script, such as `curl -fsSL .../install.sh | bash -s -- arch-desktop install`. A run without a terminal needs a command, because it cannot show the screen.

Before changing anything, each module records what was there in `~/.local/state/inayos/state.json` (`%LOCALAPPDATA%\inayos\state\state.json` on Windows). `remove` uses that record to put files, settings and packages back.

### Machines that myconfig set up

myconfig is the earlier name of InayOS. On a machine that myconfig set up, its units, scripts, system files, KDE keys, APT sources and Windows startup shortcuts carry the name `myconfig`. An InayOS install adds its own copies next to them and leaves them in place. Kanata, Handy, the tray icons and AutoHotkey then run twice, the two libinput plugins double the pointer speed, and on Ubuntu Devbox `apt-get update` fails on the two sources of one repository.

The first InayOS run moves the record of everything installed under the earlier name into a group named `myconfig-leftovers`. Run this once on each system that myconfig set up, inside Arch WSL as well as on Windows, before the first InayOS install:

```bash
inayos remove myconfig-leftovers
```

It disables and stops the earlier units, deletes or puts back the earlier files, and deletes the earlier KDE keys, VSCodium theme and startup shortcuts. It takes the `# BEGIN MYCONFIG REFIND` block out of `refind.conf`, and the `myconfig-picture-in-picture` rule out of the KWin rule list, and leaves the rest of both as it is.

It also deletes the links under earlier names that point into `~/dotfiles` at a file that is gone. The links under earlier names whose file is still in `~/dotfiles` go when the next install replaces that package. Packages, the other settings and the dock pins stay. Running it after an install, or a second time, is safe.

### Virtual Machine Testing

The [VM xtask](test/vm/README.md) builds the installers from the working tree and runs them in throwaway Ubuntu and Windows virtual machines, without a commit or release. The Ubuntu cycle passes on a real virtual machine. The Windows cycle has not run yet, and no guest runs the InayOS desktop profile.

## Features

- **Repeatable**: Safe to rerun; a module only changes what differs from its target state
- **Modular**: Each module installs, verifies and removes one concern, and you choose which ones run
- **Reversible**: `remove` restores the files, settings and packages recorded before install

## Requirements

Minimal requirements - the install script handles everything else:

**InayOS desktop, Arch WSL, and Ubuntu Devbox:**

- `curl` and `sha256sum` - for downloading and checking the binary
- Internet connection

**Windows:**

- Winget (pre-installed on Windows 10 1709+)
- Internet connection
