#!/usr/bin/env bash

module_ghostty() {
    myconfig_log "Installing Ghostty and removing alternative terminal emulators"
    install_package_ids ghostty
    remove_package_ids kde_utilities_meta kitty alacritty wezterm konsole
}
