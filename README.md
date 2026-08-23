# Setup Configuration

> A modular configuration bank for building reproducible development environments across platforms.

This repository provides automated setup scripts and dotfiles for CachyOS, Ubuntu Server, and Windows Workstation. CachyOS installs the complete development profile. Ubuntu Server installs only the shared Zsh setup.

## Quick Start

Bootstrap your entire development environment with one command:

### CachyOS

Run this after completing the CachyOS graphical installer:

```bash
curl -fsSL https://raw.githubusercontent.com/inayayousfi/myconfig/main/bootstrap.sh | bash -s -- cachyos
```

The final authentication step offers GitHub and Tailscale login. Declining either prompt prints the command for later.

### Ubuntu Server

```bash
curl -fsSL https://raw.githubusercontent.com/inayayousfi/myconfig/main/bootstrap.sh | bash -s -- ubuntu
```

### Windows Workstation (PowerShell)

```powershell
irm https://raw.githubusercontent.com/inayayousfi/myconfig/main/bootstrap.ps1 | iex
```

This pipes the script straight into `iex` without writing anything to disk, so there's no Mark-of-the-Web flag and no execution-policy check to satisfy — it works even on locked-down/GPO-managed machines.

If your environment blocks piping remote content into `iex`, fall back to:

```bash
powershell -NoProfile -ExecutionPolicy Bypass -Command "& { $p = Join-Path $env:TEMP 'bootstrap.ps1'; Invoke-WebRequest 'https://raw.githubusercontent.com/inayayousfi/myconfig/main/bootstrap.ps1' -OutFile $p; Unblock-File $p; & $p }"
```

After the first install, the bootstrap flow imports a self-signed code-signing certificate and sets PowerShell's `ExecutionPolicy` to `RemoteSigned` for the current user. Re-running `windows-workstation/install.ps1`, `nvim`, or the PowerShell profile script directly afterward no longer needs `-Bypass`, since those scripts are signed as part of each release.

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

## Features

- **Repeatable**: Safe to rerun, with timestamped backups before replacement
- **Modular**: Shared dotfiles with platform-specific additions
- **Automated**: Installs all dependencies and tools
- **Documented**: Full specifications in [SPECS.md](SPECS.md)
- **Backed Up**: Automatically backs up existing configurations

## Uninstalling

Available uninstall scripts:

```bash
./ubuntu-server/uninstall.sh  # Ubuntu Server
./windows-workstation/uninstall.ps1  # Windows Workstation
```

## Documentation

See [SPECS.md](SPECS.md) for complete configuration specifications and details about all installed components.

## Requirements

Minimal requirements - the bootstrap script handles everything else:

**CachyOS and Ubuntu Server:**

- `curl` - for downloading
- Internet connection

**Windows:**

- Winget (pre-installed on Windows 10 1709+)
- Internet connection
