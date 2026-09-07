#!/usr/bin/env bash

module_ghostty() {
    myconfig_log "Installing Ghostty"
    install_package_ids ghostty

    if command -v plasmashell >/dev/null 2>&1 && command -v kwriteconfig6 >/dev/null 2>&1; then
        kwriteconfig6 --file kdeglobals --group General \
            --key TerminalApplication "/usr/bin/ghostty --gtk-single-instance=true"
        kwriteconfig6 --file kdeglobals --group General \
            --key TerminalService com.mitchellh.ghostty.desktop
    else
        myconfig_log "KDE Plasma not found; leaving its default terminal unchanged"
    fi
}
