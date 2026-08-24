#!/usr/bin/env bash

myconfig_log() {
    printf '[myconfig][%s] %s\n' "$MYCONFIG_PROFILE" "$*"
}

myconfig_fail() {
    printf '[myconfig][%s] ERROR: %s\n' "$MYCONFIG_PROFILE" "$*" >&2
    return 1
}

require_command() {
    command -v "$1" >/dev/null 2>&1 || myconfig_fail "required command not found: $1"
}

require_function() {
    declare -F "$1" >/dev/null 2>&1 || myconfig_fail "adapter function not found: $1"
}

SUDO_KEEPALIVE_PID=""

start_sudo_keepalive() {
    require_command sudo
    sudo -v
    (
        while sleep 60; do
            sudo -n true || break
        done
    ) &
    SUDO_KEEPALIVE_PID=$!
    trap stop_sudo_keepalive EXIT
}

stop_sudo_keepalive() {
    if [ -n "$SUDO_KEEPALIVE_PID" ] && kill -0 "$SUDO_KEEPALIVE_PID" 2>/dev/null; then
        kill "$SUDO_KEEPALIVE_PID" 2>/dev/null || true
        wait "$SUDO_KEEPALIVE_PID" 2>/dev/null || true
    fi
    SUDO_KEEPALIVE_PID=""
}

unique_backup_path() {
    local base="$1.backup.$(date +%Y%m%d_%H%M%S)"
    local candidate="$base"
    local suffix=1

    while [ -e "$candidate" ] || [ -L "$candidate" ]; do
        candidate="$base.$suffix"
        suffix=$((suffix + 1))
    done

    printf '%s\n' "$candidate"
}
