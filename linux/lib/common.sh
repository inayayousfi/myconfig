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
