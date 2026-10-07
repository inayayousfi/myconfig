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
    [ "$MYCONFIG_PROFILE" = cachyos ] || myconfig_fail "Docker is supported only on CachyOS" || return 1

    require_command id || return 1
    require_command systemctl || return 1
    require_function user_is_in_group || return 1

    if user_is_in_group docker; then
        myconfig_fail "current user is already in the docker group; refusing Docker setup"
        return 1
    fi

    myconfig_log "Installing rootless Docker Engine, CLI, Buildx, and Compose"
    install_package_ids docker docker_buildx docker_compose \
        docker_rootless_extras slirp4netns

    myconfig_log "Disabling the system Docker daemon"
    sudo systemctl disable --now docker.service docker.socket

    myconfig_log "Starting the rootless Docker daemon"
    systemctl --user daemon-reload
    systemctl --user enable --now docker.service
    [ "$(systemctl --user is-active docker.service)" = active ] \
        || myconfig_fail "the rootless docker.service user unit is not active" || return 1

    docker context inspect rootless >/dev/null 2>&1 \
        || docker context create rootless \
            --description "Rootless Docker daemon" \
            --docker "host=unix:///run/user/$(id -u)/docker.sock"
    docker context use rootless

    docker info --format '{{.SecurityOptions}}' | grep -Fq 'name=rootless' \
        || myconfig_fail "the selected Docker daemon is not rootless"
}
