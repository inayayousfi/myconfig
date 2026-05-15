#!/bin/bash

# macOS Development Environment Setup Script
# This script is idempotent - running it multiple times is safe
# Uses GNU Stow for dotfiles management
# Dotfiles are copied to ~/dotfiles and stowed from there

# set -e (Disabled to ensure script continues even if some packages fail)

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(dirname "$SCRIPT_DIR")"
SHARED_DOTFILES_DIR="$REPO_ROOT/dotfiles"
MACOS_DOTFILES_DIR="$SCRIPT_DIR/dotfiles"
USER_DOTFILES_DIR="$HOME/dotfiles"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

log_info() {
    echo -e "${BLUE}[INFO]${NC} $1"
}

log_success() {
    echo -e "${GREEN}[SUCCESS]${NC} $1"
}

log_warning() {
    echo -e "${YELLOW}[WARNING]${NC} $1"
}

log_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

# Ensure sudo access and keep it alive
check_sudo() {
    log_info "This script requires sudo privileges for some operations."
    log_info "You may be prompted for your password."

    # Ask for sudo upfront
    if sudo -v; then
        # Keep-alive: update existing sudo time stamp until the script has finished
        while true; do sudo -n true; sleep 60; kill -0 "$$" || exit; done 2>/dev/null &
        log_success "Sudo access granted"
    else
        log_error "Sudo access is required for this script to run correctly."
        exit 1
    fi
}

# Ensure XDG directories exist and have correct ownership
ensure_dir_owned() {
    local dir="$1"
    if [ -d "$dir" ]; then
        if [ ! -w "$dir" ]; then
            log_warning "Directory $dir is not writable. Attempting to fix with sudo..."
            sudo chown -R "$(id -u):$(id -g)" "$dir"
        fi
    else
        mkdir -p "$dir" || {
            log_warning "Failed to create $dir. Trying with sudo..."
            sudo mkdir -p "$dir"
            sudo chown -R "$(id -u):$(id -g)" "$dir"
        }
    fi
}

export XDG_CONFIG_HOME="${XDG_CONFIG_HOME:-$HOME/.config}"
export XDG_CACHE_HOME="${XDG_CACHE_HOME:-$HOME/.cache}"

ensure_dir_owned "$XDG_CONFIG_HOME"
ensure_dir_owned "$XDG_CACHE_HOME"

# ============================================================================
# Homebrew Installation
# ============================================================================

install_homebrew() {
    if command -v brew &>/dev/null; then
        log_success "Homebrew is already installed"
    else
        log_info "Installing Homebrew..."
        /bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)"

        # Add Homebrew to PATH for this session
        eval "$(/opt/homebrew/bin/brew shellenv)"
        log_success "Homebrew installed"
    fi
}

# ============================================================================
# Brew Packages Installation
# ============================================================================

install_brew_package() {
    local package="$1"
    local cask="${2:-false}"

    if [ "$cask" = "true" ]; then
        if brew list --cask "$package" &>/dev/null; then
            log_success "$package (cask) is already installed"
        else
            log_info "Installing $package (cask)..."
            if brew install --cask "$package"; then
                log_success "$package (cask) installed"
            else
                log_error "Failed to install $package (cask)"
                return 1
            fi
        fi
    else
        if brew list "$package" &>/dev/null; then
            log_success "$package is already installed"
        else
            log_info "Installing $package..."
            if brew install "$package"; then
                log_success "$package installed"
            else
                log_error "Failed to install $package"
                return 1
            fi
        fi
    fi
}

install_uv_package() {
    local package="$1"

    if uv tool list | grep -q "^$package "; then
        log_success "$package (uv) is already installed"
    else
        log_info "Installing $package (uv)..."
        if uv tool install "$package"; then
            log_success "$package (uv) installed"
        else
            log_error "Failed to install $package (uv)"
            return 1
        fi
    fi
}

# ============================================================================
# Individual Package Installation Functions
# ============================================================================

# Core tools
install_git() { install_brew_package "git"; }
install_stow() { install_brew_package "stow"; }
install_zsh() { install_brew_package "zsh"; }
install_tmux() { install_brew_package "tmux"; }
install_neovim() { install_brew_package "neovim"; }
install_python() { install_brew_package "python"; }

install_go() { install_brew_package "go"; }
install_llvm() { install_brew_package "llvm"; }
install_rustup() { install_brew_package "rustup-init"; }
install_bun() { install_brew_package "oven-sh/bun/bun"; }

# Java and build tools
install_openjdk() { install_brew_package "openjdk"; }
install_maven() { install_brew_package "maven"; }

# Modern CLI tools
install_zoxide() { install_brew_package "zoxide"; }
install_eza() { install_brew_package "eza"; }
install_fd() { install_brew_package "fd"; }
install_fzf() { install_brew_package "fzf"; }
install_ripgrep() { install_brew_package "ripgrep"; }
install_bat() { install_brew_package "bat"; }
install_lazygit() { install_brew_package "lazygit"; }
install_btop() { install_brew_package "btop"; }
install_fastfetch() { install_brew_package "fastfetch"; }
install_1password_cli() { install_brew_package "1password-cli"; }
install_uv() { install_brew_package "uv"; }
install_meson() { install_uv_package "meson"; }
install_conan() { install_uv_package "conan"; }

# Yazi file manager and dependencies
install_yazi() { install_brew_package "yazi"; }
install_ffmpeg() { install_brew_package "ffmpeg"; }
install_sevenzip() { install_brew_package "sevenzip"; }
install_jq() { install_brew_package "jq"; }
install_poppler() { install_brew_package "poppler"; }
install_resvg() { install_brew_package "resvg"; }
install_imagemagick() { install_brew_package "imagemagick"; }
install_nerd_font_symbols() { install_brew_package "font-symbols-only-nerd-font" "true"; }

# Cask applications
install_wezterm() { install_brew_package "wezterm" "true"; }
install_vscode() { install_brew_package "visual-studio-code" "true"; }

# Window management (macOS)
install_yabai() {
    if brew list "asmvik/formulae/yabai" &>/dev/null; then
        log_success "yabai is already installed"
    else
        log_info "Installing yabai..."
        if brew install asmvik/formulae/yabai; then
            log_success "yabai installed"
        else
            log_error "Failed to install yabai"
        fi
    fi
}

install_sketchybar() {
    if brew list "FelixKratz/formulae/sketchybar" &>/dev/null; then
        log_success "sketchybar is already installed"
    else
        log_info "Installing sketchybar..."
        brew tap FelixKratz/formulae
        if brew install sketchybar; then
            log_success "sketchybar installed"
        else
            log_error "Failed to install sketchybar"
        fi
    fi
}

install_sketchybar_system_stats() {
    if brew list "joncrangle/tap/sketchybar-system-stats" &>/dev/null; then
        log_success "sketchybar-system-stats is already installed"
    else
        log_info "Installing sketchybar-system-stats..."
        brew tap joncrangle/tap
        if brew install sketchybar-system-stats; then
            log_success "sketchybar-system-stats installed"
        else
            log_error "Failed to install sketchybar-system-stats"
        fi
    fi
}

install_opencode() {
    if brew list "sst/tap/opencode" &>/dev/null; then
        log_success "SST opencode is already installed"
    else
        log_info "Installing SST opencode..."
        if brew install sst/tap/opencode; then
            log_success "SST opencode installed"
        else
            log_error "Failed to install SST opencode"
        fi
    fi
}

install_borders() {
    if brew list "FelixKratz/formulae/borders" &>/dev/null; then
        log_success "borders is already installed"
    else
        log_info "Installing borders..."
        brew tap FelixKratz/formulae
        if brew install borders; then
            log_success "borders installed"
        else
            log_error "Failed to install borders"
        fi
    fi
}

# ============================================================================
# Install All Packages
# ============================================================================

install_packages() {
    log_info "Installing packages via Homebrew..."

    # Core tools (order matters - git first)
    install_git
    install_stow
    install_zsh
    install_tmux
    install_neovim
    install_python
    install_go
    install_llvm
    install_rustup
    install_bun

    # Java and build tools
    install_openjdk
    install_maven

    # Modern CLI tools
    install_zoxide
    install_eza
    install_fd
    install_fzf
    install_ripgrep
    install_bat
    install_lazygit
    install_btop
    install_fastfetch
    install_1password_cli
    install_uv
    install_meson
    install_conan

    # Yazi file manager and dependencies
    install_yazi
    install_ffmpeg
    install_sevenzip
    install_jq
    install_poppler
    install_resvg
    install_imagemagick
    install_nerd_font_symbols

    # Cask applications
    install_wezterm
    install_vscode

    # Window management (macOS)
    install_yabai
    install_sketchybar
    install_sketchybar_system_stats
    install_borders

    # Development tools
    install_opencode

    log_success "All packages installed"
}

# ============================================================================
# Dotfiles Setup - Copy to ~/.dotfiles
# ============================================================================

setup_user_dotfiles() {
    log_info "Setting up dotfiles in $USER_DOTFILES_DIR..."

    # Create the user dotfiles directory if it doesn't exist
    mkdir -p "$USER_DOTFILES_DIR"

    # Copy shared dotfiles (platform-independent) from repo root
    for package in lazygit nvim tmux wezterm yazi zsh; do
        if [ -d "$SHARED_DOTFILES_DIR/$package" ]; then
            log_info "Copying $package (shared) to $USER_DOTFILES_DIR..."
            rsync -a --update "$SHARED_DOTFILES_DIR/$package/" "$USER_DOTFILES_DIR/$package/"
        fi
    done

    # Copy macOS-specific dotfiles
    for package in sketchybar yabai; do
        if [ -d "$MACOS_DOTFILES_DIR/$package" ]; then
            log_info "Copying $package (macOS) to $USER_DOTFILES_DIR..."
            rsync -a --update "$MACOS_DOTFILES_DIR/$package/" "$USER_DOTFILES_DIR/$package/"
        fi
    done

    log_success "Dotfiles copied to $USER_DOTFILES_DIR"
}

# ============================================================================
# Stow Helper Functions
# ============================================================================

stow_package() {
    local package="$1"
    local target="${2:-$HOME}"

    log_info "Stowing $package to $target..."

    # Try to stow and capture any error output
    local stow_output
    if stow_output=$(stow --dir="$USER_DOTFILES_DIR" --target="$target" --restow --no-folding "$package" 2>&1); then
        log_success "$package stowed successfully"
        return 0
    fi

    # If we get here, there was a conflict
    log_warning "$package has conflicts with existing files:"
    echo ""

    # Extract and display conflicting files from stow output
    echo "$stow_output" | grep -E "(existing target|conflict)" | while read -r line; do
        echo -e "  ${YELLOW}→${NC} $line"
    done
    echo ""

    # Check if running in interactive mode
    if [ -t 0 ]; then
        # Interactive mode - ask user what to do
        echo -e "${BLUE}How would you like to resolve this conflict?${NC}"
        echo "  [a] Adopt   - Keep existing files and bring them into ~/dotfiles"
        echo "               (Use this if you've customized these configs)"
        echo "  [o] Override - Delete existing files and use ~/dotfiles versions"
        echo "               (Use this to get fresh configs from the repo)"
        echo "  [s] Skip    - Don't stow this package"
        echo ""

        while true; do
            read -r -p "Your choice [a/o/s]: " choice
            case "$choice" in
                [aA])
                    log_info "Adopting existing files for $package..."
                    stow --dir="$USER_DOTFILES_DIR" --target="$target" --adopt "$package"
                    stow --dir="$USER_DOTFILES_DIR" --target="$target" --restow --no-folding "$package"
                    log_success "$package adopted and restowed successfully"
                    return 0
                    ;;
                [oO])
                    log_info "Overriding existing files for $package..."
                    # Find and remove conflicting files
                    # Use stow --simulate to find what would be stowed, then remove existing files
                    local stow_sim_output
                    stow_sim_output=$(stow --dir="$USER_DOTFILES_DIR" --target="$target" --simulate --restow --no-folding "$package" 2>&1 || true)

                    # Extract conflicts from both error patterns:
                    # 1. "existing target is not owned by stow: <path>"
                    # 2. "over existing target <path> since"
                    local conflicts
                    conflicts=$(echo "$stow_sim_output" | grep -oE "existing target is not owned by stow: [^ ]+" | sed 's/existing target is not owned by stow: //')
                    conflicts="$conflicts $(echo "$stow_sim_output" | grep -oE "over existing target [^ ]+ since" | sed 's/over existing target //' | sed 's/ since//')"

                    for conflict_file in $conflicts; do
                        # Skip empty strings
                        [ -z "$conflict_file" ] && continue
                        local full_path="$target/$conflict_file"
                        if [ -e "$full_path" ] || [ -L "$full_path" ]; then
                            log_info "Removing $full_path..."
                            rm -rf "$full_path"
                        fi
                    done

                    # Now stow should work
                    stow --dir="$USER_DOTFILES_DIR" --target="$target" --restow --no-folding "$package"
                    log_success "$package stowed successfully (existing files overridden)"
                    return 0
                    ;;
                [sS])
                    log_warning "Skipping $package"
                    return 0
                    ;;
                *)
                    echo "Please enter 'a' for adopt, 'o' for override, or 's' for skip."
                    ;;
            esac
        done
    else
        # Non-interactive mode - use adopt strategy (safer default)
        log_info "Non-interactive mode: adopting existing files for $package..."
        stow --dir="$USER_DOTFILES_DIR" --target="$target" --adopt "$package" 2>&1 || {
            log_warning "Failed to adopt $package, trying to override..."
            # Find and remove conflicting files
            local stow_sim_output
            stow_sim_output=$(stow --dir="$USER_DOTFILES_DIR" --target="$target" --simulate --restow --no-folding "$package" 2>&1 || true)

            local conflicts
            conflicts=$(echo "$stow_sim_output" | grep -oE "existing target is not owned by stow: [^ ]+" | sed 's/existing target is not owned by stow: //')
            conflicts="$conflicts $(echo "$stow_sim_output" | grep -oE "over existing target [^ ]+ since" | sed 's/over existing target //' | sed 's/ since//')"

            for conflict_file in $conflicts; do
                [ -z "$conflict_file" ] && continue
                local full_path="$target/$conflict_file"
                if [ -e "$full_path" ] || [ -L "$full_path" ]; then
                    log_info "Removing $full_path..."
                    rm -rf "$full_path"
                fi
            done
        }
        stow --dir="$USER_DOTFILES_DIR" --target="$target" --restow --no-folding "$package" 2>&1 || {
            log_warning "Failed to stow $package, skipping..."
            return 0
        }
        log_success "$package stowed successfully"
        return 0
    fi
}

# ============================================================================
# Oh My Zsh Installation
# ============================================================================

install_oh_my_zsh() {
    if [ -d "$HOME/.oh-my-zsh" ]; then
        log_success "Oh My Zsh is already installed"
    else
        log_info "Installing Oh My Zsh..."
        RUNZSH=no KEEP_ZSHRC=yes sh -c "$(curl -fsSL https://raw.githubusercontent.com/ohmyzsh/ohmyzsh/master/tools/install.sh)"
        log_success "Oh My Zsh installed"
    fi
}

install_zsh_plugins() {
    local ZSH_CUSTOM="${ZSH_CUSTOM:-$HOME/.oh-my-zsh/custom}"

    # zsh-autosuggestions
    if [ -d "$ZSH_CUSTOM/plugins/zsh-autosuggestions" ]; then
        log_success "zsh-autosuggestions is already installed"
    else
        log_info "Installing zsh-autosuggestions..."
        git clone https://github.com/zsh-users/zsh-autosuggestions "$ZSH_CUSTOM/plugins/zsh-autosuggestions"
        log_success "zsh-autosuggestions installed"
    fi

    # zsh-syntax-highlighting
    if [ -d "$ZSH_CUSTOM/plugins/zsh-syntax-highlighting" ]; then
        log_success "zsh-syntax-highlighting is already installed"
    else
        log_info "Installing zsh-syntax-highlighting..."
        git clone https://github.com/zsh-users/zsh-syntax-highlighting "$ZSH_CUSTOM/plugins/zsh-syntax-highlighting"
        log_success "zsh-syntax-highlighting installed"
    fi

    # Stow custom inaya plugin
    stow_package "zsh"
    if [ -L "$HOME/.oh-my-zsh/custom/plugins/zieds/zieds.plugin.zsh" ]; then
        rm -f "$HOME/.oh-my-zsh/custom/plugins/zieds/zieds.plugin.zsh"
        rmdir "$HOME/.oh-my-zsh/custom/plugins/zieds" 2>/dev/null || true
    fi
}

configure_zshrc() {
    local ZSHRC="$HOME/.zshrc"

    log_info "Configuring .zshrc..."

    # Backup existing .zshrc if it exists and wasn't created by us
    if [ -f "$ZSHRC" ] && ! grep -q "# Managed by setup-config" "$ZSHRC"; then
        cp "$ZSHRC" "$ZSHRC.backup.$(date +%Y%m%d%H%M%S)"
        log_info "Backed up existing .zshrc"
    fi

    cat > "$ZSHRC" << 'EOF'
# Managed by setup-config
# Path to your Oh My Zsh installation.
export ZSH="$HOME/.oh-my-zsh"

# Theme
ZSH_THEME="blacknpink"

# Enable command auto-correction
ENABLE_CORRECTION="true"

# Plugins
plugins=(git zsh-autosuggestions zsh-syntax-highlighting vi-mode inaya)

source $ZSH/oh-my-zsh.sh
EOF

    log_success ".zshrc configured"
}

set_default_shell() {
    local zsh_path
    zsh_path=$(which zsh)

    if [ -z "$zsh_path" ]; then
        log_error "Zsh not found in PATH"
        return 1
    fi

    # Check if zsh is already the default shell
    if [[ "$SHELL" == *"/zsh" ]]; then
        log_success "Zsh is already the default shell"
        return 0
    fi

    log_info "Setting zsh as default shell ($zsh_path)..."

    # Ensure zsh is in /etc/shells
    if ! grep -Fxq "$zsh_path" /etc/shells; then
        log_info "Adding $zsh_path to /etc/shells..."
        echo "$zsh_path" | sudo tee -a /etc/shells > /dev/null
    fi

    # Try to use sudo chsh to avoid interaction if we have sudo access
    if sudo -n true 2>/dev/null; then
        if sudo chsh -s "$zsh_path" "$USER"; then
            log_success "Zsh set as default shell via sudo"
            export SHELL="$zsh_path"
            return 0
        fi
    fi

    # Fallback to interactive chsh if sudo failed or not available
    if [ -t 0 ]; then
        if chsh -s "$zsh_path"; then
            log_success "Zsh set as default shell"
            export SHELL="$zsh_path"
        else
            log_warning "Failed to set zsh as default shell"
        fi
    else
        log_warning "Cannot change default shell in non-interactive mode without sudo"
        log_info "Run 'sudo chsh -s $zsh_path $USER' manually to set zsh as your default shell"
    fi
}

# ============================================================================
# Oh My Tmux Installation
# ============================================================================

install_oh_my_tmux() {
    local OH_MY_TMUX_DIR="$HOME/.oh-my-tmux"
    local TMUX_CONFIG_DIR="$XDG_CONFIG_HOME/tmux"

    # Clone Oh My Tmux
    if [ -d "$OH_MY_TMUX_DIR" ]; then
        log_success "Oh My Tmux is already installed"
    else
        log_info "Installing Oh My Tmux..."
        git clone https://github.com/gpakosz/.tmux.git "$OH_MY_TMUX_DIR"
        log_success "Oh My Tmux installed"
    fi

    # Create tmux config directory
    ensure_dir_owned "$TMUX_CONFIG_DIR"

    # Symlink tmux.conf from Oh My Tmux
    if [ -L "$TMUX_CONFIG_DIR/tmux.conf" ]; then
        log_success "tmux.conf symlink already exists"
    else
        log_info "Creating tmux.conf symlink..."
        ln -sf "$OH_MY_TMUX_DIR/.tmux.conf" "$TMUX_CONFIG_DIR/tmux.conf"
        log_success "tmux.conf symlink created"
    fi

    # Stow custom tmux.conf.local (like we do for LazyVim plugins)
    log_info "Stowing custom tmux configuration..."
    stow_package "tmux"
}

# ============================================================================
# Neovim Configuration (LazyVim)
# ============================================================================

install_lazyvim() {
    local NVIM_CONFIG_DIR="$XDG_CONFIG_HOME/nvim"
    local LAZYVIM_SOURCE_DIR="$USER_DOTFILES_DIR/nvim/.config/nvim"

    if [ ! -f "$LAZYVIM_SOURCE_DIR/lua/config/lazy.lua" ]; then
        log_error "LazyVim source is missing in $LAZYVIM_SOURCE_DIR"
        return 1
    fi

    if [ -d "$NVIM_CONFIG_DIR" ] && [ -f "$NVIM_CONFIG_DIR/lua/config/lazy.lua" ]; then
        log_success "LazyVim is already installed"
    else
        log_info "Installing LazyVim from repo..."
    fi

    # Stow custom Neovim plugins
    log_info "Stowing custom Neovim plugins..."
    stow_package "nvim"
}

# ============================================================================
# WezTerm Configuration
# ============================================================================

configure_wezterm() {
    log_info "Configuring WezTerm via stow..."
    stow_package "wezterm"
    log_success "WezTerm configured"
}

# VS Code Configuration
# ============================================================================

install_vscode_extensions() {
    local extensions=("$@")
    if ! command -v code &>/dev/null; then
        log_warning "VS Code 'code' command not found, skipping extension installation"
        return
    fi

    for extension in "${extensions[@]}"; do
        if code --list-extensions | grep -qi "^$extension$"; then
            log_success "Extension $extension is already installed"
        else
            log_info "Installing VS Code extension: $extension..."
            code --install-extension "$extension" --force
            log_success "Extension $extension installed"
        fi
    done
}

configure_vscode() {
    log_info "Configuring VS Code extensions..."

    # Define your extensions here
        local extensions=(
            cheshirekow.cmake-format
            davidanson.vscode-markdownlint
            dbaeumer.vscode-eslint
            donjayamanne.githistory
            esbenp.prettier-vscode
            fill-labs.dependi
            geequlim.godot-tools
            github.copilot
            github.copilot-chat
            github.vscode-github-actions
            github.vscode-pull-request-github
            golang.go
            llvm-vs-code-extensions.vscode-clangd
            ms-azuretools.vscode-containers
            ms-azuretools.vscode-docker
            ms-dotnettools.csdevkit
            ms-dotnettools.csharp
            ms-dotnettools.vscode-dotnet-runtime
            ms-python.debugpy
            ms-python.python
            ms-python.vscode-pylance
            ms-python.vscode-python-envs
            ms-vscode.cmake-tools
            ms-vscode.makefile-tools
            ms-vscode.powershell
            oven.bun-vscode
            redhat.java
            redhat.vscode-xml
            rust-lang.rust-analyzer
            sst-dev.opencode
            sumneko.lua
            svelte.svelte-vscode
            upstash.context7-mcp
            usernamehw.errorlens
            vscjava.vscode-gradle
            vscjava.vscode-java-debug
            vscjava.vscode-java-dependency
            vscjava.vscode-java-pack
            vscjava.vscode-java-test
            vscjava.vscode-maven
        )

    install_vscode_extensions "${extensions[@]}"
    log_success "VS Code configuration complete"
}

# ============================================================================
# Yabai Configuration
# ============================================================================

configure_yabai() {
    log_info "Configuring Yabai via stow..."
    stow_package "yabai"

    # Make yabairc executable
    local YABAIRC="$XDG_CONFIG_HOME/yabai/yabairc"
    if [ -f "$YABAIRC" ]; then
        chmod +x "$YABAIRC"
    fi

    # Start yabai service if not running
    if ! pgrep -x "yabai" > /dev/null; then
        log_info "Starting yabai service..."
        yabai --start-service 2>/dev/null || true
    else
        log_info "Restarting yabai to apply configuration..."
        yabai --restart-service 2>/dev/null || true
    fi

    log_success "Yabai configured"
    log_warning "Note: Yabai requires accessibility permissions and SIP configuration for full functionality."
    log_warning "See: https://github.com/koekeishiya/yabai/wiki/Disabling-System-Integrity-Protection"
}

# ============================================================================
# Sketchybar Configuration
# ============================================================================

configure_sketchybar() {
    log_info "Configuring Sketchybar via stow..."
    stow_package "sketchybar"

    # Make sketchybarrc and plugins executable
    local SKETCHYBAR_CONFIG="$XDG_CONFIG_HOME/sketchybar"
    if [ -f "$SKETCHYBAR_CONFIG/sketchybarrc" ]; then
        chmod +x "$SKETCHYBAR_CONFIG/sketchybarrc"
    fi

    # Make all plugin scripts executable
    if [ -d "$SKETCHYBAR_CONFIG/plugins" ]; then
        find "$SKETCHYBAR_CONFIG/plugins" -type f -exec chmod +x {} \;
    fi

    # Start sketchybar service if not running
    if ! pgrep -x "sketchybar" > /dev/null; then
        log_info "Starting sketchybar service..."
        brew services start sketchybar 2>/dev/null || true
    else
        log_info "Restarting sketchybar to apply configuration..."
        brew services restart sketchybar 2>/dev/null || true
    fi

    log_success "Sketchybar configured"
}

# ============================================================================
# Yazi Configuration
# ============================================================================

configure_yazi() {
    log_info "Configuring Yazi via stow..."
    stow_package "yazi"

    # Ensure flavors directory exists (needed for ya pkg to work)
    ensure_dir_owned "$XDG_CONFIG_HOME/yazi/flavors"

    # Install monokai flavor for yazi
    if command -v ya &>/dev/null; then
        log_info "Installing yazi monokai flavor..."
        # ya pkg has a bug where it looks for preview.png but package has preview.webp
        # So we install and manually deploy if needed
        ya pkg add malick-tammal/monokai 2>/dev/null || true

        # Check if flavor was deployed, if not, manually copy it
        local FLAVOR_DIR="$XDG_CONFIG_HOME/yazi/flavors/monokai.yazi"
        if [ ! -f "$FLAVOR_DIR/flavor.toml" ]; then
            # Find the package in yazi state directory
            local PKG_DIR
            PKG_DIR=$(find "$HOME/.local/state/yazi/packages" -name "flavor.toml" -path "*monokai*" -exec dirname {} \; 2>/dev/null | head -n1)
            if [ -n "$PKG_DIR" ] && [ -d "$PKG_DIR" ]; then
                log_info "Manually deploying monokai flavor..."
                ensure_dir_owned "$FLAVOR_DIR"
                cp -r "$PKG_DIR"/* "$FLAVOR_DIR/"
            fi
        fi

        if [ -f "$FLAVOR_DIR/flavor.toml" ]; then
            log_success "Yazi monokai flavor installed"
        else
            log_warning "Could not install monokai flavor"
        fi
    else
        log_warning "ya command not found, skipping flavor installation"
    fi

    log_success "Yazi configured"
}

# ============================================================================
# Lazygit Configuration
# ============================================================================

configure_lazygit() {
    log_info "Configuring Lazygit via stow..."
    stow_package "lazygit"
    log_success "Lazygit configured"
}

# ============================================================================
# macOS System Settings
# ============================================================================

configure_macos_settings() {
    log_info "Configuring macOS settings..."

    # Disable press-and-hold for key repeat
    defaults write -g ApplePressAndHoldEnabled -bool false
    log_success "Disabled press-and-hold for key repeat"

    log_success "macOS settings configured"
}

# ============================================================================
# Main Installation Flow
# ============================================================================

main() {
    echo ""
    echo "╔════════════════════════════════════════════════════════════════╗"
    echo "║         macOS Development Environment Setup                    ║"
    echo "║                  (Powered by GNU Stow)                         ║"
    echo "╚════════════════════════════════════════════════════════════════╝"
    echo ""
    # Check for sudo access upfront
    check_sudo
    # Check if running on macOS
    if [[ "$(uname)" != "Darwin" ]]; then
        log_error "This script is intended for macOS only."
        exit 1
    fi

    # Install Homebrew first (needed for all other packages)
    install_homebrew

    # Install all packages (including stow)
    install_packages

    # Copy dotfiles to ~/.dotfiles (before stowing)
    setup_user_dotfiles

    # Setup Oh My Zsh and plugins (uses stow for custom plugin)
    install_oh_my_zsh
    install_zsh_plugins
    configure_zshrc
    set_default_shell

    # Setup tmux with Oh My Tmux (uses stow for tmux.conf.local)
    install_oh_my_tmux

    # Setup Neovim with LazyVim (uses stow for custom plugins)
    install_lazyvim

    # Configure WezTerm (uses stow)
    configure_wezterm

    # Configure VS Code
    configure_vscode

    # Setup Yabai window manager
    configure_yabai

    # Setup Sketchybar status bar
    configure_sketchybar

    # Configure Yazi file manager
    configure_yazi

    # Configure Lazygit
    configure_lazygit

    # Configure macOS-specific settings
    configure_macos_settings

    echo ""
    echo "╔════════════════════════════════════════════════════════════════╗"
    echo "║         Installation Complete! 🎉                              ║"
    echo "╚════════════════════════════════════════════════════════════════╝"
    echo ""
    log_info "Dotfiles are stored in: $USER_DOTFILES_DIR"
    log_info "Dotfiles are managed via GNU Stow from ~/dotfiles"
    log_info "You can now safely delete the setup-config repository."
    log_info "To modify configs, edit files in ~/dotfiles and re-run stow."
    echo ""
    log_info "Please restart your terminal or run 'source ~/.zshrc' to apply changes."
    log_info "You may also need to log out and back in for some macOS settings to take effect."
    echo ""
}

main "$@"
