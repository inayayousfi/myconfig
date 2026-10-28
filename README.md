# Setup Configuration

> A modular configuration bank for building reproducible development environments across platforms.

This repository provides automated setup scripts and dotfiles for CachyOS, Ubuntu Devbox, and Windows Workstation. CachyOS installs the complete development profile. Ubuntu Devbox sets up a personal Ubuntu machine for coding and long-running agents.

## Quick Start

Bootstrap your entire development environment with one command:

### CachyOS

Run this after completing the CachyOS graphical installer:

```bash
curl -fsSL https://raw.githubusercontent.com/inayayousfi/myconfig/main/bootstrap.sh | bash -s -- cachyos
```

The CachyOS profile requires KDE Plasma 6.7 through 6.x and applies Breeze Dark with a pink accent color and a macOS-style layout: a top bar holding a KDE session menu, the global menu, tray and clock, and an application dock with a one-click Overview icon, on every display. It installs VSCodium as the text editor. The profile also installs the Black & Pink rEFInd theme, installs Axidev OSK, Kanata with an independent KDE tray, and Handy offline dictation with either-side `Ctrl+Shift` push-to-talk. Log out and back in so new input-device group memberships apply, then restart the selected login manager or reboot to activate login-screen startup.

The final authentication step offers GitHub and Tailscale login. Declining either prompt prints the command for later.

### Ubuntu Devbox

```bash
curl -fsSL https://raw.githubusercontent.com/inayayousfi/myconfig/main/bootstrap.sh | bash -s -- ubuntu-devbox
```

The Ubuntu Devbox profile installs the development runtimes, the command-line tools, rootless Docker, Tailscale, the coding agents, and the T3 Code server, which keeps running after logout. It leaves the machine's SSH configuration as it is. VSCodium and VS Code open folders on the machine over SSH with their remote SSH extensions.

The final authentication step offers GitHub and Tailscale login. Declining either prompt prints the command for later.

### Windows Workstation (PowerShell)

```powershell
irm https://raw.githubusercontent.com/inayayousfi/myconfig/main/bootstrap.ps1 | iex
```

This pipes the script straight into `iex` without writing anything to disk, so there's no Mark-of-the-Web flag and no execution-policy check to satisfy — it works even on locked-down/GPO-managed machines.

If your environment blocks piping remote content into `iex`, fall back to:

```bash
powershell -NoProfile -ExecutionPolicy Bypass -Command "& { $p = Join-Path $env:TEMP 'bootstrap.ps1'; Invoke-WebRequest 'https://raw.githubusercontent.com/inayayousfi/myconfig/main/bootstrap.ps1' -OutFile $p; Unblock-File $p; & $p }"
```

After the first install, the bootstrap flow imports a self-signed code-signing certificate and sets PowerShell's `ExecutionPolicy` to `RemoteSigned` for the current user. Re-running `windows-workstation/install.ps1` or the PowerShell profile script directly afterward does not need `-Bypass`, since those scripts are signed as part of each release.

### Interactive Mode (Auto-detect or Choose)

/!\ This does not work on Windows. Use the Windows command above instead.

```bash
curl -fsSL https://raw.githubusercontent.com/inayayousfi/myconfig/main/bootstrap.sh | bash
```

The bootstrap script will:

1. Download the latest release
2. Extract all configuration files
3. Run the appropriate platform installer
4. Copy Linux dotfiles into `~/dotfiles`, back up conflicts, and link them with GNU Stow
5. Install the packages for the selected platform

### Editor Settings Only (Windows)

On a Windows machine that should receive only the VSCodium or VS Code settings, theme and extensions, right-click `windows-workstation\install-editor.ps1` in a clone of this repository and choose **Run with PowerShell**. It configures every editor installed at that moment; run it again after installing the other editor. It also makes the first installed of VSCodium and VS Code, or Notepad when neither is installed, the editor that Git and other programs open.

### CachyOS Virtual Machine Testing

The [CachyOS VM harness](vm/README.md) installs the Desktop ISO interactively, preserves a clean base system, and tests the current working tree without a commit or release.

## Features

- **Repeatable**: Safe to rerun, with timestamped backups before replacement
- **Modular**: Shared dotfiles with platform-specific additions
- **Automated**: Installs all dependencies and tools
- **Backed Up**: Automatically backs up existing configurations

## Uninstalling

Available uninstall scripts:

```bash
./windows-workstation/uninstall.ps1  # Windows Workstation
```

## Requirements

Minimal requirements - the bootstrap script handles everything else:

**CachyOS and Ubuntu Devbox:**

- `curl` - for downloading
- Internet connection

**Windows:**

- Winget (pre-installed on Windows 10 1709+)
- Internet connection
