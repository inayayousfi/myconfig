#!/usr/bin/env bash

module_base() {
    myconfig_log "Installing base packages"

    if [ "$MYCONFIG_PROFILE" = ubuntu-server ]; then
        install_package_ids ca_certificates curl git rsync stow
        return
    fi

    install_package_ids \
        ca_certificates sudo git curl wget rsync stow tar unzip zip xz file \
        man_db man_pages base_devel rustup polkit
}
