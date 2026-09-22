#!/usr/bin/env bash

module_emacs() {
    [ "$MYCONFIG_PROFILE" = cachyos ] || {
        myconfig_fail "Emacs Atelier is only supported by the CachyOS profile"
        return 1
    }

    myconfig_log "Installing Emacs Atelier"
    install_package_ids emacs_wayland sshfs iosevka_font ufw

    local config="$HOME/.config/emacs"
    [ -f "$config/early-init.el" ] || myconfig_fail "Emacs early init was not stowed"
    [ -f "$config/init.el" ] || myconfig_fail "Emacs init was not stowed"
    [ -d "$config/lisp" ] || myconfig_fail "Emacs modules were not stowed"

    if [ -e "$HOME/.emacs.d" ] || [ -L "$HOME/.emacs.d" ]; then
        local legacy_backup
        legacy_backup="$(unique_backup_path "$HOME/.emacs.d")"
        myconfig_log "Backing up legacy Emacs directory to $legacy_backup"
        mv "$HOME/.emacs.d" "$legacy_backup"
    fi

    local subnet
    for subnet in 10.0.0.0/8 172.16.0.0/12 192.168.0.0/16; do
        sudo ufw allow in proto tcp from "$subnet" to any port 18080,18081 \
            comment 'myconfig Emacs browser terminal'
    done
    sudo ufw --force enable

}
