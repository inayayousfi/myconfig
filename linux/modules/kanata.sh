#!/usr/bin/env bash

module_kanata() {
    myconfig_log "Installing and configuring Kanata"
    install_package_ids kanata

    require_command kanata
    require_command systemctl
    require_function configure_named_input_access
    require_function user_has_active_group

    local config="$HOME/.config/kanata/config.kbd"
    local service="$HOME/.config/systemd/user/myconfig-kanata.service"
    [ -f "$config" ] || myconfig_fail "Kanata configuration was not stowed"
    [ -f "$service" ] || myconfig_fail "Kanata user service was not stowed"
    kanata --check --cfg "$config"

    configure_named_input_access kanata

    systemctl --user daemon-reload
    systemctl --user enable myconfig-kanata.service

    if user_has_active_group input && user_has_active_group uinput; then
        systemctl --user restart myconfig-kanata.service
        systemctl --user --quiet is-active myconfig-kanata.service \
            || myconfig_fail "Kanata user service did not become active"
    else
        myconfig_log "Log out and back in before Kanata can access input devices; its service is enabled for the next login"
    fi
}
