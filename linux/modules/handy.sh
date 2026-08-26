#!/usr/bin/env bash

module_handy() {
    [ "$MYCONFIG_PROFILE" = cachyos ] || {
        myconfig_fail "Handy is only supported by the CachyOS profile"
        return 1
    }

    myconfig_log "Installing and configuring Handy push-to-talk dictation"
    install_package_ids handy

    require_command handy
    require_command jq
    require_command systemctl
    require_function configure_named_input_access
    require_function user_has_active_group

    local configure="$HOME/.local/bin/myconfig-handy-configure"
    local service="$HOME/.config/systemd/user/myconfig-handy.service"
    [ -x "$configure" ] || myconfig_fail "Handy settings updater was not stowed as an executable"
    [ -f "$service" ] || myconfig_fail "Handy user service was not stowed"

    configure_named_input_access handy

    systemctl --user daemon-reload
    systemctl --user stop myconfig-handy.service
    "$configure"
    systemctl --user enable myconfig-handy.service

    if ! user_has_active_group input || ! user_has_active_group uinput; then
        myconfig_log "Log out and back in before Handy can read keyboard input; its service is enabled for the next login"
    elif systemctl --user --quiet is-active graphical-session.target; then
        systemctl --user restart myconfig-handy.service
        systemctl --user --quiet is-active myconfig-handy.service \
            || myconfig_fail "Handy user service did not become active"
    else
        myconfig_log "Handy is enabled for the next KDE Plasma login"
    fi
}
