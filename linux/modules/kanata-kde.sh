#!/usr/bin/env bash

module_kanata_kde() {
    [ "$MYCONFIG_PROFILE" = cachyos ] || {
        myconfig_fail "Kanata KDE integration is only supported by the CachyOS profile"
        return 1
    }
    module_kde_plasma_validate || return 1

    myconfig_log "Configuring the Kanata KDE tray"
    install_package_ids python pyside6

    require_command kwriteconfig6
    require_command python
    require_command systemctl

    local tray="$HOME/.local/bin/myconfig-kanata-tray"
    local service="$HOME/.config/systemd/user/myconfig-kanata-tray.service"
    local engine_enablement="$HOME/.config/systemd/user/default.target.wants/myconfig-kanata.service"
    [ -x "$tray" ] || myconfig_fail "Kanata KDE tray was not stowed as an executable"
    [ -f "$service" ] || myconfig_fail "Kanata KDE tray service was not stowed"
    python -m py_compile "$tray"

    kwriteconfig6 \
        --file kglobalshortcutsrc \
        --group kwin \
        --key Overview \
        'Meta+W,Meta+W,Toggle Overview'

    rm -f "$engine_enablement"
    systemctl --user enable myconfig-kanata-tray.service
    systemctl --user daemon-reload
    if ! user_has_active_group input || ! user_has_active_group uinput; then
        systemctl --user stop myconfig-kanata.service
        myconfig_log "Log out and back in before the Kanata engine and tray start together"
    elif systemctl --user --quiet is-active graphical-session.target; then
        systemctl --user restart myconfig-kanata-tray.service
        systemctl --user --quiet is-active myconfig-kanata-tray.service \
            || myconfig_fail "Kanata KDE tray service did not become active"
    else
        systemctl --user stop myconfig-kanata.service
        myconfig_log "The Kanata engine and tray are enabled for the next KDE Plasma login"
    fi
}
