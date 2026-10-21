#!/usr/bin/env bash

# The stowed t3ctl owns the install and the service, so a later `t3ctl repair`
# does exactly what the installer did.
module_t3code() {
    myconfig_log "Installing the T3 Code server"
    require_command npm
    require_command systemctl

    local t3ctl="$HOME/.local/bin/t3ctl"
    [ -x "$t3ctl" ] || myconfig_fail "t3ctl was not stowed from the t3code package"

    "$t3ctl" repair || myconfig_fail "t3ctl could not install the T3 Code server"
    systemctl --user --quiet is-active t3code.service \
        || myconfig_fail "t3code.service is not active"
}
