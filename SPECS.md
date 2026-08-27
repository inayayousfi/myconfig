# Configuration Specifications

> A modular configuration bank for CachyOS, Ubuntu Server, Windows Workstation, and Arch WSL.

This document describes the current setup scripts, package groups, dotfiles, and platform-specific behavior in this repository.

---

## Table of Contents

- [Bootstrap Flow](#bootstrap-flow)
- [Shared Linux Installer](#shared-linux-installer)
- [Shell](#shell)
- [CLI Tools](#cli-tools)
- [Terminal And Editors](#terminal-and-editors)
- [File Manager](#file-manager)
- [Development Languages And Runtimes](#development-languages-and-runtimes)
- [Shared Non-Windows Package Baseline](#shared-non-windows-package-baseline)
- [Platform-Specific: Ubuntu Server](#platform-specific-ubuntu-server)
- [Platform-Specific: CachyOS](#platform-specific-cachyos)
- [Platform-Specific: Windows Workstation](#platform-specific-windows-workstation)
- [Platform-Specific: Arch WSL](#platform-specific-arch-wsl)
- [Dotfiles Summary](#dotfiles-summary)

---

## Bootstrap Flow

The root bootstrap scripts download the latest GitHub release when available, fall back to the main branch when needed, stage the repository under `~/.setup-config`, back up any previous staged install, and hand off to the platform installer.

| Platform            | Bootstrap       | Installer                         |
| ------------------- | --------------- | --------------------------------- |
| Ubuntu Server       | `bootstrap.sh`  | `ubuntu-server/install.sh`        |
| CachyOS             | `bootstrap.sh`  | `cachyos/install.sh`              |
| Windows Workstation | `bootstrap.ps1` | `windows-workstation/install.ps1` |

### Linux Bootstrap

`bootstrap.sh` supports the `cachyos` target. It also supports `ubuntu`, `linux`, and `server` as Ubuntu Server aliases. Interactive mode detects CachyOS and apt-based Linux systems.

The Linux bootstrap ensures `curl` and `unzip` exist before downloading the archive.

### Windows Bootstrap

`bootstrap.ps1` downloads and validates the ZIP archive, extracts with `Expand-Archive` or a .NET fallback, unblocks PowerShell files, and runs `windows-workstation/install.ps1` from the staged repository.

---

## Shared Linux Installer

`linux/install.sh` runs one fixed profile selected by a platform entry point. Capability modules are private and cannot be selected from the public bootstrap command.

| Profile         | Package adapter            | Modules                                     |
| --------------- | -------------------------- | ------------------------------------------- |
| `cachyos`       | Arch (`pacman` and `paru`) | Complete profile with Axidev OSK and Kanata |
| `arch-wsl`      | Arch (`pacman` and `paru`) | Complete profile with WSL integrations      |
| `ubuntu-server` | apt                        | Base, Zsh, and Zsh dotfiles                 |

Modules request logical package identifiers. `linux/registry/packages.sh` maps each identifier to an exact package source and name. A target override can replace either value, such as `fd` becoming `fd-find` on apt. Arch User Repository packages use the explicit `aur:` source. Unsupported sources and missing mappings stop the profile.

The shared installer validates sudo once before package preparation and refreshes that credential every 60 seconds until the profile exits. Long package builds therefore do not ask for the same password again. The refresh process is stopped on both successful and failed exits.

Linux profiles copy selected packages into `~/dotfiles`, back up the previous tree, back up conflicting home files, and run GNU Stow. CachyOS selects `zsh`, `yazi`, `lazygit`, `hunk`, `ai`, `herdr`, `nvim`, `opencode`, `ghostty`, `kanata`, `kanata-kde`, and `kde-plasma`. Arch WSL selects the first eight shared packages. Ubuntu Server selects only `zsh`.

The complete profiles configure OpenSSH as a system service that listens on all IPv4 and IPv6 interfaces and allows only the current user. They also install Tailscale as a system service. The installer validates the SSH daemon configuration before enabling and restarting it, but leaves authentication policy and network perimeter security at OpenSSH and system defaults.

---

## Shell

### Zsh

The Unix-like shell is **Zsh** with **Oh My Zsh**.

| Component       | Value                  |
| --------------- | ---------------------- |
| Shell           | `zsh`                  |
| Framework       | Oh My Zsh              |
| Theme           | `blacknpink`           |
| Config location | `~/.zshrc`             |
| Custom files    | `~/.oh-my-zsh/custom/` |

#### Plugins

| Plugin                    | Source                              | Description                                  |
| ------------------------- | ----------------------------------- | -------------------------------------------- |
| `git`                     | Built-in                            | Git aliases and completions                  |
| `vi-mode`                 | Built-in                            | Vi keybindings in shell                      |
| `zsh-autosuggestions`     | `zsh-users/zsh-autosuggestions`     | Fish-like autosuggestions                    |
| `zsh-syntax-highlighting` | `zsh-users/zsh-syntax-highlighting` | Syntax highlighting                          |
| `inaya`                   | Custom dotfile                      | Personal aliases, functions, and environment |

#### Custom Theme: `blacknpink`

Location: `dotfiles/zsh/.oh-my-zsh/custom/themes/blacknpink.zsh-theme`

The theme is based on Oh My Zsh's bundled `refined` prompt and maps prompt colors to the shared Black & Pink palette.

#### Custom Plugin: `inaya`

Location: `dotfiles/zsh/.oh-my-zsh/custom/plugins/inaya/inaya.plugin.zsh`

Key environment defaults include XDG paths, `nvim` as editor, `xterm-256color`, UTF-8 locale, and vi-mode cursor support.

Key aliases and functions include `nvim` shortcuts, modern CLI replacements for `ls`, `find`, and `grep`, `lg` for Lazygit, `ff` for Fastfetch, `y` for Yazi directory handoff, `pf` for fuzzy file opening, `mkd`, `reload-zsh`, `update`, and `cleanup`.

### PowerShell

Windows uses **PowerShell Core** with **Oh My Posh**.

| Component | Value                                                                      |
| --------- | -------------------------------------------------------------------------- |
| Shell     | PowerShell Core (`pwsh`)                                                   |
| Prompt    | Oh My Posh                                                                 |
| Theme     | `black-pink.omp.json`                                                      |
| Profile   | `windows-workstation/dotfiles/PowerShell/Microsoft.PowerShell_profile.ps1` |
| Modules   | `PSReadLine`, `Terminal-Icons`                                             |

PowerShell profile features include vi mode keybindings, history predictions, cursor shape changes for insert and normal modes, terminal icons, and Windows Terminal integration helpers.

---

## CLI Tools

### Core Utilities

| Tool             | Purpose                                  | Replaces      |
| ---------------- | ---------------------------------------- | ------------- |
| `eza`            | Modern ls with icons and git integration | `ls`          |
| `fd`             | Fast, user-friendly find                 | `find`        |
| `ripgrep` (`rg`) | Fast recursive grep                      | `grep`        |
| `bat`            | Cat with syntax highlighting             | `cat`         |
| `fzf`            | Fuzzy finder                             | -             |
| `zoxide`         | Smart cd with frecency                   | `cd`          |
| `btop`           | Resource monitor                         | `top`, `htop` |
| `fastfetch`      | System information display               | `neofetch`    |
| `jq`             | JSON processor                           | -             |
| `tokei`          | Code statistics                          | -             |

### Git Tools

| Tool      | Purpose             |
| --------- | ------------------- |
| `git`     | Version control     |
| `lazygit` | Terminal UI for git |

### Lazygit

Lazygit uses a shared Black & Pink theme.

| Setting                 | Value                                         |
| ----------------------- | --------------------------------------------- |
| Config location         | `$XDG_CONFIG_HOME/lazygit/config.yml`         |
| Windows config location | `%LOCALAPPDATA%\lazygit\config.yml`           |
| Dotfile                 | `dotfiles/lazygit/.config/lazygit/config.yml` |

---

## Terminal And Editors

### Windows Terminal

Windows Terminal is the native Windows terminal emulator.

| Setting       | Value                                                                                      |
| ------------- | ------------------------------------------------------------------------------------------ |
| Package       | `Microsoft.WindowsTerminal`                                                                |
| Config source | `windows-workstation/dotfiles/WindowsTerminal/settings.json`                               |
| Config target | `%LOCALAPPDATA%\Packages\Microsoft.WindowsTerminal_8wekyb3d8bbwe\LocalState\settings.json` |
| Profiles      | PowerShell and Developer PowerShell for VS 2022                                            |

### Neovim

Neovim is the primary editor everywhere, including Windows, where `$EDITOR`/`$VISUAL` shell out to the nvim instance running inside WSL.

| Component | Value |
| --- | --- |
| Plugin manager | native `vim.pack` |
| Config location | `$XDG_CONFIG_HOME/nvim/` |
| Dotfile | `dotfiles/nvim/.config/nvim/` |
| Windows wrapper | `windows-workstation/dotfiles/bin/nvim.cmd`, installed to `%LOCALAPPDATA%\Programs\bin` and added to user PATH; shells into WSL via `wsl.exe -u <user> -- zsh -ic nvim`, converting file args with `wslpath` |

`dotfiles/zed/` is kept in the repo for reference but is no longer installed or referenced by any install script.

---

## File Manager

### Yazi

Yazi is the terminal file manager for Unix-like environments and Arch WSL.

| Setting       | Value                                  |
| ------------- | -------------------------------------- |
| Theme         | Black & Pink / configured flavor files |
| Config source | `dotfiles/yazi/.config/yazi/config/`   |

Dependencies include FFmpeg, 7-Zip, Poppler, resvg, ImageMagick, and Nerd Font symbols for previews and icons.

---

## Development Languages And Runtimes

| Language/Runtime      | Tool                                         | Primary Target             |
| --------------------- | -------------------------------------------- | -------------------------- |
| Python                | Python / Python Install Manager              | Windows DevTools, Arch WSL |
| Go                    | `go`                                         | Arch WSL                   |
| Rust                  | `rustup`                                     | Windows DevTools, Arch WSL |
| JavaScript/TypeScript | `bun`                                        | Arch WSL                   |
| Java                  | `jdk-openjdk`, Maven                         | Arch WSL                   |
| C/C++                 | LLVM, Visual Studio Build Tools, Make, CMake | Windows DevTools, Arch WSL |
| Containers            | Docker Desktop                               | Windows DevTools           |

---

## Shared Non-Windows Package Baseline

These packages form the complete CachyOS and Arch WSL profiles. Ubuntu Server intentionally installs only a minimal shell baseline.

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
- Yazi
- Lazygit
- Herdr

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

### Programming Languages And Build Tools

- Python
- Go
- Rustup
- Bun
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

| Category | Packages                                |
| -------- | --------------------------------------- |
| Core     | `ca-certificates`, `curl`, `git`, `zsh` |

### Installed Configuration

- Installs Oh My Zsh if missing.
- Installs `zsh-autosuggestions` and `zsh-syntax-highlighting`.
- Copies the shared `blacknpink` theme and `inaya` plugin into Oh My Zsh custom directories.
- Backs up an unmanaged `~/.zshrc`, then installs `dotfiles/zsh/.zshrc`.
- Adds zsh to `/etc/shells` when needed and sets it as the default shell.

### Not Included

The Ubuntu Server installation does not include GNU Stow, development runtimes, Neovim, Yazi, Lazygit, extra CLI tools, GUI apps, Docker, or creative applications.

---

## Platform-Specific: CachyOS

CachyOS uses the complete shared Linux profile after the graphical operating-system installer finishes. The profile installs development packages, Ghostty, Axidev OSK, Kanata, its independent KDE tray, Handy offline dictation, and the KDE Plasma desktop configuration, configures the OpenSSH service, sets Zsh as the default shell, deploys shared dotfiles, configures agent tools, and offers GitHub and Tailscale authentication.

The installer does not change sudoers, locale, kernel, drivers, or power settings. It installs OpenSSH, generates missing host keys, writes the shared listener policy, and enables the system service. Tailscale installs its system service separately.

### Ghostty

The CachyOS profile installs Ghostty before removing installed copies of Kitty, Alacritty, WezTerm, and Konsole. When installed, it also removes the metadata-only `kde-utilities-meta` package that would otherwise block Konsole removal; the KDE applications grouped by that package remain installed. It leaves unused dependencies installed and does not change the desktop's default-terminal setting. The Ghostty module does not run on Arch WSL or Ubuntu Server.

### Axidev OSK

The CachyOS profile installs the latest published [Axidev OSK](https://github.com/axide-dev/axidev-osk) Linux release. A first run downloads the project's lifecycle installer; later profile runs use `axidev-osk-install upgrade`. The lifecycle installer keeps the active payload under `/opt/axidev-osk`, retains one rollback payload, and exposes the application through `/usr/local/bin/axidev-osk`.

The profile installs the Arch host dependencies for Python, PySide6, Qt Wayland, LayerShellQt, libinput, systemd, and libxkbcommon. The downloaded lifecycle installer verifies the payload against the release checksum manifest before activation. The installer and checksum manifest come from the same GitHub release, so this check detects download corruption but does not independently authenticate the publisher.

After installation, the profile uses the Axidev OSK command line to configure the `uinput` kernel module, udev rule, shared input group, current-user membership, and desktop-session autostart. It then connects the command's login-manager menu directly to the terminal and configures greeter startup for the selected supported manager. The menu supports Plasma Login Manager, greetd, and LightDM. Log out and back in after a new group membership is added; restart the selected login manager or reboot to activate greeter startup.

This module runs only for CachyOS. Arch WSL and Ubuntu Server neither install nor configure Axidev OSK.

### Kanata

The CachyOS profile installs `kanata-bin` from the Arch User Repository and stows one portable configuration with Off, Home Row, Disabled, and Valo layers. Off is the startup layer. Every layer maps `F17` and `F18` to repeated wheel up and wheel down, and maps `F19` to `Meta+W`. All 18 tap-hold mappings use the AutoHotkey configuration's 400-millisecond hold threshold and disable Kanata's tap-repress window.

The Kanata module calls the reusable named input-access helper. The helper loads `uinput`, persists that module through `/etc/modules-load.d/myconfig-kanata.conf`, creates `input` and `uinput` groups when needed, adds the current user to both, and writes `/etc/udev/rules.d/99-myconfig-kanata.rules`. This lets the user service read and emit input without root. It also lets every other process running as that user read raw input and inject events. New group membership becomes active only after logout and login; until then, the installer enables the service without claiming it started successfully.

`myconfig-kanata.service` runs one Kanata process and binds its layer-control protocol to `127.0.0.1:5829`. The separate Kanata KDE module installs a PySide6 tray and `myconfig-kanata-tray.service`. On CachyOS, the tray service becomes the startup entry point: it starts Kanata first, keeps its icon hidden until Kanata reports an active layer, reconnects after a Kanata restart, and stops Kanata whenever the tray unit stops. A tray failure stops Kanata before systemd restarts the pair. The tray shows mutually exclusive Home Row, Disabled, Valo, and Off actions, and Quit stops the pair. The KDE module assigns `Meta+W` to KWin Overview. Neither the generic Kanata module nor its dotfiles depend on KDE, and the existing KDE Plasma module does not depend on Kanata.

### Handy

The CachyOS profile installs `handy-bin` from the Arch User Repository and runs Handy as `myconfig-handy.service` during the graphical session. Handy starts hidden and restarts after failures. The service starts after the Kanata KDE tray when both are part of the login transaction.

During module configuration and before every service start, `myconfig-handy-configure` updates `~/.config/com.pais.handy/settings_store.json` atomically. It preserves unrelated Handy settings while enforcing the direct `handy_keys` keyboard backend, push-to-talk mode, and the modifier-only `ctrl+shift` transcription binding. The backend treats left and right modifiers alike, so either Ctrl key held with either Shift key starts recording and releasing the combination stops it.

Handy calls the same reusable input-access helper independently with its own name. This writes `/etc/modules-load.d/myconfig-handy.conf` and `/etc/udev/rules.d/99-myconfig-handy.rules`; Handy does not depend on the Kanata module. When both applications are installed, their equivalent named rules coexist. The file names show ownership, but the Linux permissions remain user-wide rather than process-specific.

### KDE Plasma

The CachyOS profile requires KDE Plasma 6.7 through the latest 6.x release. Earlier versions lack the required per-screen virtual-desktop behavior, and KDE Plasma 7 is rejected until its panel scripting interface is deliberately validated. The version boundary is isolated in the KDE Plasma module so support can be extended without changing the layout.

The profile installs Iosevka Nerd Font and applies the Black & Pink KDE color scheme to KDE Plasma and KDE applications through Breeze. It uses the root stylesheet's black surfaces, light text, pink accent, muted inactive text, semantic colors, and Iosevka typography. A narrow `blacknpink` Plasma theme inherits Breeze and overrides only its panel background: the floating dock keeps Breeze's shape and left margin while its trailing content margin is reduced from 8 to 4 logical pixels. KDE Plasma's adaptive panel opacity remains enabled, and the profile does not use the experimental Union styling engine.

At each KDE Plasma login, a desktop script reconciles two module-owned panels on every connected display. The full-width top panel has a text-only Overview button on the left, two expanding spacers around a system-locale Digital Clock, then the System Tray and separate icon-only Session and Power menus. Session contains Lock, Log Out, and Switch User. Power contains Restart, Shut Down, Sleep, and Hibernate. Unsupported actions remain visible but disabled. The centered bottom application dock has an Application Launcher and one Icons-only Task Manager. It starts with no pinned applications, supports the normal **Pin to Task Manager** action, and shows only windows from its own display and that display's current virtual desktop.

Both panels use automatic hiding and adaptive opacity. MyConfig Plasma Panels, a KWin script, gives the top and bottom edges an 8-logical-pixel inward activation zone across each display. A matching panel appears as an overlay and returns to native automatic hiding 400 milliseconds after the pointer leaves both the panel and its activation zone; an open panel popup postpones hiding. The top-panel height is 4.5 percent of its display's logical height. The floating dock uses KDE Plasma's 8-pixel floating margin, a height of 6 percent, and a width that follows its launcher and visible tasks. Its responsive dimensions are recalculated at login and when its content changes.

The first layout application backs up `~/.config/plasma-org.kde.plasma.desktop-appletsrc`, builds its replacement panels, and removes the initial panels only after replacement construction succeeds. Later logins update responsive geometry, create missing managed panels, and replace panels from an older layout version while carrying manual task-manager pins into replacement docks. MyConfig Plasma Panels also starts the same reconciler immediately when KWin reports a display change, with one delayed retry for Plasma's output update. They preserve unrelated panels, widget state on retained panels, and managed panels for temporarily disconnected displays. If KDE Plasma is not running during installation, the profile installs the configuration and defers panel creation until the next KDE Plasma login.

### Virtual Machine Testing

`vm/cachyos.sh` provides an interactive QEMU and KVM test environment for the complete CachyOS profile. The `install` action discovers the current official Desktop ISO, downloads its adjacent SHA-256 file, verifies the image, and boots the graphical installer with UEFI. The VM defaults to 8 GiB RAM, 6 virtual CPUs, and a 100 GiB sparse disk. QEMU uses its local SPICE display and QXL graphics device, installs the Arch `virt-viewer` host package when needed, and exposes the guest-agent channel so viewer resizing requests a matching CachyOS resolution instead of scaling a low-resolution framebuffer.

The `seal` action requires the default unencrypted CachyOS Btrfs layout. It installs the Arch host packages `linux`, `libguestfs`, and `guestfs-tools` when the libguestfs commands or helper kernel are missing. WSL does not boot this Arch kernel; libguestfs uses it only for offline disk access. Sealing explicitly mounts the `@` and `@home` subvolumes so installation snapshots are not mistaken for separate operating systems. It then detects the single desktop user, enables the installed OpenSSH service offline, injects a dedicated VM key, and adds a one-time boot service that allows TCP port 22 through UFW before SSH starts. It boots the manually installed system, forwards localhost port 2222 to guest SSH, and waits for SSH. After QEMU exits and the user confirms the desktop worked, sealing copies that user's QXL output layout to the Plasma Login greeter inside the VM so login-screen input can be tested at the same crisp resolution. It then marks the system as the clean base. The `run` action creates or reuses a copy-on-write test disk backed by that sealed base. The `run-multi-display` action uses the same disposable system with two QXL outputs for KDE Plasma per-screen checks. The `reset` action removes only the test disk and its UEFI variables, leaving the installed base, SSH state, and ISO cache intact.

During test boots, QEMU exposes the current repository through a writable 9p mount tagged `myconfig`. This makes unstaged and untracked host files available to the guest without a release, but guest root can also modify or delete the checkout as the host user running QEMU. The harness waits for SSH, mounts the share at `/mnt/myconfig`, and starts `cachyos/install.sh` in an interactive terminal. Installation and sealing boots do not receive the repository device. VM state and its dedicated SSH key live under `${XDG_DATA_HOME:-$HOME/.local/share}/myconfig/cachyos-vm`; ISO files live under `${XDG_CACHE_HOME:-$HOME/.cache}/myconfig/cachyos-vm`.

The VM covers installation flow, packages, login-manager integration, desktop startup, and virtual input. It does not validate physical GPU, touch-screen, firmware, device-driver, or native-performance behavior. The complete interactive procedure is documented in [`vm/README.md`](vm/README.md).

---

## Platform-Specific: Windows Workstation

Windows uses Winget for packages and direct-copy dotfile installation. The installer prompts for optional package groups so a run can stay minimal or install the broader workstation setup.

### Package Manager: Winget

See `windows-workstation/install.ps1` for the complete installation script.

### Core Packages

Installed from `windows-workstation/dotfiles/winget/packages.json`:

| Package Identifier          | Purpose                        |
| --------------------------- | ------------------------------ |
| `7zip.7zip`                 | Archive tooling                |
| `Git.Git`                   | Version control                |
| `Microsoft.PowerShell`      | PowerShell Core                |
| `Microsoft.WindowsTerminal` | Terminal emulator              |
| `Microsoft.WSL`             | Windows Subsystem for Linux    |
| `JanDeDobbeleer.OhMyPosh`   | Prompt renderer                |
| `Microsoft.PowerToys`       | Windows productivity utilities |

### Optional Package Groups

| Group | Package File | Contents |
| --- | --- | --- |
| DevTools | `packages_devtools.json` | Rustup, LLVM, Visual Studio Build Tools, Python Install Manager, Docker Desktop |
| Art | `packages_art.json` | Blender, Krita, Kdenlive, Audacity, OBS Studio, MuseScore |
| Supplementary | `packages_supplementary.json` | Handy, VirtualBox, LibreOffice |
| Arch WSL | `setup-arch-wsl.ps1` | Fresh Arch Linux WSL distro named `<windows-hostname>-subsystem`, with shared dotfiles and Linux tools |

### Installed Configuration

- PowerShell profile is copied to `$PROFILE` and unblocked.
- Oh My Posh theme is copied beside the PowerShell profile.
- nvim.cmd wrapper is copied to `%LOCALAPPDATA%\Programs\bin` and that directory is added to user PATH, so `nvim` (and `$EDITOR`/`$VISUAL`) shell into WSL nvim.
- Windows Terminal settings are backed up and copied into the packaged Windows Terminal profile location.
- AutoHotkey scripts are copied to `%USERPROFILE%\AutoHotkey`, and `myconfig.exe` is added to Startup when present.
- On the Arch WSL path only, `%UserProfile%\.wslconfig` is written and a WSL logon shortcut is added to Startup. See [Instance Persistence](#instance-persistence).
- `PSReadLine` is installed for the current user when missing.
- Iosevka is installed through `oh-my-posh font install Iosevka` when Oh My Posh is available.
- LLVM is added to PATH when the DevTools group was installed and LLVM exists.
- `RegistryPreferences.reg` is imported.
- Taskbar auto-hide is enabled.
- Shared/default desktop items are moved to the current user's desktop and removed from shared desktop locations.

### Windows Dotfiles

| Package | Description | Target |
| --- | --- | --- |
| `PowerShell` | Profile and Oh My Posh theme | `$PROFILE` and profile directory |
| `WindowsTerminal` | Windows Terminal settings | `%LOCALAPPDATA%\Packages\Microsoft.WindowsTerminal_8wekyb3d8bbwe\LocalState\settings.json` |
| `AutoHotkey` | Personal AutoHotkey executable/script | `%USERPROFILE%\AutoHotkey` and Startup shortcut |
| `winget` | Core and optional package manifests | Imported by `windows-workstation/install.ps1` |

---

## Platform-Specific: Arch WSL

Arch WSL is an optional Windows installer path. It installs the online `archlinux` image under the generated name `<windows-hostname>-subsystem`. The installer uses that same value as the Linux hostname and intentionally unregisters an existing distro with that name before reinstalling. Passing `-Distro` overrides both names.

### Distribution Identity

The online source name, WSL registration name, and Linux hostname have separate roles:

| Identity         | Value                          | Purpose                                                 |
| ---------------- | ------------------------------ | ------------------------------------------------------- |
| Online source    | `archlinux`                    | Selects the Arch Linux image from `wsl --list --online` |
| WSL registration | `<windows-hostname>-subsystem` | Identifies the installed instance to `wsl.exe -d`       |
| Linux hostname   | `<windows-hostname>-subsystem` | Identifies the running Arch environment inside Linux    |

For example, Windows host `INTERNET-GYAL-TERMINAL` produces the WSL registration and Linux hostname `internet-gyal-terminal-subsystem`.

### Setup Phases

| Phase | Behavior |
| --- | --- |
| Root bootstrap | Sets root password to `root`, initializes pacman keys, updates packages, installs base tools, writes initial `/etc/wsl.conf` with systemd and the generated hostname |
| User setup | Creates a user named after the Windows user, enables wheel sudo, grants passwordless sudo, enables lingering so user services survive with no shell open, sets the default user and hostname, generates `en_US.UTF-8` locale |
| Shared Arch WSL profile | Calls `linux/install.sh arch-wsl` for packages, the OpenSSH service, shell tools, dotfiles, agents, Windows Git SSH configuration, and the environment inventory |
| Shell enforcement | Sets and verifies zsh as the WSL user's default shell |
| Instance persistence | Writes `%UserProfile%\.wslconfig` with `instanceIdleTimeout=-1` and `vmIdleTimeout=-1`, adds a hidden logon shortcut that boots the distro, then restarts the instance under the new timeouts |

### Background Service Stack

`user@<uid>.service` is not a custom application service. It is systemd's standard template for starting one service manager per Linux user. The system systemd process runs it as the matching user, and that process then manages the user's background services.

```text
Windows logon shortcut
  -> WSL distribution
    -> system systemd (PID 1)
      -> sshd.service
      -> user@<uid>.service
        -> user systemd manager
          -> user D-Bus socket at /run/user/<uid>/bus
```

The template belongs to the Arch systemd package, normally under `/usr/lib/systemd/system/user@.service`. An instance such as `user@1000.service` means the template is running for Linux user ID `1000`. The repo enables lingering so this manager survives after the user's last shell closes. Lingering cannot keep the surrounding WSL distribution alive.

### Instance Persistence

The OpenSSH server runs as a system unit, but systemd cannot stop WSL from tearing the instance down. Two independent timeouts do that, and both must be disabled:

| Key                   | Section     | Default  | Effect                                         |
| --------------------- | ----------- | -------- | ---------------------------------------------- |
| `instanceIdleTimeout` | `[general]` | 15000 ms | Stops the distro instance. Added in WSL 2.4.4. |
| `vmIdleTimeout`       | `[wsl2]`    | 60000 ms | Stops the utility VM.                          |

Setting only `vmIdleTimeout` leaves the instance timeout at its default, so the distro still stops 15-20 seconds after the last terminal closes.

Disabling the timeouts stops WSL shutting the instance down, but nothing starts it either. `myconfig-wsl-autostart.lnk` in the Startup folder supplies that half, running `wsl.exe -d <windows-hostname>-subsystem --exec /bin/true` through a hidden `powershell.exe` because `wsl.exe` is a console program. This keeps the SSH server available after Windows login. `windows-workstation/uninstall.ps1` removes both the shortcut and `.wslconfig`.

### Arch Package Set

The Arch WSL setup installs packages through `pacman` and `paru`, including `base-devel`, `rustup`, `openssh`, `zsh`, `rsync`, `stow`, `wsl2-ssh-agent`, `ripgrep`, `go`, `yazi-git`, `ffmpeg`, `7zip`, `jq`, `poppler`, `fd`, `fzf`, `bat`, `zoxide`, `resvg`, `imagemagick`, `eza`, `llvm`, `bun`, `python`, `fastfetch`, `lazygit`, `jdk-openjdk`, `maven`, `make`, `cmake`, `btop`, `tokei`, `hunk-bin`, `herdr-bin`, `neovim`, `nodejs`, `npm`, `node-gyp`, `opencode`, and `github-cli`.

The shared profile installs the latest Playwright MCP package through Bun, then downloads only its matching Chromium Headless Shell. The `opencode` Stow package owns the shared `opencode.jsonc`, `tui.json`, and `themes/blacknpink.json`: it launches Playwright through `{env:HOME}`, applies the Black & Pink theme, and binds half-page message scrolling to `Ctrl+U` and `Ctrl+D`. Machine-specific MCP servers can live in untracked `~/.config/opencode/config.json`, which OpenCode merges with the tracked runtime file. Existing conflicting configs are backed up but not migrated automatically. The installer validates the merged runtime configuration after Stow. Invalid configuration, browser failures, SSH service failures, and Herdr integration failures stop the profile. Missing GitHub or Tailscale authentication offers an interactive login, or prints the deferred command without failing.

The shared `update()` function updates global Bun packages, then updates Chromium Headless Shell when the Playwright command exists in Bun's global package workspace. Browser update failures produce a warning and do not stop later updates. The same browser step runs after Homebrew updates on macOS.

The generated `~/environment.md` inventories the installed shell, development, media, interoperability, and agent capabilities. The shared `AGENTS.md` points models to this file when they need to inspect available tools.

### Synced Dotfiles

Arch WSL syncs and stows these shared dotfile packages from the Windows-accessible repo path:

- `zsh`
- `yazi`
- `lazygit`
- `hunk`
- `ai`
- `herdr`
- `nvim`
- `opencode`

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
│   ├── ai/
│   ├── hermes/
│   ├── herdr/
│   ├── hyfetch/
│   ├── hunk/
│   ├── kanata/
│   ├── kanata-kde/
│   ├── lazygit/
│   ├── nvim/
│   ├── opencode/
│   ├── qbt-search/
│   ├── wallpaper/
│   ├── yazi/
│   ├── zed/
│   └── zsh/
├── linux/
│   ├── adapters/
│   ├── lib/
│   ├── modules/
│   ├── profiles/
│   ├── registry/
│   └── install.sh
├── cachyos/
│   └── install.sh
├── ubuntu-server/
│   ├── install.sh
│   └── uninstall.sh
└── windows-workstation/
    ├── install.ps1
    ├── setup-arch-wsl.ps1
    ├── uninstall.ps1
    ├── RegistryPreferences.reg
    └── dotfiles/
        ├── AutoHotkey/
        ├── bin/
        ├── PowerShell/
        ├── WindowsTerminal/
        └── winget/
```

### Shared Dotfiles

| Package | Description | Primary Target |
| --- | --- | --- |
| `ai` | Claude Code global instructions, skills, and settings | `~/.claude/` |
| `hermes` | Hermes config | `$XDG_CONFIG_HOME/hermes/` |
| `herdr` | Herdr config | `$XDG_CONFIG_HOME/herdr/` |
| `hyfetch` | Hyfetch config | `$XDG_CONFIG_HOME/hyfetch.json` |
| `hunk` | Hunk diff viewer config and Black & Pink theme | `$XDG_CONFIG_HOME/hunk/` |
| `kanata` | Portable key mappings and user service | `$XDG_CONFIG_HOME/kanata/` and user systemd units |
| `kanata-kde` | Independent KDE tray for Kanata layer selection | `~/.local/bin/` and user systemd units |
| `lazygit` | Lazygit theme/config | `$XDG_CONFIG_HOME/lazygit/` |
| `nvim` | Neovim config | `$XDG_CONFIG_HOME/nvim/` |
| `opencode` | OpenCode runtime, TUI, and Black & Pink theme | `$XDG_CONFIG_HOME/opencode/` |
| `qbt-search` | qBittorrent search plugins | Application-specific search plugin directory |
| `wallpaper` | Wallpaper assets | Wallpaper directory |
| `yazi` | Yazi config and flavor | `$XDG_CONFIG_HOME/yazi/` |
| `zed` | Zed settings, keymap, and theme (unused, kept for reference) | Not installed by any script |
| `zsh` | `.zshrc`, Oh My Zsh theme, and custom plugin | Home directory and Oh My Zsh custom paths |

The `ai` package ships `settings.json` without a `hooks` key on purpose. CachyOS and Arch WSL run `herdr integration install claude` after stowing, and that command merges its own `SessionStart` hook into the file with a home-relative path. Windows Workstation copies only `CLAUDE.md` and `skills/` from this package.

### Installation Model

| Target              | Dotfile Strategy                                                                    |
| ------------------- | ----------------------------------------------------------------------------------- |
| CachyOS             | Back up `~/dotfiles`, copy eleven packages, back up conflicts, then `stow --restow` |
| Ubuntu Server       | Back up `~/dotfiles`, copy Zsh, back up conflicts, then `stow --restow`             |
| Windows Workstation | Direct copy of Windows configs                                                      |
| Arch WSL            | Back up `~/dotfiles`, copy eight packages, back up conflicts, then `stow --restow`  |
