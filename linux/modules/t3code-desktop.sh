#!/usr/bin/env bash

# The desktop app runs its own T3 Code server on ~/.t3, so it replaces the
# background service instead of running beside it.
module_t3code_desktop() {
    myconfig_log "Installing the T3 Code desktop app"
    install_package_ids t3code_desktop

    [ -L "$HOME/.t3/userdata/client-settings.json" ] \
        || myconfig_fail "T3 Code client settings were not stowed"
}
