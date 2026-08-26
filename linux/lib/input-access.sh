#!/usr/bin/env bash

user_has_active_group() {
    local wanted="$1"
    local group

    for group in $(id -Gn); do
        [ "$group" = "$wanted" ] && return 0
    done
    return 1
}

configure_named_input_access() {
    local owner="$1"
    [[ "$owner" =~ ^[a-z0-9-]+$ ]] || {
        myconfig_fail "input-access owner must contain only lowercase letters, digits, and hyphens: $owner"
        return 1
    }

    require_command id
    require_command sudo
    require_command udevadm

    local target_user
    target_user="$(id -un)"

    sudo groupadd --system --force input
    sudo groupadd --system --force uinput
    sudo usermod -aG input,uinput "$target_user"
    sudo modprobe uinput
    sudo install -d /etc/modules-load.d /etc/udev/rules.d
    printf '%s\n' uinput \
        | sudo tee "/etc/modules-load.d/myconfig-$owner.conf" >/dev/null
    printf '%s\n' \
        'KERNEL=="uinput", MODE="0660", GROUP="uinput", OPTIONS+="static_node=uinput"' \
        'SUBSYSTEM=="input", KERNEL=="event*", MODE="0660", GROUP="input"' \
        | sudo tee "/etc/udev/rules.d/99-myconfig-$owner.rules" >/dev/null
    sudo udevadm control --reload-rules
    sudo udevadm trigger --subsystem-match=misc --sysname-match=uinput
    sudo udevadm trigger --subsystem-match=input
}
