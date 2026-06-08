# Setup Configuration

> A modular configuration bank for building reproducible development environments across platforms.

This repository provides automated setup scripts and dotfiles for Ubuntu Server and Windows. Ubuntu Server is intentionally minimal and only installs the shared zsh/Oh My Zsh setup.

## Quick Start

Bootstrap your entire development environment with one command:

### Ubuntu Server

```bash
curl -fsSL https://raw.githubusercontent.com/ZiedYousfi/myconfig/main/bootstrap.sh | bash -s -- ubuntu
```

### Windows (cmd.exe)

```bash
powershell -NoProfile -ExecutionPolicy Bypass -Command "& { $p = Join-Path $env:TEMP 'bootstrap.ps1'; Invoke-WebRequest 'https://raw.githubusercontent.com/ZiedYousfi/myconfig/main/bootstrap.ps1' -OutFile $p; Unblock-File $p; & $p }"
```

### Interactive Mode (Auto-detect or Choose)

/!\ This does not work on Windows. Use the Windows command above instead.

```bash
curl -fsSL https://raw.githubusercontent.com/ZiedYousfi/myconfig/main/bootstrap.sh | bash
```

The bootstrap script will:

1. Download the latest release
2. Extract all configuration files
3. Run the appropriate platform installer
4. Set up dotfiles using GNU Stow where needed, or copy configs directly for minimal targets
5. Install the packages for the selected platform

## Features

- **Idempotent**: Safe to run multiple times without side effects
- **Modular**: Shared dotfiles with platform-specific additions
- **Automated**: Installs all dependencies and tools
- **Documented**: Full specifications in [SPECS.md](SPECS.md)
- **Backed Up**: Automatically backs up existing configurations

## Uninstalling

Available uninstall scripts:

```bash
./ubuntu-server/uninstall.sh  # Ubuntu Server
./windows/uninstall.ps1       # Windows (PowerShell)
```

## Documentation

See [SPECS.md](SPECS.md) for complete configuration specifications and details about all installed components.

## Requirements

Minimal requirements - the bootstrap script handles everything else:

**Ubuntu:**

- `curl` - for downloading
- Internet connection

**Windows:**

- Winget (pre-installed on Windows 10 1709+)
- Internet connection
