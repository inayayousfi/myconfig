#!/usr/bin/env bash

module_cli() {
    myconfig_log "Installing command-line tools"
    remove_package_ids fd fzf zoxide eza bat hunk neovim lazygit tmux
    install_package_ids \
        ripgrep jq fastfetch btop tokei github_cli
}
