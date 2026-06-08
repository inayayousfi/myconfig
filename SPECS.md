# Configuration Specifications

> A modular configuration bank for building reproducible development environments across Ubuntu Server, Windows, and optional Arch WSL.

This document describes the current setup scripts, package groups, dotfiles, and platform-specific behavior in this repository.

---

## Table of Contents

- [Bootstrap Flow](#bootstrap-flow)
- [Shell](#shell)
- [CLI Tools](#cli-tools)
- [Terminal And Editors](#terminal-and-editors)
- [File Manager](#file-manager)
- [Development Languages And Runtimes](#development-languages-and-runtimes)
- [Shared Non-Windows Package Baseline](#shared-non-windows-package-baseline)
- [Platform-Specific: Ubuntu Server](#platform-specific-ubuntu-server)
- [Platform-Specific: Windows](#platform-specific-windows)
- [Platform-Specific: Arch WSL](#platform-specific-arch-wsl)
- [Dotfiles Summary](#dotfiles-summary)

---

## Bootstrap Flow

The root bootstrap scripts download the latest GitHub release when available, fall back to the main branch when needed, stage the repository under `~/.setup-config`, back up any previous staged install, and hand off to the platform installer.

| Platform | Bootstrap | Installer |
|----------|-----------|-----------|
| Ubuntu Server | `bootstrap.sh` | `ubuntu-server/install.sh` |
| Windows | `bootstrap.ps1` | `windows/install.ps1` |

### Linux Bootstrap

`bootstrap.sh` supports `ubuntu`, `linux`, and `server` as Ubuntu Server aliases. Interactive mode can auto-detect apt-based Linux systems or prompt for Ubuntu Server.

The Linux bootstrap ensures `curl` and `unzip` exist before downloading the archive.

### Windows Bootstrap

`bootstrap.ps1` downloads and validates the ZIP archive, extracts with `Expand-Archive` or a .NET fallback, unblocks PowerShell files, and runs `windows/install.ps1` from the staged repository.

---

## Shell

### Zsh

The Unix-like shell is **Zsh** with **Oh My Zsh**.

| Component | Value |
|-----------|-------|
| Shell | `zsh` |
| Framework | Oh My Zsh |
| Theme | `blacknpink` |
| Config location | `~/.zshrc` |
| Custom files | `~/.oh-my-zsh/custom/` |

#### Plugins

| Plugin | Source | Description |
|--------|--------|-------------|
| `git` | Built-in | Git aliases and completions |
| `vi-mode` | Built-in | Vi keybindings in shell |
| `zsh-autosuggestions` | `zsh-users/zsh-autosuggestions` | Fish-like autosuggestions |
| `zsh-syntax-highlighting` | `zsh-users/zsh-syntax-highlighting` | Syntax highlighting |
| `inaya` | Custom dotfile | Personal aliases, functions, and environment |

#### Custom Theme: `blacknpink`

Location: `dotfiles/zsh/.oh-my-zsh/custom/themes/blacknpink.zsh-theme`

The theme is based on Oh My Zsh's bundled `refined` prompt and maps prompt colors to the shared Black & Pink palette.

#### Custom Plugin: `inaya`

Location: `dotfiles/zsh/.oh-my-zsh/custom/plugins/inaya/inaya.plugin.zsh`

Key environment defaults include XDG paths, `nvim` as editor, `xterm-256color`, UTF-8 locale, and vi-mode cursor support.

Key aliases and functions include `nvim` shortcuts, modern CLI replacements for `ls`, `find`, and `grep`, `lg` for Lazygit, `ff` for Fastfetch, `y` for Yazi directory handoff, `pf` for fuzzy file opening, `mkd`, `use-tmux`, `reload-zsh`, `stowgo`, `update`, and `cleanup`.

### PowerShell

Windows uses **PowerShell Core** with **Oh My Posh**.

| Component | Value |
|-----------|-------|
| Shell | PowerShell Core (`pwsh`) |
| Prompt | Oh My Posh |
| Theme | `black-pink.omp.json` |
| Profile | `windows/dotfiles/PowerShell/Microsoft.PowerShell_profile.ps1` |
| Modules | `PSReadLine`, `Terminal-Icons` |

PowerShell profile features include vi mode keybindings, history predictions, cursor shape changes for insert and normal modes, terminal icons, and Windows Terminal integration helpers.

---

## CLI Tools

### Core Utilities

| Tool | Purpose | Replaces |
|------|---------|----------|
| `eza` | Modern ls with icons and git integration | `ls` |
| `fd` | Fast, user-friendly find | `find` |
| `ripgrep` (`rg`) | Fast recursive grep | `grep` |
| `bat` | Cat with syntax highlighting | `cat` |
| `fzf` | Fuzzy finder | - |
| `zoxide` | Smart cd with frecency | `cd` |
| `btop` | Resource monitor | `top`, `htop` |
| `fastfetch` | System information display | `neofetch` |
| `jq` | JSON processor | - |
| `tokei` | Code statistics | - |

### Git Tools

| Tool | Purpose |
|------|---------|
| `git` | Version control |
| `lazygit` | Terminal UI for git |

### Lazygit

Lazygit uses a shared Black & Pink theme.

| Setting | Value |
|---------|-------|
| Config location | `$XDG_CONFIG_HOME/lazygit/config.yml` |
| Windows config location | `%LOCALAPPDATA%\lazygit\config.yml` |
| Dotfile | `dotfiles/lazygit/.config/lazygit/config.yml` |

### Tmux

Tmux uses **Oh My Tmux** plus a local custom theme.

| Component | Value |
|-----------|-------|
| Framework | Oh My Tmux |
| Theme | Monokai-style custom colors |
| Default shell | `/bin/zsh` |
| Config location | `$XDG_CONFIG_HOME/tmux/` |
| Dotfiles | `dotfiles/tmux/.config/tmux/` |

---

## Terminal And Editors

### Windows Terminal

Windows Terminal is the native Windows terminal emulator.

| Setting | Value |
|---------|-------|
| Package | `Microsoft.WindowsTerminal` |
| Config source | `windows/dotfiles/WindowsTerminal/settings.json` |
| Config target | `%LOCALAPPDATA%\Packages\Microsoft.WindowsTerminal_8wekyb3d8bbwe\LocalState\settings.json` |
| Profiles | PowerShell and Developer PowerShell for VS 2022 |

### Zed

Zed is the primary graphical editor on Windows and shares its config from the root `dotfiles` directory.

| Setting | Value |
|---------|-------|
| Package | `ZedIndustries.Zed` |
| Theme | Black & Pink |
| Font | Iosevka Nerd Font / Iosevka Nerd Font Mono |
| Base keymap | VS Code |
| Vim mode | Enabled |
| Config source | `dotfiles/zed/.config/zed/` |
| Windows target | `%APPDATA%\Zed` |

Zed is configured for autosave, split diff view, relative line numbers, right-side project/git/outline panels, local Ollama-backed agent models, local Zeta edit predictions, telemetry disabled, and the `opencode` agent server.

### Neovim

Neovim remains the primary terminal editor for Unix-like environments.

| Component | Value |
|-----------|-------|
| Distribution | LazyVim |
| Config location | `$XDG_CONFIG_HOME/nvim/` |
| Dotfile | `dotfiles/nvim/.config/nvim/` |

---

## File Manager

### Yazi

Yazi is the terminal file manager for Unix-like environments and Arch WSL.

| Setting | Value |
|---------|-------|
| Theme | Black & Pink / configured flavor files |
| Config source | `dotfiles/yazi/.config/yazi/config/` |

Dependencies include FFmpeg, 7-Zip, Poppler, resvg, ImageMagick, and Nerd Font symbols for previews and icons.

---

## Development Languages And Runtimes

| Language/Runtime | Tool | Primary Target |
|------------------|------|----------------|
| Python | Python / Python Install Manager | Windows DevTools, Arch WSL |
| Go | `go` | Arch WSL |
| Rust | `rustup` | Windows DevTools, Arch WSL |
| JavaScript/TypeScript | `nvm`, Node.js LTS | Arch WSL |
| Java | `jdk-openjdk`, Maven | Arch WSL |
| C/C++ | LLVM, Visual Studio Build Tools, Make, CMake | Windows DevTools, Arch WSL |
| Containers | Docker Desktop | Windows DevTools |
| Local AI | Ollama | Windows supplementary, Zed integration |

---

## Shared Non-Windows Package Baseline

These packages are intended for Unix-like targets that receive the full shared toolchain, currently the optional Arch WSL setup. Ubuntu Server intentionally installs only a minimal shell baseline.

### Shell And Dotfiles

- Git
- curl
- wget
- rsync
- GNU Stow
- Zsh
- Oh My Zsh
- zsh autosuggestions
- zsh syntax highlighting
- custom zsh theme and plugin
- tar, unzip, zip, xz, file
- fontconfig / Nerd Font support

### Terminal And Editors

- Neovim
- tmux
- Yazi
- Lazygit

### CLI Tools

- ripgrep
- fd
- fzf
- zoxide
- eza
- bat
- jq
- fastfetch
- btop
- tokei
- opencode

### Programming Languages And Build Tools

- Python
- Go
- Rustup
- NVM and Node.js LTS
- OpenJDK
- Maven
- LLVM
- Make
- CMake
- base-devel on Arch

### Media And File Tooling

- FFmpeg
- 7-Zip
- Poppler
- resvg
- ImageMagick

---

## Platform-Specific: Ubuntu Server

Ubuntu Server is deliberately minimal and only installs the zsh/Oh My Zsh shell setup.

### Package Manager: apt

See `ubuntu-server/install.sh` for the complete installation script.

### Installed Packages

| Category | Packages |
|----------|----------|
| Core | `ca-certificates`, `curl`, `git`, `zsh` |

### Installed Configuration

- Installs Oh My Zsh if missing.
- Installs `zsh-autosuggestions` and `zsh-syntax-highlighting`.
- Copies the shared `blacknpink` theme and `inaya` plugin into Oh My Zsh custom directories.
- Backs up an unmanaged `~/.zshrc`, then installs `dotfiles/zsh/.zshrc`.
- Adds zsh to `/etc/shells` when needed and sets it as the default shell.

### Not Included

The Ubuntu Server installation does not include GNU Stow, development runtimes, tmux, Neovim, Yazi, Lazygit, extra CLI tools, GUI apps, Docker, Ollama, or creative applications.

---

## Platform-Specific: Windows

Windows uses Winget for packages and direct-copy dotfile installation. The installer prompts for optional package groups so a run can stay minimal or install the broader workstation setup.

### Package Manager: Winget

See `windows/install.ps1` for the complete installation script.

### Core Packages

Installed from `windows/dotfiles/winget/packages.json`:

| Package Identifier | Purpose |
|--------------------|---------|
| `7zip.7zip` | Archive tooling |
| `Git.Git` | Version control |
| `Microsoft.PowerShell` | PowerShell Core |
| `Microsoft.WindowsTerminal` | Terminal emulator |
| `Microsoft.WSL` | Windows Subsystem for Linux |
| `JanDeDobbeleer.OhMyPosh` | Prompt renderer |
| `ZedIndustries.Zed` | Graphical editor |
| `Microsoft.PowerToys` | Windows productivity utilities |

### Optional Package Groups

| Group | Package File | Contents |
|-------|--------------|----------|
| DevTools | `packages_devtools.json` | Rustup, LLVM, Visual Studio Build Tools, Python Install Manager, Docker Desktop |
| Art | `packages_art.json` | Blender, Krita, Kdenlive, Audacity, OBS Studio, MuseScore |
| Supplementary | `packages_supplementary.json` | Ollama, Handy, VirtualBox, LibreOffice, Windhawk |
| Arch WSL | `setup-arch-wsl.ps1` | Fresh Arch Linux WSL distro with shared dotfiles and Linux tools |

### Installed Configuration

- PowerShell profile is copied to `$PROFILE` and unblocked.
- Oh My Posh theme is copied beside the PowerShell profile.
- Zed config is copied from `dotfiles/zed/.config/zed/` to `%APPDATA%\Zed`.
- Ollama models are pulled only when the supplementary group was installed and `ollama` is available.
- Windows Terminal settings are backed up and copied into the packaged Windows Terminal profile location.
- AutoHotkey scripts are copied to `%USERPROFILE%\AutoHotkey`, and `myconfig.exe` is added to Startup when present.
- `PSReadLine` is installed for the current user when missing.
- Iosevka is installed through `oh-my-posh font install Iosevka` when Oh My Posh is available.
- LLVM is added to PATH when the DevTools group was installed and LLVM exists.
- `RegistryPreferences.reg` is imported.
- Taskbar auto-hide is enabled.
- Shared/default desktop items are moved to the current user's desktop and removed from shared desktop locations.

### Windows Dotfiles

| Package | Description | Target |
|---------|-------------|--------|
| `PowerShell` | Profile and Oh My Posh theme | `$PROFILE` and profile directory |
| `WindowsTerminal` | Windows Terminal settings | `%LOCALAPPDATA%\Packages\Microsoft.WindowsTerminal_8wekyb3d8bbwe\LocalState\settings.json` |
| `AutoHotkey` | Personal AutoHotkey executable/script | `%USERPROFILE%\AutoHotkey` and Startup shortcut |
| `WindHawk` | Windhawk taskbar styling config | Used with optional Windhawk install |
| `winget` | Core and optional package manifests | Imported by `windows/install.ps1` |

---

## Platform-Specific: Arch WSL

Arch WSL is an optional Windows installer path. It creates a fresh `archlinux` distro and intentionally unregisters an existing distro with the same name before reinstalling.

### Setup Phases

| Phase | Behavior |
|-------|----------|
| Root bootstrap | Sets root password to `root`, initializes pacman keys, updates packages, installs base tools, writes initial `/etc/wsl.conf` |
| User setup | Creates a user named after the Windows user, enables wheel sudo, grants passwordless sudo, sets default user, generates `en_US.UTF-8` locale |
| User packages and dotfiles | Installs Rust stable, builds `paru`, installs packages, configures Git for Windows SSH, installs Oh My Zsh, syncs and stows selected dotfiles, installs zsh plugins, installs Node.js LTS |
| Shell enforcement | Sets and verifies zsh as the WSL user's default shell |

### Arch Package Set

The Arch WSL setup installs packages through `pacman` and `paru`, including `base-devel`, `rustup`, `zsh`, `rsync`, `stow`, `wsl2-ssh-agent`, `ripgrep`, `go`, `yazi-git`, `ffmpeg`, `7zip`, `jq`, `poppler`, `fd`, `fzf`, `bat`, `zoxide`, `resvg`, `imagemagick`, `eza`, `llvm`, `nvm`, `python`, `fastfetch`, `lazygit`, `jdk-openjdk`, `maven`, `make`, `cmake`, `btop`, `tokei`, and `opencode`.

### Synced Dotfiles

Arch WSL syncs and stows these shared dotfile packages from the Windows-accessible repo path:

- `zsh`
- `yazi`
- `lazygit`

---

## Dotfiles Summary

### Repository Structure

```text
myconfig/
├── README.md
├── SPECS.md
├── bootstrap.sh
├── bootstrap.ps1
├── dotfiles/
│   ├── hermes/
│   ├── hyfetch/
│   ├── lazygit/
│   ├── nvim/
│   ├── qbt-search/
│   ├── tmux/
│   ├── wallpaper/
│   ├── yazi/
│   ├── zed/
│   └── zsh/
├── ubuntu-server/
│   ├── install.sh
│   └── uninstall.sh
└── windows/
    ├── install.ps1
    ├── setup-arch-wsl.ps1
    ├── uninstall.ps1
    ├── RegistryPreferences.reg
    └── dotfiles/
        ├── AutoHotkey/
        ├── PowerShell/
        ├── WindHawk/
        ├── WindowsTerminal/
        └── winget/
```

### Shared Dotfiles

| Package | Description | Primary Target |
|---------|-------------|----------------|
| `hermes` | Hermes config | `$XDG_CONFIG_HOME/hermes/` |
| `hyfetch` | Hyfetch config | `$XDG_CONFIG_HOME/hyfetch.json` |
| `lazygit` | Lazygit theme/config | `$XDG_CONFIG_HOME/lazygit/` |
| `nvim` | Neovim config | `$XDG_CONFIG_HOME/nvim/` |
| `qbt-search` | qBittorrent search plugins | Application-specific search plugin directory |
| `tmux` | Tmux config | `$XDG_CONFIG_HOME/tmux/` |
| `wallpaper` | Wallpaper assets | Wallpaper directory |
| `yazi` | Yazi config and flavor | `$XDG_CONFIG_HOME/yazi/` |
| `zed` | Zed settings, keymap, and theme | `%APPDATA%\Zed` on Windows |
| `zsh` | `.zshrc`, Oh My Zsh theme, and custom plugin | Home directory and Oh My Zsh custom paths |

### Installation Model

| Target | Dotfile Strategy |
|--------|------------------|
| Ubuntu Server | Direct copy of zsh files only |
| Windows | Direct copy of Windows configs plus Zed shared config |
| Arch WSL | Sync selected shared packages, remove conflicting target files, then `stow --restow` |
