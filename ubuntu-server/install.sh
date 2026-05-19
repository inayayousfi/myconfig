#!/bin/bash

# Ubuntu Server Zsh Setup Script
# This script is idempotent - running it multiple times is safe.

# set -e (Disabled to ensure script continues even if some steps fail)

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(dirname "$SCRIPT_DIR")"
SHARED_ZSH_DIR="$REPO_ROOT/dotfiles/zsh"

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

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

check_sudo() {
    log_info "This script requires sudo privileges to install packages and set the shell."

    if sudo -v; then
        while true; do sudo -n true; sleep 60; kill -0 "$$" || exit; done 2>/dev/null &
        log_success "Sudo access granted"
    else
        log_error "Sudo access is required."
        exit 1
    fi
}

install_packages() {
    log_info "Installing zsh and minimal prerequisites with apt..."
    sudo apt-get update
    sudo apt-get install -y ca-certificates curl git zsh
    log_success "Minimal packages installed"
}

install_oh_my_zsh() {
    if [ -d "$HOME/.oh-my-zsh" ]; then
        log_success "Oh My Zsh is already installed"
        return
    fi

    log_info "Installing Oh My Zsh..."
    RUNZSH=no KEEP_ZSHRC=yes sh -c "$(curl -fsSL https://raw.githubusercontent.com/ohmyzsh/ohmyzsh/master/tools/install.sh)"
    log_success "Oh My Zsh installed"
}

install_zsh_plugins() {
    local zsh_custom="${ZSH_CUSTOM:-$HOME/.oh-my-zsh/custom}"
    mkdir -p "$zsh_custom/plugins" "$zsh_custom/themes"

    if [ -d "$zsh_custom/plugins/zsh-autosuggestions" ]; then
        log_success "zsh-autosuggestions is already installed"
    else
        log_info "Installing zsh-autosuggestions..."
        git clone https://github.com/zsh-users/zsh-autosuggestions "$zsh_custom/plugins/zsh-autosuggestions"
        log_success "zsh-autosuggestions installed"
    fi

    if [ -d "$zsh_custom/plugins/zsh-syntax-highlighting" ]; then
        log_success "zsh-syntax-highlighting is already installed"
    else
        log_info "Installing zsh-syntax-highlighting..."
        git clone https://github.com/zsh-users/zsh-syntax-highlighting "$zsh_custom/plugins/zsh-syntax-highlighting"
        log_success "zsh-syntax-highlighting installed"
    fi

    log_info "Installing shared zsh theme and custom plugin..."
    cp -a "$SHARED_ZSH_DIR/.oh-my-zsh/custom/plugins/inaya" "$zsh_custom/plugins/"
    cp -a "$SHARED_ZSH_DIR/.oh-my-zsh/custom/themes/blacknpink.zsh-theme" "$zsh_custom/themes/"
    log_success "Shared zsh files installed"
}

configure_zshrc() {
    local zshrc="$HOME/.zshrc"

    log_info "Configuring .zshrc..."

    if [ -f "$zshrc" ] && ! grep -q "# Managed by setup-config" "$zshrc"; then
        cp "$zshrc" "$zshrc.backup.$(date +%Y%m%d%H%M%S)"
        log_info "Backed up existing .zshrc"
    fi

    cp "$SHARED_ZSH_DIR/.zshrc" "$zshrc"
    log_success ".zshrc configured"
}

set_default_shell() {
    local zsh_path
    zsh_path="$(command -v zsh)"

    if [ -z "$zsh_path" ]; then
        log_error "Zsh not found in PATH"
        return 1
    fi

    if [ "$(getent passwd "$USER" | cut -d: -f7)" = "$zsh_path" ]; then
        log_success "Zsh is already the default shell"
        return 0
    fi

    log_info "Setting zsh as default shell ($zsh_path)..."

    if ! grep -Fxq "$zsh_path" /etc/shells; then
        echo "$zsh_path" | sudo tee -a /etc/shells > /dev/null
    fi

    if sudo chsh -s "$zsh_path" "$USER"; then
        log_success "Zsh set as default shell"
        export SHELL="$zsh_path"
    else
        log_warning "Failed to set zsh as default shell"
        log_info "Run 'sudo chsh -s $zsh_path $USER' manually if needed."
    fi
}

main() {
    echo ""
    echo "Ubuntu Server Zsh Setup"
    echo "========================"
    echo ""

    if [[ "$(uname)" != "Linux" ]]; then
        log_error "This script is intended for Linux only."
        exit 1
    fi

    if ! command -v apt-get >/dev/null 2>&1; then
        log_error "This script requires apt-get and is intended for Ubuntu Server."
        exit 1
    fi

    if [ ! -d "$SHARED_ZSH_DIR" ]; then
        log_error "Shared zsh dotfiles not found at $SHARED_ZSH_DIR"
        exit 1
    fi

    check_sudo
    install_packages
    install_oh_my_zsh
    install_zsh_plugins
    configure_zshrc
    set_default_shell

    echo ""
    log_success "Ubuntu Server zsh setup complete"
    log_info "Restart your terminal or run 'source ~/.zshrc' to apply changes."
    log_info "You may need to log out and back in for the default shell change to take effect."
    echo ""
}

main "$@"
