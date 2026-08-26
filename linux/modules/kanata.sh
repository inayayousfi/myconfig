#!/usr/bin/env bash

user_has_active_group() {
    local wanted="$1"
    local group

    for group in $(id -Gn); do
        [ "$group" = "$wanted" ] && return 0
    done
    return 1
}

module_kanata() {
    myconfig_log "Installing and configuring Kanata"
    install_package_ids kanata

    require_command id
    require_command kanata
    require_command systemctl
    require_command sudo
    require_command udevadm

    local config="$HOME/.config/kanata/config.kbd"
    local service="$HOME/.config/systemd/user/myconfig-kanata.service"
    [ -f "$config" ] || myconfig_fail "Kanata configuration was not stowed"
    [ -f "$service" ] || myconfig_fail "Kanata user service was not stowed"
    kanata --check --cfg "$config"

    local target_user
    target_user="$(id -un)"

    sudo groupadd --system --force input
    sudo groupadd --system --force uinput
    sudo usermod -aG input,uinput "$target_user"
    sudo modprobe uinput
    sudo install -d /etc/modules-load.d /etc/udev/rules.d
    printf '%s\n' uinput \
        | sudo tee /etc/modules-load.d/myconfig-kanata.conf >/dev/null
    printf '%s\n' \
        'KERNEL=="uinput", MODE="0660", GROUP="uinput", OPTIONS+="static_node=uinput"' \
        'SUBSYSTEM=="input", KERNEL=="event*", MODE="0660", GROUP="input"' \
        | sudo tee /etc/udev/rules.d/99-myconfig-kanata.rules >/dev/null
    sudo udevadm control --reload-rules
    sudo udevadm trigger --subsystem-match=misc --sysname-match=uinput
    sudo udevadm trigger --subsystem-match=input

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
