# Configuration Specifications

> A modular configuration bank for building reproducible development environments across platforms.

This document defines all components of the development environment, organized by category.
Platform-specific implementations are documented in their respective sections.

---

## Table of Contents

- [Shell](#shell)
- [Terminal Emulator](#terminal-emulator)
- [CLI Tools](#cli-tools)
- [File Manager](#file-manager)
- [Editors](#editors)
- [Desktop Environment](#desktop-environment)
- [Development Languages & Runtimes](#development-languages--runtimes)
- [Shared Non-Windows Package Baseline](#shared-non-windows-package-baseline)
- [Platform-Specific: macOS](#platform-specific-macos)
- [Platform-Specific: Ubuntu Server](#platform-specific-ubuntu-server)
- [Platform-Specific: Windows](#platform-specific-windows)

---

## Shell

### Zsh

The primary shell is **Zsh** with **Oh My Zsh** framework.

| Component | Value |
|-----------|-------|
| Shell | `zsh` |
| Framework | Oh My Zsh |
| Theme | `blacknpink` |
| Config location | `~/.zshrc` |

#### Plugins

| Plugin | Source | Description |
|--------|--------|-------------|
| `git` | Built-in | Git aliases and completions |
| `vi-mode` | Built-in | Vi keybindings in shell |
| `zsh-autosuggestions` | [zsh-users/zsh-autosuggestions](https://github.com/zsh-users/zsh-autosuggestions) | Fish-like autosuggestions |
| `zsh-syntax-highlighting` | [zsh-users/zsh-syntax-highlighting](https://github.com/zsh-users/zsh-syntax-highlighting) | Syntax highlighting |
| `inaya` | Custom (stowed) | Personal aliases, functions, and environment |

#### Custom Theme: `blacknpink`

Location: `~/.oh-my-zsh/custom/themes/blacknpink.zsh-theme`

The theme is based on Oh My Zsh's bundled `refined` prompt and maps prompt colors to the shared Black & Pink terminal palette.

#### Custom Plugin: `inaya`

Location: `~/.oh-my-zsh/custom/plugins/inaya/inaya.plugin.zsh`

**Environment Variables:**

```bash
XDG_CONFIG_HOME="$HOME/.config"
XDG_CACHE_HOME="$HOME/.cache"
EDITOR="nvim"
VISUAL="nvim"
TERM="xterm-256color"
VI_MODE_SET_CURSOR=true
LANG="en_US.UTF-8"
LC_ALL="en_US.UTF-8"
```

**Key Aliases:**

| Alias | Command | Description |
|-------|---------|-------------|
| `vim`, `vi`, `v` | `nvim` | Neovim as default editor |
| `ls` | `eza --icons --group-directories-first --git` | Modern ls replacement |
| `find` | `fd` | Fast find alternative |
| `grep` | `rg` | Ripgrep with smart defaults |
| `lg` | `lazygit` | Terminal UI for git |
| `ff` | `fastfetch` | System info display |

**Key Functions:**

| Function | Description |
|----------|-------------|
| `y` | Yazi file manager wrapper (changes directory on exit) |
| `pf` | Fuzzy file picker with preview, opens in nvim |
| `mkd` | Create directory and cd into it |
| `use-tmux` | Attach or create tmux session |
| `reload-zsh` | Reload zsh configuration |
| `stowgo` | Create and stow a new dotfiles package |
| `update` | Update system packages |
| `cleanup` | Interactive cleanup utility |

#### Dotfiles Structure

```
zsh/
└── .oh-my-zsh/
    └── custom/
        ├── themes/
        │   └── blacknpink.zsh-theme
        └── plugins/
            └── inaya/
                └── inaya.plugin.zsh
```

---

## Terminal Emulator

### WezTerm

Cross-platform, GPU-accelerated terminal emulator.

| Setting | Value |
|---------|-------|
| Background | `#000000` |
| Color scheme | `BlackPink` |
| Font | `Iosevka NFM` |
| Font size | `16` |
| Default command | `pwsh.exe` on Windows |

#### Dotfiles Structure

```
wezterm/
└── .wezterm.lua
```

---

## CLI Tools

### Core Utilities

| Tool | Purpose | Replaces |
|------|---------|----------|
| `eza` | Modern ls with icons and git integration | `ls` |
| `fd` | Fast, user-friendly find | `find` |
| `ripgrep` (rg) | Fast recursive grep | `grep` |
| `bat` | Cat with syntax highlighting | `cat` |
| `fzf` | Fuzzy finder | - |
| `zoxide` | Smart cd with frecency | `cd` |
| `btop` | Resource monitor | `top`, `htop` |
| `fastfetch` | System information display | `neofetch` |
| `1password-cli` | 1Password command-line interface | - |
| `jq` | JSON processor | - |

### Git Tools

| Tool | Purpose |
|------|---------|
| `git` | Version control |
| `lazygit` | Terminal UI for git |

### Lazygit

Terminal UI for git with a shared Black & Pink theme.

| Setting | Value |
|---------|-------|
| Theme | Black & Pink |
| Config location | `$XDG_CONFIG_HOME/lazygit/config.yml` |
| Windows config location | `%LOCALAPPDATA%\lazygit\config.yml` |

#### Dotfiles Structure

```
lazygit/
└── .config/
    └── lazygit/
        └── config.yml
```

### Tmux

Terminal multiplexer with **Oh My Tmux** configuration.

| Component | Value |
|-----------|-------|
| Framework | [Oh My Tmux](https://github.com/gpakosz/.tmux) |
| Theme | Monokai (custom) |
| Default shell | `/bin/zsh` |
| Config location | `$XDG_CONFIG_HOME/tmux/` |

**Theme Colors (Monokai):**

| Element | Color |
|---------|-------|
| Background | `#272822` |
| Foreground | `#FFFFFF` |
| Accent (Pink) | `#F92672` |
| Green | `#A6E22E` |
| Orange | `#FD971F` |

#### Dotfiles Structure

```
tmux/
└── .config/
    └── tmux/
        └── tmux.conf.local
```

---

## File Manager

### Yazi

Terminal file manager with image preview support.

| Setting | Value |
|---------|-------|
| Theme | Monokai (via `ya pkg`) |
| Flavor source | `malick-tammal/monokai` |

**Dependencies:**

- `ffmpeg` - Video thumbnails
- `sevenzip` - Archive preview
- `poppler` - PDF preview
- `resvg` - SVG rendering
- `imagemagick` - Image processing
- `font-symbols-only-nerd-font` - Icons

#### Dotfiles Structure

```
yazi/
└── .config/
    └── yazi/
        ├── theme.toml
        └── flavors/
            └── .gitkeep
```

---

## Editors

### Neovim

Primary terminal-based editor.

| Component | Value |
|-----------|-------|
| Distribution | [LazyVim](https://www.lazyvim.org/) |
| Theme | Monokai (`tanvirtin/monokai.nvim`) |
| Config location | `$XDG_CONFIG_HOME/nvim/` |

**Custom Plugins:**

| Plugin | Purpose |
|--------|---------|
| `auto-save.lua` | Automatic file saving |
| `colorscheme.lua` | Monokai theme configuration |

#### Dotfiles Structure

```
nvim/
└── .config/
    └── nvim/
        └── lua/
            └── plugins/
                ├── auto-save.lua
                └── colorscheme.lua
```

## Desktop Environment

Components that form the visual desktop experience.

### Window Management

Tiling window manager for efficient workspace organization.

**Expected Features:**

- Binary space partitioning (BSP) layout
- Window gaps and padding
- Keyboard-driven window control
- Mouse modifier support
- Window opacity (focused vs unfocused)
- Per-application rules

### Status Bar

System status bar displaying:

- Workspace/space indicators
- Active application
- System stats (CPU, RAM, Battery)
- Date and time
- Volume control
- Quick actions (restart WM, mission control/spaces overview)

**Theme:** Monokai

| Element | Color (Hex) |
|---------|-------------|
| Background | `#272822` |
| Pink (accent) | `#F92672` |
| Orange | `#FD971F` |
| Green | `#A6E22E` |
| Cyan | `#66D9EF` |

---

## Development Languages & Runtimes

| Language/Runtime | Tool | Purpose |
|------------------|------|---------|
| Python | `python` | Python development |
| Go | `go` | Go development |
| Rust | `rustup-init` | Rust toolchain |
| JavaScript/TypeScript | `nvm`, `node` | Node.js runtime management |
| Java | `openjdk`, `maven` | Java development |
| C/C++ | `gcc`, `llvm`, `cmake`, `make` | Compiler and build toolchain |
| Build tools | `meson`, `conan`, `zig` | Build systems and toolchain helpers |

---

## Shared Non-Windows Package Baseline

These are the packages and tools intended for every Unix-like platform in this
repo: Fedora, Ubuntu, and macOS. Windows keeps its own native package list.

### System And Desktop

- DNF/Homebrew package manager support where applicable
- GRUB EFI tools
- Shim
- EFI boot manager
- rEFInd
- NetworkManager
- greetd
- gtkgreet
- Niri
- Waybar
- Foot
- Fuzzel
- Mako
- Layer Shell Qt
- PipeWire
- WirePlumber
- desktop portals
- brightness control
- media player controls
- screenshot tools
- Wayland clipboard tools
- Axidev OSK

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
- custom zsh plugin
- tmux
- Oh My Tmux
- tar
- unzip
- xz
- file
- fontconfig
- Iosevka Nerd Font

### Terminal And Editors

- Neovim
- WezTerm

### CLI Tools

- ripgrep
- fd
- fzf
- zoxide
- eza
- bat
- jq
- less
- lazygit
- yazi
- fastfetch
- btop
- tokei
- tree-sitter CLI

### Programming Languages And Build Tools

- Python
- Go
- Rustup
- NVM
- OpenJDK
- Maven
- GCC
- LLVM
- CMake
- Make
- Meson
- Conan
- Zig

### Media And File Tooling

- FFmpeg
- 7-Zip
- Poppler
- resvg
- ImageMagick

### Developer Apps And Services

- 1Password
- 1Password CLI
- Google Chrome
<!--- Docker-->
- Ollama
- OpenAI Codex
- T3 Code

### Creative Apps

- Blender
- Krita
- Kdenlive

---

## Platform-Specific: macOS

This section documents components specific to macOS.

### Window Management: Yabai

[Yabai](https://github.com/koekeishiya/yabai) - Tiling window manager for macOS.

| Setting | Value |
|---------|-------|
| Layout | BSP (Binary Space Partitioning) |
| Window gap | 20px |
| Padding (all sides) | 20px |
| External bar | Bottom, 30px height |
| Window placement | Second child |
| Mouse modifier | `alt` |
| Window shadows | Float only |
| Active window opacity | 1.0 |
| Inactive window opacity | 0.5 |

**Installation:** `brew install asmvik/formulae/yabai`

**Requirements:**

- Accessibility permissions
- SIP configuration for full functionality

#### Dotfiles Structure

```
yabai/
└── .config/
    └── yabai/
        └── yabairc
```

### Status Bar: Sketchybar

[Sketchybar](https://github.com/FelixKratz/SketchyBar) - Highly customizable macOS status bar.

| Setting | Value |
|---------|-------|
| Position | Bottom |
| Height | 34px |
| Blur radius | 30 |
| Theme | Monokai |
| Font | Hack Nerd Font |

**Components:**

| Item | Position | Description |
|------|----------|-------------|
| Restart WM | Left | Restart Yabai/Sketchybar |
| Mission Control | Left | Open Mission Control |
| Space indicators | Left | Workspace switcher (1-10) |
| Front app | Center | Currently focused application |
| CPU/RAM | Right | System statistics |
| Volume | Right | Audio control |
| Battery | Right | Battery status |
| Clock | Right | Date and time |

**Dependencies:**

- `sketchybar-system-stats` - CPU/RAM stats provider

**Plugin Scripts:**

- `battery.sh` - Battery status
- `clock.sh` - Time display
- `cpu.sh` - CPU usage
- `ram.sh` - Memory usage
- `front_app.sh` - Active application
- `volume.sh` - Audio control
- `space.sh` - Workspace indicator
- `switch_space.sh` - Workspace switching
- `hover.sh` - Hover effects
- `mission_control.sh` - Mission Control trigger
- `restart_window_management.sh` - Restart services

#### Dotfiles Structure

```
sketchybar/
└── .config/
    └── sketchybar/
        ├── sketchybarrc
        └── plugins/
            ├── battery.sh
            ├── clock.sh
            ├── cpu.sh
            ├── front_app.sh
            ├── hover.sh
            ├── mission_control.sh
            ├── ram.sh
            ├── restart_window_management.sh
            ├── space.sh
            ├── switch_space.sh
            └── volume.sh
```

### macOS-Specific Shell Functions

Located in `inaya.plugin.zsh`:

| Function | Description |
|----------|-------------|
| `update` | Runs `brew update && brew upgrade && brew cleanup` |
| `bootout-gui` | Bootout current GUI session via launchctl |

### macOS-Specific Environment Variables

```bash
JAVA_HOME="/opt/homebrew/opt/openjdk"
PATH="$HOME/.local/bin:$PATH:$(go env GOPATH)/bin:$JAVA_HOME/bin"
VCPKG_ROOT="$HOME/vcpkg"
```

### Package Manager: Homebrew

All packages installed via Homebrew. See [macos/install.sh](macos/install.sh) for the complete installation script.

### Dotfiles Management: GNU Stow

Dotfiles are managed using GNU Stow:

- Source: `~/dotfiles/` (copied from repository)
- Target: `$HOME` (symlinked via stow)
- Mode: `--restow --no-folding`

---

## Platform-Specific: Ubuntu Server

This section documents components specific to Ubuntu Server (headless, no desktop environment).

### Package Manager: apt

Ubuntu Server uses apt only. Homebrew/Linuxbrew is intentionally not installed.

See [ubuntu-server/install.sh](ubuntu-server/install.sh) for the complete installation script.

### Ubuntu-Specific Shell Functions

Located in `inaya.plugin.zsh` (platform detection is automatic):

| Function | Description |
|----------|-------------|
| `update` | Updates system packages with apt |
| `use-tmux` | Attach or create tmux session |

### Ubuntu-Specific Environment Variables

```bash
PATH="$HOME/.local/bin:$PATH"
```

### Installed Packages

Only the minimal zsh/Oh My Zsh prerequisites are installed:

| Category | Packages |
|----------|----------|
| Core | `ca-certificates`, `curl`, `git`, `zsh` |

### What's NOT Included (Server Edition)

The Ubuntu Server installation does **not** include:

- Desktop environment components
- Homebrew/Linuxbrew
- GNU Stow
- Development runtimes and SDKs such as Go, Rust, Node, Java, Maven, or LLVM
- Extra CLI tools such as tmux, Neovim, Yazi, Lazygit, fzf, ripgrep, or btop
- Yabai (macOS-only window manager)
- Sketchybar (macOS-only status bar)
- GUI applications

---

## Platform-Specific: Windows

This section documents components specific to Windows.

### Package Manager: Winget

All Windows native packages are installed via **Winget** (Windows Package Manager).
See [windows/install.ps1](windows/install.ps1) for the complete installation script.

### Shell & Terminal

| Component | Value |
|-----------|-------|
| Shell | PowerShell Core (`pwsh`) |
| Prompt | Oh My Posh (`pure` theme) |
| Terminal | WezTerm |
| Modules | `PSReadLine`, `Terminal-Icons` |

**PowerShell Profile Features:**

- Vi mode keybindings
- Prediction source (History)
- Cursor shape change (Beam for Insert, Block for Normal)
- Terminal icons for file listings

### Terminal Emulator: WezTerm

Cross-platform GPU-accelerated terminal used as the default Windows terminal.

| Setting | Value |
|---------|-------|
| Color Scheme | BlackPink |
| Font | Iosevka NFM |
| Background | `#000000` |
| Opacity | `1.0` |

### Window Management: Glaze WM

[Glaze WM](https://github.com/glazewm/glazewm) - A tiling window manager for Windows inspired by i3.

| Setting | Value |
|---------|-------|
| Layout | Tiling |
| Window Gap | 20px |
| Border Color | `#A6E22E` (Focused) |
| Shortcut | Alt-based (Alt+H/J/K/L for focus) |

#### Dotfiles Structure

```
windows/dotfiles/
└── glazewm/
    └── config.yaml
```

### Status Bar: Zebar

[Zebar](https://github.com/glazewm/zebar) - Customizable status bar for Windows.

| Setting | Value |
|---------|-------|
| Theme | Monokai |
| Widget | monokai-topbar |
| Pack | monokai-statusbar |

#### Dotfiles Structure

```
windows/dotfiles/
└── zebar/
    └── monokai-statusbar/
        ├── monokai-statusbar.css
        ├── monokai-statusbar.html
        └── zpack.json
```

### Windows Subsystem for Linux (WSL)

For all development tools, shell, and CLI utilities, Windows utilizes the **Ubuntu configuration** through WSL.

- **Distribution:** Ubuntu
- **Configuration:** Shared with [Ubuntu Server](#platform-specific-ubuntu-server)
- **Integration:** VS Code (native) connects to WSL for development.

---

## Dotfiles Summary

### Repository Structure

```
setup-config/
├── SPECS.md                    # This specification document
├── dotfiles/                   # Shared cross-platform dotfiles
│   ├── wezterm/
│   ├── nvim/
│   ├── tmux/
│   ├── yazi/
│   └── zsh/
├── macos/                      # macOS-specific
│   ├── install.sh
│   ├── uninstall.sh
│   └── dotfiles/               # macOS-only dotfiles
│       ├── sketchybar/
│       └── yabai/
├── ubuntu-server/              # Ubuntu Server & Windows (WSL) specific
│   ├── install.sh
│   └── uninstall.sh
└── windows/                    # Windows-specific
    ├── install.ps1
    ├── uninstall.ps1
    └── dotfiles/               # Windows-only dotfiles
        ├── glazewm/
        └── zebar/
```

### Cross-Platform (Shareable)

These dotfiles are located in `/dotfiles/` at the project root and can be used across different Unix-like systems:

| Package | Description | Target |
|---------|-------------|--------|
| `wezterm` | Terminal emulator config | `~/.wezterm.lua` |
| `lazygit` | Lazygit theme/config | `~/.config/lazygit/` |
| `nvim` | Neovim/LazyVim plugins | `~/.config/nvim/` |
| `tmux` | Tmux customization | `~/.config/tmux/` |
| `yazi` | File manager theme | `~/.config/yazi/` |
| `zsh` | Custom Oh My Zsh theme and plugin | `~/.oh-my-zsh/custom/` |

### macOS-Specific

These dotfiles are located in `/macos/dotfiles/`:

| Package | Description | Target |
|---------|-------------|--------|
| `yabai` | Window manager config | `~/.config/yabai/` |
| `sketchybar` | Status bar config | `~/.config/sketchybar/` |

### Windows-Specific

These dotfiles are located in `/windows/dotfiles/`:

| Package | Description | Target |
|---------|-------------|--------|
| `glazewm` | Window manager config | `~/.glzr/glazewm/` |
| `zebar` | Status bar widgets | `~/.glzr/zebar/` |

---

## Theme: Monokai

A consistent Monokai theme is applied across all components:

| Color | Hex | Usage |
|-------|-----|-------|
| Background | `#272822` | Dark backgrounds |
| Foreground | `#FFFFFF` | Primary text |
| Pink | `#F92672` | Accents, highlights |
| Orange | `#FD971F` | Warnings, secondary accents |
| Green | `#A6E22E` | Success, active states |
| Cyan | `#66D9EF` | Links, info |
| Yellow | `#E6DB74` | Strings, emphasis |

**Applied to:**

- Neovim (colorscheme)
- Tmux (status bar)
- Sketchybar (UI elements)
- Yazi (file manager flavor)
- Zebar (status bar)
- Glaze WM
