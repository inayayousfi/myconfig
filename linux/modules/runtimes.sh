#!/usr/bin/env bash

module_runtimes() {
    myconfig_log "Installing development runtimes"
    install_package_ids go python python_venv jdk maven llvm make cmake nodejs npm node_gyp

    if [ "$MYCONFIG_PROFILE" = ubuntu-devbox ]; then
        install_bun_script
    else
        install_package_ids bun
    fi

    rustup default stable
}

# Ubuntu has no Bun package. Bun's installer puts it in ~/.bun, where the agent
# module expects it, and `bun upgrade` updates it later.
install_bun_script() {
    export BUN_INSTALL="$HOME/.bun"
    [ -x "$BUN_INSTALL/bin/bun" ] && return 0

    local installer
    installer="$(curl -fsSL https://bun.sh/install)" \
        || myconfig_fail "Could not download the Bun installer" || return 1
    # Without a known shell the installer leaves the shell files alone; the
    # stowed Zsh configuration already puts ~/.bun/bin on PATH.
    SHELL=/bin/sh bash -c "$installer" \
        || myconfig_fail "Bun installer failed" || return 1
    [ -x "$BUN_INSTALL/bin/bun" ] || myconfig_fail "Bun was not installed at $BUN_INSTALL/bin/bun"
}
