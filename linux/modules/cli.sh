#!/usr/bin/env bash

module_cli() {
    myconfig_log "Installing command-line tools"
    install_package_ids \
        ripgrep jq fastfetch btop tokei github_cli
}
