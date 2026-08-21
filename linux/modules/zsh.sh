#!/usr/bin/env bash

module_zsh() {
    myconfig_log "Installing Zsh and Oh My Zsh"
    install_package_ids zsh

    if [ ! -d "$HOME/.oh-my-zsh" ]; then
        RUNZSH=no CHSH=no KEEP_ZSHRC=yes \
            sh -c "$(curl -fsSL https://raw.githubusercontent.com/ohmyzsh/ohmyzsh/master/tools/install.sh)"
    fi

    local custom="${ZSH_CUSTOM:-$HOME/.oh-my-zsh/custom}"
    mkdir -p "$custom/plugins"

    if [ ! -d "$custom/plugins/zsh-autosuggestions" ]; then
        git clone https://github.com/zsh-users/zsh-autosuggestions "$custom/plugins/zsh-autosuggestions"
    fi

    if [ ! -d "$custom/plugins/zsh-syntax-highlighting" ]; then
        git clone https://github.com/zsh-users/zsh-syntax-highlighting.git "$custom/plugins/zsh-syntax-highlighting"
    fi

    if [ "$MYCONFIG_PROFILE" != arch-wsl ]; then
        local zsh_path current_shell
        zsh_path="$(command -v zsh)"
        current_shell="$(getent passwd "$USER" | cut -d: -f7)"
        if [ "$current_shell" != "$zsh_path" ]; then
            sudo chsh -s "$zsh_path" "$USER"
        fi
    fi
}
