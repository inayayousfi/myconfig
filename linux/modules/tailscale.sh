#!/usr/bin/env bash

module_tailscale() {
    myconfig_log "Installing Tailscale"
    install_package_ids tailscale

    sudo systemctl enable --now tailscaled
    [ "$(systemctl is-active tailscaled)" = active ] \
        || myconfig_fail "tailscaled is not active"

    # t3ctl publishes T3 Code through Tailscale Serve without sudo.
    require_command jq
    local user
    user="$(id -un)"
    sudo tailscale set --operator="$user"
    [ "$(tailscale debug prefs | jq -r '.OperatorUser')" = "$user" ] \
        || myconfig_fail "$user is not the Tailscale operator"
}
