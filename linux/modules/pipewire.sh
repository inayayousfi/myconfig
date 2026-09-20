#!/usr/bin/env bash

module_pipewire() {
    [ "$MYCONFIG_PROFILE" = cachyos ] || return 0

    myconfig_log "Configuring the PipeWire mono playback toggle"
    install_package_ids python pyside6

    require_command python
    require_command systemctl

    local tray="$HOME/.local/bin/myconfig-pipewire-tray"
    local service="$HOME/.config/systemd/user/myconfig-pipewire-tray.service"
    [ -x "$tray" ] || myconfig_fail "PipeWire tray was not stowed as an executable"
    [ -f "$service" ] || myconfig_fail "PipeWire tray service was not stowed"
    python -m py_compile "$tray"

    systemctl --user enable myconfig-pipewire-tray.service
    systemctl --user daemon-reload
    if systemctl --user --quiet is-active graphical-session.target; then
        systemctl --user restart myconfig-pipewire-tray.service
        systemctl --user --quiet is-active myconfig-pipewire-tray.service \
            || myconfig_fail "PipeWire tray service did not become active"
    else
        systemctl --user stop myconfig-pipewire-tray.service
        myconfig_log "The PipeWire mono toggle will start at the next KDE Plasma login"
    fi
}
