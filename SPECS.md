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
- [Docker](#docker)
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

Linux profiles copy selected packages into `~/dotfiles`, back up the previous tree, back up conflicting home files, and run GNU Stow. CachyOS selects `zsh`, `yazi`, `ai`, `kanata`, `kanata-kde`, `handy`, `kde-plasma`, `emacs`, `phone`, and `pipewire`; the PipeWire configuration downmixes playback to mono without changing capture, and a KDE system-tray toggle enables or disables that downmix. Arch WSL selects `zsh`, `yazi`, and `ai`; it retains the Neovim binary without a managed configuration as its shell editor. Ubuntu Server selects only `zsh`.

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

Key environment defaults include XDG paths, UTF-8 locale, and vi-mode cursor support. CachyOS uses normal-process `emacs` for `EDITOR`, `VISUAL`, `vim`, `vi`, and `v`. Arch WSL and other platforms retain their available Neovim, Vim, or Vi fallback.

Retained aliases and functions include `ff` for Fastfetch, `y` for Yazi directory handoff, `mkd`, `reload-zsh`, `update`, `cleanup`, and agent commands. The migration removes automatic tmux startup, `tx`, `pf`, `lg`, `hd`, `hdc`, `hdb`, zoxide initialization, and aliases that replaced `ls`, `find`, or `rg`.

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

Lazygit and Hunk binaries may remain available, but their dotfiles and shell entry points are archived. Emacs provides the active Git status, staging, history, branch, conflict, and side-by-side review interfaces on CachyOS.

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

### Emacs Workbench

CachyOS uses native Wayland graphical Emacs 31 as a normal application process. KDE launches the distribution desktop entry, whose command is `emacs %F`. Closing the final frame ends Emacs and its child processes without process-confirmation prompts. Killing a terminal buffer likewise stops its process without prompting. The atomic private snapshot under `$XDG_STATE_HOME/myconfig-emacs/` records workspaces, split topology, native buffer assignments, directories, jobs, restart policies, process recipes, and AIPanel source attachments. Reopening restores that structure and starts fresh eligible terminal or attached AIPanel processes in their recorded buffers; terminal screen memory and process-internal state are not restored. Version 1 snapshots are migrated by retaining the selected tab's layout and moving jobs from every old tab into their workspace. Migration to version 7 discards legacy workspace-level AIPanel records because they contain no source entry from which a correct directory can be recovered.

After the user runs `M-x remot-set-password`, graphical Emacs on CachyOS serves a browser terminal rendered by pinned `ghostty-web` 0.4.0 over plain HTTP on TCP port 18080 and a WebSocket pseudo-terminal relay on TCP port 18081. UFW allows those ports only from the private IPv4 ranges `10.0.0.0/8`, `172.16.0.0/12`, and `192.168.0.0/16`. Emacs accepts the password without length or character rules and stores only a salted scrypt verifier in a mode-0600 file under `$XDG_STATE_HOME/myconfig-emacs/`. The browser uses a standard password-manager-compatible form for username `emacs`. Repeated failures receive an increasing global delay. Only one browser controls a fresh `emacsclient -t` frame at a time. Disconnecting immediately destroys that client and frame, while shared buffers, jobs, workspaces, and process state remain in graphical Emacs. Closing the final graphical frame ends Emacs, its listeners, and its child processes even when a browser frame exists. Windows receives the shared source but does not start this GNU/Linux-only terminal because Windows Emacs cannot mix graphical and text frames in one process.

The browser transport is intentionally plaintext. The password, keystrokes, terminal output, and session control can be observed or altered by a device able to intercept local-network traffic. Running `M-x remot-set-password` again replaces the password and disconnects the controller. A forgotten password cannot be recovered from its verifier, but the local command can replace it.

Authored configuration is under `dotfiles/emacs/.config/emacs/`. ELPA packages, Tree-sitter grammars, and Mason packages use `$XDG_DATA_HOME/myconfig-emacs/`; histories and snapshots use `$XDG_STATE_HOME/myconfig-emacs/`; native compilation uses `$XDG_CACHE_HOME/myconfig-emacs/`. The installer backs up a legacy `~/.emacs.d` so stock Emacs discovers `$XDG_CONFIG_HOME/emacs/init.el`.

The workbench hierarchy is workspace, then split. Each workspace owns one split layout, its entry tree, and process restart metadata. The entry tree is authoritative; live Emacs buffers carry no Atelier ownership metadata. Every entry has a storage `kind` that controls persistence and restoration, while reusable workspace services may also have a registered `type` that controls default lookup and derives the buffer name as `*type:workspace*`. The initial registered types are Dired, terminal, and AIPanel. Automatic buffer discovery does not add another entry when that type already exists. Explicit actions may add another typed entry, which receives Emacs's normal `<2>` suffix: `Space f` from Dired and `Space t` from a terminal create another entry, while those commands from another buffer reopen the oldest live entry of that type. Terminals and AIPanel sessions appear as normal workspace entries even though their restart recipes are tracked internally. Each AIPanel is attached to one basic leaf entry by its permanent entry ID, and one source entry may own one panel. Different source entries may run independent panels. Moving a source entry also moves its panel entry; removing the source entry or killing its live buffer stops and removes the panel. Durable workspace-owned file, Dired, and scratch entries are restored after a restart, while transient entries exist only for the current process. Hidden entries remain assigned to their workspace, and `Space b` excludes buffers owned by other workspaces. The full-frame navigator combines visible and hidden entries into one list per workspace and sorts that list by each entry's permanent ID, so use, visibility, titles, and renames do not move rows. Visible entries retain their real split numbers and targets. The navigator also displays known projects and unowned native Emacs buffers. Its cleanup action uses a single-key confirmation before killing all scratch and detached buffers. Selecting a buffer enters its owner workspace. Navigator `x` and `Space x` close the selected entry, stop its process when present, remove the entry and its restart record from the workspace, and close its split when several splits exist. When it was the only visible entry, Atelier shows the most recently used surviving live entry from that workspace, restores a saved entry if none are live, and shows the empty-workspace message only when no entry remains. New splits explicitly open additional typed Dired entries; `Space =` splits right and `Space -` splits below at the current directory. `Space h/j/k/l` moves left/down/up/right. `M-x compile` starts with a blank command.

Evil normal state uses `Space` as the leader, including Dired and Compile buffers. Ghostel terminals and Atelier-owned AIPanel processes use `Ctrl-Escape` to return to normal state; `evil-ghostel` routes plain `Escape` to the terminal application. Ghostel uses libghostty's VT engine and native PTY path, advertises `xterm-256color` for local and remote compatibility, forwards terminal mouse tracking, excludes terminal buffers from editor line highlighting, and uses the Black & Pink 16-color palette formerly configured in Ghostty. Its prebuilt native module downloads without prompting into `$XDG_DATA_HOME/myconfig-emacs/ghostel-module/`. `Space t` opens a terminal, `Space a` toggles the panel attached to the current source, `Space w` opens the navigator, and `Space g g` opens Magit. AIPanel uses a standard Emacs side window dedicated to its process. Without an integration, the standalone `aipan.el` package attaches a panel to the current buffer and owns matching-environment discovery, process startup, context delivery, source cleanup, and panel lifecycle. `aipanel-atelier.el` optionally replaces the buffer attachment with the current basic Atelier entry and connects panel lifecycle to Atelier jobs, entry movement, remote execution, and restoration. Either package can load without the other, and the main configuration loads the adapter only when AIPanel is present. When no process is running, AIPanel lists only configured agents found in the attached source's execution environment, or starts one directly when only one is available. The agent starts with the attached buffer or entry directory as its working directory. File context retains the current line and column and expresses the file path relative to that exact working directory. Local, WSL, POSIX SSH, and Windows SSH sources launch the agent in the matching environment; AIPanel does not silently combine a source from one environment with an agent from another. The Emacs startup adds the conventional `~/.local/bin` directory to its process environment once. OpenCode, Claude Code, Codex, and `fx` are configured by default; `aipanel-agents` controls each executable's flags, compact-mode arguments, project argument, and readiness delay independently. Tree-sitter Auto prompts to install missing parsers and selects compatible `*-ts-mode` modes. `M-x mason-manager` manually installs Emacs-owned language servers; Eglot starts automatically only for commands installed under the Emacs Mason directory.

`M-x diff` compares `HEAD` with all tracked and untracked working-tree changes. `M-x diff-current-commit` compares `HEAD^` with `HEAD`, including every changed file. `M-x diff-branch` finds the remote default branch, falls back to `main`, `master`, or `dev`, computes the fork point or merge base, and compares it with the complete working tree. Each command opens one continuous left/right Ediff review with file headers, wrapped source, Black & Pink hunk colors, `n`/`p` navigation, and review-buffer cleanup on quit.

Local workspaces use normal filesystem paths. Native Windows workspaces use bounded SSHFS mounts and OpenSSH-launched PowerShell rather than TRAMP against a `cmd.exe` login. The workspace creator also supports WSL distributions through TRAMP `/wsl:` paths and `wsl.exe` terminals. AIPanel discovers and launches the selected agent in the attached entry's local, WSL, POSIX SSH, or Windows SSH environment and gives it the entry directory as its working directory. Its file context uses the path relative to that directory plus the current line and column. Context waits for a stable agent screen, and a newer request cancels an older pending send. Platform-specific behavior remains isolated in the Emacs platform and Windows adapters.

The former `nvim`, `tmux`, `ghostty`, `zed`, `lazygit`, and `hunk` packages are preserved under `dotfiles/old/` and are not stowed by any profile. Arch WSL retains only the unconfigured Neovim binary as its shell editor.

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

- Yazi
- Lazygit
- Neovim on Arch WSL only, without managed dotfiles
- Emacs on CachyOS only

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

### Docker

The CachyOS profile installs the open source Docker Engine and its command-line tools from the official Arch repositories. The `docker` package provides the engine, daemon, containerd, and runc; `docker-buildx` provides BuildKit builds; and `docker-compose` provides Compose. Docker Desktop is not installed.

The module enables and starts `docker.service`. It deliberately does not add the current user to the `docker` group because membership grants root-equivalent access to the host. Before installing anything, it checks both the current process groups and the account's configured supplementary groups and refuses to continue if either contains `docker`. Docker therefore remains usable through `sudo` unless the user chooses a separate, explicit access policy.

This module runs only for CachyOS. Arch WSL and Ubuntu Server do not install a Docker daemon.

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

CachyOS uses the complete shared Linux profile after the graphical operating-system installer finishes. The profile installs the CachyOS Kernel Manager and the stable `linux-cachyos` kernel, removes the Cachy Update notifier, CachyOS Hello, the CachyOS default Zsh configuration, Vim, Konsole, and Alacritty, installs development packages, native Wayland graphical Emacs, SSHFS, Axidev OSK, Kanata, its independent KDE tray, Handy offline dictation, ydotool desktop automation, and the KDE Plasma desktop configuration, configures the OpenSSH service, sets Zsh as the default shell, deploys shared dotfiles, configures agent tools, and offers GitHub and Tailscale authentication.

The installer does not change sudoers, locale, drivers, or power settings. It selects the stable CachyOS kernel package through the Arch package repository; it does not install an LTS kernel. It installs OpenSSH, generates missing host keys, writes the shared listener policy, and enables the system service. Tailscale installs its system service separately.

### Emacs Application Lifecycle

The CachyOS profile does not install or configure a separate terminal emulator. It removes the CachyOS Fish stack and legacy terminal packages, then uses Ghostel inside normal-process graphical Emacs for workbench terminals. The same Emacs configuration supports native Windows through Ghostel's prebuilt Windows module and ConPTY backend. CachyOS installs the native Wayland Emacs package and launches it through the distribution desktop entry. Closing the last Emacs frame therefore stops the complete workbench process.

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

The profile installs Iosevka Nerd Font and applies the Black & Pink color scheme through Breeze. Its Plasma theme supplies transparent backgrounds for the clock island, dock, popup panels and temporary volume/brightness indicators. Application windows keep the normal color scheme, while text and icons remain opaque.

The shared frosted-glass material uses an index of refraction of 1.50, roughness of 0.50, simulated thickness of 60 and an inward bevel width of up to 48 pixels, limited by the surface size. Soft black shadows sit four pixels below the surfaces. The island masks its shadow out of the transparent interior. The dock stays translucent rather than switching to adaptive opacity.

The KDE Plasma module builds and installs `myconfig-kde-glass` from the pinned upstream source and the patches in `linux/assets/kde-glass`. The package declares its build and runtime dependencies. The material settings are stored in `dotfiles/kde-plasma/.local/share/myconfig/kde-plasma/glass.conf`. The standard CachyOS installer includes this setup without additional manual installation steps.

The module reuses the installed package when its version and recorded KWin version match, and rebuilds it otherwise. Each build has a distinct effect filename and resource namespace because KWin can retain unloaded libraries. Old blur providers are unloaded before the replacement loads. If Glass cannot initialize, the module attempts to restore KDE's standard blur and reports the failure. A user service repeats the version check at every Plasma login and runs the dedicated Glass repair script after a KWin upgrade, so the full profile does not need to run again. Running Plasma is reloaded without restarting KWin or the desktop session; offline installation takes effect at the next login.

An existing rEFInd installation receives the Black & Pink theme originally used by the removed Fedora profile. Its full-screen banner is pitch black with a four-pixel pink bar along the bottom, and its default selections use translucent pink outlines. The theme keeps rEFInd's built-in OS and utility icons, hides labels, hints, arrows, and device badges, and deliberately leaves `showtools` to the stock configuration to prevent duplicate firmware, reboot, and shutdown buttons. The installer generates the three theme images from the historical script, enables mouse support, keeps a one-time `refind.conf.pre-blacknpink` backup, and owns two include lines in a marked configuration block so reruns preserve unrelated boot entries and settings.

The profile installs a libinput Lua plugin at `/etc/libinput/plugins/90-myconfig-pointer-sensitivity.lua`. KWin loads it at the next login and multiplies relative X/Y pointer motion by 4. It applies to mice, trackballs, and other relative pointers, while absolute touchpad and touchscreen events are unchanged. KDE's pointer and touchpad acceleration remain at their maximum flat-profile setting.

At each KDE Plasma login, a desktop script reconciles two module-owned panels on every connected display. The top panel fits `myconfig.island`, a clock capsule with 26-pixel Iosevka text and 24 logical pixels of padding on either side. Clicking the clock opens five pages: hardware and settings, calendar, notifications, applications, and session and power. Native Plasma widgets supply CPU, memory and network measurements, the calendar, notifications and tray controls. Tray filtering keeps application icons separate from Plasma settings and excludes notifications from the settings tray. No Quickshell process or configuration is required.

The clock keeps its position during the opening and closing animation, while the capsule stretches and compresses around it. A pink marker on the right slides and deforms as the page changes. The marker buttons provide direct navigation and show their labels on hover. Wheel events over the header and unused space switch pages, while widgets retain their own wheel handling. Settings popups open to the left and close when the user clicks elsewhere in the island or changes pages. Session and power buttons show both icons and text, use KDE's native actions and leave unsupported actions visible but disabled.

The centered bottom dock keeps its Application Dashboard, Overview control and Icons-only Task Manager. It starts with no pinned applications, supports the normal **Pin to Task Manager** action, and shows windows from its own display across all virtual desktops. The separate legacy Session and Power widgets remain installed for saved panels that still reference them.

Both panels use automatic hiding. MyConfig Plasma Panels, a KWin script, gives the top and bottom edges a 24-logical-pixel inward activation zone across each display. A matching panel appears as an overlay and returns to native automatic hiding 400 milliseconds after the pointer leaves both the panel and its activation zone; an open panel popup postpones hiding. The script temporarily places truly fullscreen windows in KWin's below layer, allowing panels, picture-in-picture windows, and other desktop interactions to remain available, then restores each window's prior stacking state when it leaves fullscreen. The top panel is 68 logical pixels high and fits the clock capsule. The floating dock is 47 logical pixels high, uses KDE Plasma's 8-pixel floating margin, and follows the width of its launcher and visible tasks.

The first layout application backs up `~/.config/plasma-org.kde.plasma.desktop-appletsrc`, builds its replacement panels, and removes the initial panels only after replacement construction succeeds. Later logins configure panel geometry, create missing managed panels, and replace panels from an older layout version while carrying manual task-manager pins into replacement docks. An existing top panel with the old clock and tray controls receives the island in place. Once the island is present, reconciliation keeps that instance and its child widget settings rather than recreating them. Dock controls are rebuilt with their manual task-manager pins retained. MyConfig Plasma Panels also starts the reconciler immediately when KWin reports a display change, with one delayed retry for Plasma's output update. Unrelated panels and managed panels for temporarily disconnected displays remain intact. If KDE Plasma is not running during installation, the profile installs the configuration and defers panel creation until the next KDE Plasma login.

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

The Arch WSL setup installs packages through `pacman` and `paru`, including `base-devel`, `rustup`, `openssh`, `zsh`, `rsync`, `stow`, `wsl2-ssh-agent`, `ripgrep`, `go`, `yazi-git`, `ffmpeg`, `7zip`, `jq`, `poppler`, `fd`, `fzf`, `bat`, `zoxide`, `resvg`, `imagemagick`, `eza`, `llvm`, `bun`, `python`, `fastfetch`, `lazygit`, `jdk-openjdk`, `maven`, `make`, `cmake`, `btop`, `tokei`, `hunk-bin`, `neovim`, `nodejs`, `npm`, `node-gyp`, `opencode`, and `github-cli`. Neovim remains the unconfigured shell editor; the profile does not stow the archived Neovim, tmux, Lazygit, or Hunk packages.

The shared profile installs the latest Playwright MCP package through Bun, then downloads only its matching Chromium Headless Shell. The `ai` Stow package owns the shared agent instructions, skills, Claude configuration, and OpenCode configuration. OpenCode launches Playwright through `{env:HOME}`, applies the Black & Pink theme, binds half-page message scrolling to `Ctrl+U` and `Ctrl+D`, and receives its single generated `AGENTS.md` link during agent configuration. Machine-specific MCP servers can live in untracked `~/.config/opencode/config.json`, which OpenCode merges with the tracked runtime file. Existing conflicting configs are backed up but not migrated automatically. The installer validates the merged runtime configuration after Stow. Invalid configuration, browser failures, and SSH service failures stop the profile. Missing GitHub or Tailscale authentication offers an interactive login, or prints the deferred command without failing.

The shared `update()` function updates global Bun packages, then updates Chromium Headless Shell when the Playwright command exists in Bun's global package workspace. Browser update failures produce a warning and do not stop later updates. The same browser step runs after Homebrew updates on macOS.

The generated `~/environment.md` inventories the installed shell, development, media, interoperability, and agent capabilities. The shared `AGENTS.md` points models to this file when they need to inspect available tools.

### Synced Dotfiles

Arch WSL syncs and stows these shared dotfile packages from the Windows-accessible repo path:

- `zsh`
- `yazi`
- `ai`

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
│   ├── emacs/
│   ├── hermes/
│   ├── hyfetch/
│   ├── kanata/
│   ├── kanata-kde/
│   ├── old/
│   │   ├── ghostty/
│   │   ├── hunk/
│   │   ├── lazygit/
│   │   ├── nvim/
│   │   ├── tmux/
│   │   └── zed/
│   ├── qbt-search/
│   ├── wallpaper/
│   ├── yazi/
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
| `ai` | Shared agent instructions, skills, Claude configuration, and OpenCode runtime | `~/.agents/`, `~/.claude/`, and `$XDG_CONFIG_HOME/opencode/` |
| `emacs` | Normal-process graphical workbench | `$XDG_CONFIG_HOME/emacs/` |
| `hermes` | Hermes config | `$XDG_CONFIG_HOME/hermes/` |
| `hyfetch` | Hyfetch config | `$XDG_CONFIG_HOME/hyfetch.json` |
| `kanata` | Portable key mappings and user service | `$XDG_CONFIG_HOME/kanata/` and user systemd units |
| `kanata-kde` | Independent KDE tray for Kanata layer selection | `~/.local/bin/` and user systemd units |
| `qbt-search` | qBittorrent search plugins | Application-specific search plugin directory |
| `wallpaper` | Wallpaper assets | Wallpaper directory |
| `yazi` | Yazi config and flavor | `$XDG_CONFIG_HOME/yazi/` |
| `zsh` | `.zshrc`, Oh My Zsh theme, and custom plugin | Home directory and Oh My Zsh custom paths |

`dotfiles/old/` preserves the retired Ghostty, Hunk, Lazygit, Neovim, tmux, and Zed configurations. No installer profile stows packages from that directory.

The `ai` package ships `settings.json` without a `hooks` key on purpose. Windows Workstation copies only `CLAUDE.md` and `skills/` from this package.

### Installation Model

| Target              | Dotfile Strategy                                                                    |
| ------------------- | ----------------------------------------------------------------------------------- |
| CachyOS             | Back up `~/dotfiles`, copy eleven packages, back up conflicts, then `stow --restow` |
| Ubuntu Server       | Back up `~/dotfiles`, copy Zsh, back up conflicts, then `stow --restow`             |
| Windows Workstation | Direct copy of Windows configs                                                      |
| Arch WSL            | Back up `~/dotfiles`, copy four packages, back up conflicts, then `stow --restow`   |
