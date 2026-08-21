#!/usr/bin/env bash

adapter_supports_source() {
    [ "$1" = official ]
}

adapter_prepare() {
    require_command sudo
    require_command apt-get
    sudo -v
    myconfig_log "Updating apt package databases"
    sudo apt-get update
}

adapter_install_specs() {
    local packages=()
    local spec source package

    for spec in "$@"; do
        source="${spec%%:*}"
        package="${spec#*:}"
        [ "$source" = official ] || myconfig_fail "unsupported apt package source: $source"
        packages+=("$package")
    done

    if ((${#packages[@]})); then
        sudo apt-get install -y "${packages[@]}"
    fi
}
