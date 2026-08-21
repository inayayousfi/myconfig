#!/usr/bin/env bash

adapter_supports_source() {
    case "$1" in
        official | aur) return 0 ;;
        *) return 1 ;;
    esac
}

adapter_prepare() {
    require_command sudo
    require_command pacman
    sudo -v
    myconfig_log "Updating Arch package databases"
    sudo pacman -Syu --noconfirm
}

adapter_ensure_paru() {
    command -v paru >/dev/null 2>&1 && return 0

    myconfig_log "Bootstrapping paru"
    sudo pacman -S --needed --noconfirm base-devel git

    local build_dir
    build_dir="$(mktemp -d)"
    git clone https://aur.archlinux.org/paru.git "$build_dir/paru"
    (
        cd "$build_dir/paru"
        makepkg -si --noconfirm --needed
    )
    rm -rf "$build_dir"
}

adapter_install_specs() {
    local official=()
    local aur=()
    local spec source package

    for spec in "$@"; do
        source="${spec%%:*}"
        package="${spec#*:}"
        case "$source" in
            official) official+=("$package") ;;
            aur) aur+=("$package") ;;
            *) myconfig_fail "unsupported Arch package source: $source" ;;
        esac
    done

    if ((${#official[@]})); then
        sudo pacman -S --needed --noconfirm "${official[@]}"
    fi

    if ((${#aur[@]})); then
        adapter_ensure_paru
        paru -S --needed --noconfirm --skipreview "${aur[@]}"
    fi
}
