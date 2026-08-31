#!/usr/bin/env bash

TMUX_ATELIER_INSTALLER_URL="https://raw.githubusercontent.com/inayayousfi/tmux-atelier/main/install.sh"
TMUX_ATELIER_DIR="$HOME/.config/tmux/tmux-atelier"

module_tmux() {
    myconfig_log "Installing tmux and tmux-atelier"
    install_package_ids tmux
    remove_package_ids herdr

    if [ "$MYCONFIG_PROFILE" = cachyos ]; then
        install_package_ids wl_clipboard
    fi

    if [ -d "$HOME/dotfiles/herdr" ]; then
        stow --dir "$HOME/dotfiles" --target "$HOME" --delete herdr
    fi

    if ! curl -fsSL "$TMUX_ATELIER_INSTALLER_URL" \
        | bash -s -- --install-dir "$TMUX_ATELIER_DIR"; then
        myconfig_fail "could not install tmux-atelier"
        return 1
    fi
}
