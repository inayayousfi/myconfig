#!/usr/bin/env bash

module_flatpak() {
    myconfig_log "Installing Flatpak with Flathub"
    install_package_ids flatpak

    sudo flatpak remote-add --if-not-exists flathub \
        https://dl.flathub.org/repo/flathub.flatpakrepo
}
