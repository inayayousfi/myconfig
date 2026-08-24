#!/usr/bin/env bash

declare -Ag MYCONFIG_PACKAGE_DEFAULTS=()
declare -Ag MYCONFIG_PACKAGE_OVERRIDES=()

register_package() {
    local id="$1"
    local default_spec="$2"
    shift 2

    [ -n "$id" ] || myconfig_fail "package identifier cannot be empty"
    [ -z "${MYCONFIG_PACKAGE_DEFAULTS[$id]+set}" ] || myconfig_fail "duplicate package identifier: $id"

    MYCONFIG_PACKAGE_DEFAULTS["$id"]="$default_spec"

    local override target spec
    for override in "$@"; do
        target="${override%%=*}"
        spec="${override#*=}"
        [ "$target" != "$override" ] || myconfig_fail "invalid package override: $override"
        [ -n "$target" ] && [ -n "$spec" ] || myconfig_fail "invalid package override: $override"
        MYCONFIG_PACKAGE_OVERRIDES["$target:$id"]="$spec"
    done
}

resolve_package() {
    local id="$1"
    local override_key="$MYCONFIG_PROFILE:$id"
    local spec

    if [ -n "${MYCONFIG_PACKAGE_OVERRIDES[$override_key]+set}" ]; then
        spec="${MYCONFIG_PACKAGE_OVERRIDES[$override_key]}"
    elif [ -n "${MYCONFIG_PACKAGE_DEFAULTS[$id]+set}" ]; then
        spec="${MYCONFIG_PACKAGE_DEFAULTS[$id]}"
    else
        myconfig_fail "unknown package identifier: $id"
        return 1
    fi

    [ -n "$spec" ] || {
        myconfig_fail "package $id has no mapping for $MYCONFIG_PROFILE"
        return 1
    }

    local source="${spec%%:*}"
    local package="${spec#*:}"
    [ "$source" != "$spec" ] && [ -n "$source" ] && [ -n "$package" ] || {
        myconfig_fail "invalid package mapping for $id: $spec"
        return 1
    }

    adapter_supports_source "$source" || {
        myconfig_fail "package source $source is unsupported by $MYCONFIG_ADAPTER"
        return 1
    }

    printf '%s\n' "$spec"
}

install_package_ids() {
    local specs=()
    local id

    for id in "$@"; do
        specs+=("$(resolve_package "$id")")
    done

    adapter_install_specs "${specs[@]}"
}

remove_package_ids() {
    local specs=()
    local id

    for id in "$@"; do
        specs+=("$(resolve_package "$id")")
    done

    adapter_remove_specs "${specs[@]}"
}
