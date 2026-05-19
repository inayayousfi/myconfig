#!/bin/bash

# Ubuntu Server Zsh Uninstall Script

set -e

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

confirm_uninstall() {
    echo ""
    echo "Ubuntu Server Zsh Uninstaller"
    echo "=============================="
    echo ""
    log_warning "This script will remove managed .zshrc, Oh My Zsh, and installed zsh plugins/themes."
    echo ""

    read -r -p "Are you sure you want to continue? (yes/no): " confirm
    if [[ "$confirm" != "yes" ]]; then
        log_info "Uninstall cancelled."
        exit 0
    fi
    echo ""
}

remove_zshrc() {
    if [ -f "$HOME/.zshrc" ] && grep -q "# Managed by setup-config" "$HOME/.zshrc"; then
        log_info "Removing managed .zshrc..."
        rm -f "$HOME/.zshrc"
        log_success ".zshrc removed"

        local latest_backup
        latest_backup=$(ls -t "$HOME"/.zshrc.backup.* 2>/dev/null | head -n1 || true)
        if [ -n "$latest_backup" ]; then
            log_info "Restoring backup: $latest_backup"
            mv "$latest_backup" "$HOME/.zshrc"
            log_success "Backup restored"
        fi
    else
        log_warning ".zshrc not managed by setup-config or not found"
    fi
}

remove_oh_my_zsh() {
    if [ -d "$HOME/.oh-my-zsh" ]; then
        log_info "Removing Oh My Zsh..."
        rm -rf "$HOME/.oh-my-zsh"
        log_success "Oh My Zsh removed"
    else
        log_warning "Oh My Zsh not found"
    fi
}

restore_bash_shell() {
    local current_shell
    current_shell=$(getent passwd "$USER" | cut -d: -f7)

    if [[ "$current_shell" == */zsh ]]; then
        log_info "Restoring default shell to bash..."
        chsh -s /bin/bash || true
        log_success "Default shell restored to bash"
    fi
}

main() {
    if [[ "$(uname)" != "Linux" ]]; then
        log_error "This script is intended for Linux only."
        exit 1
    fi

    confirm_uninstall
    remove_zshrc
    remove_oh_my_zsh
    restore_bash_shell

    echo ""
    log_success "Ubuntu Server zsh uninstall complete"
    log_info "Restart your terminal for all changes to take effect."
    echo ""
}

main "$@"
