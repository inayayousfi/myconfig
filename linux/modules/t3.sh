#!/usr/bin/env bash

module_t3() {
    myconfig_log "Installing and configuring T3 Code"

    if [ "$MYCONFIG_PROFILE" = cachyos ]; then
        install_package_ids t3code_desktop
    fi

    [ -x "$HOME/.local/bin/t3-setup" ] \
        || myconfig_fail "t3-setup was not installed by the Zsh dotfiles"

    "$HOME/.local/bin/t3-setup" </dev/null
}
