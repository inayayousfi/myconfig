#!/usr/bin/env bash

user_is_in_group() {
    local wanted="$1"
    local group
    local target_user

    target_user="$(id -un)"
    for group in $(id -Gn) $(id -nG "$target_user"); do
        [ "$group" = "$wanted" ] && return 0
    done
    return 1
}

module_docker() {
    [ "$MYCONFIG_PROFILE" = cachyos ] || myconfig_fail "Docker is supported only on CachyOS"

    require_command id
    require_command systemctl
    require_function user_is_in_group

    if user_is_in_group docker; then
        myconfig_fail "current user is already in the docker group; refusing Docker setup"
        return 1
    fi

    myconfig_log "Installing Docker Engine, CLI, Buildx, and Compose"
    install_package_ids docker docker_buildx docker_compose

    sudo systemctl enable --now docker.service
    [ "$(systemctl is-active docker.service)" = active ] \
        || myconfig_fail "docker.service is not active"
}
