#!/usr/bin/env bash

module_emacs() {
    [ "$MYCONFIG_PROFILE" = cachyos ] || {
        myconfig_fail "Emacs Atelier is only supported by the CachyOS profile"
        return 1
    }

    myconfig_log "Installing Emacs Atelier"
    install_package_ids emacs sshfs iosevka_font

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

    systemctl --user disable --now emacs.service >/dev/null 2>&1 || true
    rm -f \
        "$HOME/.config/systemd/user/emacs.service.d/myconfig.conf" \
        "$HOME/.local/bin/myconfig-emacs-client" \
        "$HOME/.local/share/applications/emacs.desktop"
    rmdir "$HOME/.config/systemd/user/emacs.service.d" 2>/dev/null || true
    systemctl --user daemon-reload

    if command -v update-desktop-database >/dev/null 2>&1; then
        update-desktop-database "$HOME/.local/share/applications"
    fi
    if command -v kbuildsycoca6 >/dev/null 2>&1; then
        kbuildsycoca6 --noincremental
    fi
}
