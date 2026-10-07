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

# Asks on the terminal before running an optional step. Without a terminal, or
# on any answer but yes, it prints the command that does the step later.
offer_action() {
    local state="$1"
    local question="$2"
    local command_text="$3"
    shift 3

    if ! { exec 9<>"${MYCONFIG_TTY_PATH:-/dev/tty}"; } 2>/dev/null; then
        myconfig_log "$state Run: $command_text"
        return 0
    fi

    local answer
    printf '%s %s [y/N] ' "$state" "$question" >&9
    IFS= read -r answer <&9

    case "$answer" in
        y | Y | yes | YES | Yes)
            "$@" <&9 >&9 2>&9
            ;;
        *)
            myconfig_log "$state Run: $command_text"
            ;;
    esac

    exec 9>&-
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
