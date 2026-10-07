#!/usr/bin/env bash

module_authentication() {
    myconfig_log "Configuring development authentication"
    git config --global core.symlinks true

    if [ "$MYCONFIG_PROFILE" = arch-wsl ] && [ -n "${MYCONFIG_WINDOWS_SSH:-}" ]; then
        [ -x "$MYCONFIG_WINDOWS_SSH" ] || myconfig_fail "Windows SSH client is not executable: $MYCONFIG_WINDOWS_SSH"
        git config --global core.sshCommand "$MYCONFIG_WINDOWS_SSH"
    fi

    if ! gh auth status >/dev/null 2>&1; then
        offer_action "GitHub CLI is not authenticated." "Log in now?" "gh auth login" gh auth login
    fi

    if ! tailscale status >/dev/null 2>&1; then
        offer_action "Tailscale is not authenticated." "Log in now?" "sudo tailscale up" sudo tailscale up
    fi
}
