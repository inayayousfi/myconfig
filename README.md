# Setup Configuration

> A modular configuration bank for building reproducible development environments across platforms.

This repository provides installers and dotfiles for CachyOS, Arch WSL, Ubuntu Devbox, and Windows Workstation. CachyOS installs the complete development profile. Arch WSL installs the shell, terminal and agent tools. Ubuntu Devbox sets up a personal Ubuntu machine for coding and long-running agents.

## Quick Start

Each install script downloads the binary for your platform from the latest release, checks its SHA-256, keeps it as `myconfig`, and opens its screen. On the screen, `space` ticks a module, `i` installs, `v` verifies, `r` removes and `q` quits.

### CachyOS, Arch WSL, and Ubuntu Devbox

Run this after completing the CachyOS graphical installer, inside the Arch WSL distribution, or on Ubuntu:

```bash
curl -fsSL https://raw.githubusercontent.com/inayayousfi/myconfig/main/install.sh | bash
```

The script detects the platform and asks you to confirm it. To name it yourself, pass `cachyos`, `arch-wsl`, or `ubuntu-devbox`:

```bash
curl -fsSL https://raw.githubusercontent.com/inayayousfi/myconfig/main/install.sh | bash -s -- cachyos
```

The binary is kept as `~/.local/bin/myconfig`.

The CachyOS profile requires KDE Plasma 6.7 through 6.x and applies Breeze Dark with a pink accent color and a macOS-style layout: a top bar holding a KDE session menu, the global menu, tray and clock, and an application dock with a one-click Overview icon, on every display. It installs VSCodium as the text editor and the T3 Code desktop app, whose client settings come from the `t3code-desktop` package. The profile replaces the CachyOS boot splash with [bootvine](https://codeberg.org/wvhulle/bootvine), which draws the systemd jobs still starting as a live tree on the console. It removes the `nowatchdog` and `splash` kernel options from `/boot/refind_linux.conf` and passes `quiet` only on the first boot entry. It also installs the Black & Pink rEFInd theme, installs Axidev OSK, Kanata with an independent KDE tray, and Handy offline dictation with either-side `Ctrl+Shift` push-to-talk. Log out and back in so new input-device group memberships apply, then restart the selected login manager or reboot to activate login-screen startup.

The Ubuntu Devbox profile installs the development runtimes, the command-line tools, rootless Docker, Tailscale, the coding agents, and the T3 Code server, which keeps running after logout. It leaves the machine's SSH configuration as it is. VSCodium and VS Code open folders on the machine over SSH with their remote SSH extensions.

When the GitHub CLI or Tailscale is logged out, the Linux installers offer to log in. If you decline, they print the command to run later: `gh auth login` or `sudo tailscale up`.

### Windows Workstation (PowerShell)

```powershell
irm https://raw.githubusercontent.com/inayayousfi/myconfig/main/install.ps1 | iex
```

The binary is kept as `%LOCALAPPDATA%\myconfig\myconfig.exe`, and that folder is added to your user `PATH`. The script also sets PowerShell's `ExecutionPolicy` to `RemoteSigned` for the current user when it is stricter, so the installed PowerShell profile can load.

### Commands

Arguments after the platform go to the binary instead of opening the screen:

```bash
myconfig install              # installs every module, then verifies each
myconfig install zsh ghostty  # installs only the named modules
myconfig verify               # checks every module without changing anything
myconfig remove zsh           # undoes what the zsh module changed
myconfig list                 # prints the module names
```

The same commands work through the install script, such as `curl -fsSL .../install.sh | bash -s -- cachyos install`. A run without a terminal needs a command, because it cannot show the screen.

Before changing anything, each module records what was there in `~/.local/state/myconfig/state.json` (`%LOCALAPPDATA%\myconfig\state\state.json` on Windows). `remove` uses that record to put files, settings and packages back.

### Virtual Machine Testing

The [VM xtask](test/vm/README.md) builds the installers from the working tree and tests them in throwaway CachyOS, Ubuntu and Windows virtual machines, without a commit or release.

## Features

- **Repeatable**: Safe to rerun; a module only changes what differs from its target state
- **Modular**: Each module installs, verifies and removes one concern, and you choose which ones run
- **Reversible**: `remove` restores the files, settings and packages recorded before install

## Requirements

Minimal requirements - the install script handles everything else:

**CachyOS, Arch WSL, and Ubuntu Devbox:**

- `curl` and `sha256sum` - for downloading and checking the binary
- Internet connection

**Windows:**

- Winget (pre-installed on Windows 10 1709+)
- Internet connection
