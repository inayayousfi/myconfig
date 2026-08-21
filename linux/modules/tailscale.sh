#!/usr/bin/env bash

module_tailscale() {
    myconfig_log "Installing Tailscale"
    install_package_ids tailscale

    sudo systemctl enable --now tailscaled
    [ "$(systemctl is-active tailscaled)" = active ] \
        || myconfig_fail "tailscaled is not active"
}
