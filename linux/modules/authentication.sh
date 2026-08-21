#!/usr/bin/env bash

offer_authentication() {
    local name="$1"
    local command_text="$2"
    shift 2

    if ! { exec 9<>/dev/tty; } 2>/dev/null; then
        myconfig_log "$name is not authenticated. Run: $command_text"
        return 0
    fi

    local answer
    printf '%s is not authenticated. Log in now? [y/N] ' "$name" >&9
    IFS= read -r answer <&9

    case "$answer" in
        y | Y | yes | YES | Yes)
            "$@" <&9 >&9 2>&9
            ;;
        *)
            myconfig_log "Skipped $name authentication. Run: $command_text"
            ;;
    esac

    exec 9>&-
}

module_authentication() {
    myconfig_log "Configuring development authentication"
    git config --global core.symlinks true

    if [ "$MYCONFIG_PROFILE" = arch-wsl ] && [ -n "${MYCONFIG_WINDOWS_SSH:-}" ]; then
        [ -x "$MYCONFIG_WINDOWS_SSH" ] || myconfig_fail "Windows SSH client is not executable: $MYCONFIG_WINDOWS_SSH"
        git config --global core.sshCommand "$MYCONFIG_WINDOWS_SSH"
    fi

    if ! gh auth status >/dev/null 2>&1; then
        offer_authentication "GitHub CLI" "gh auth login" gh auth login
    fi

    if ! tailscale status >/dev/null 2>&1; then
        offer_authentication "Tailscale" "sudo tailscale up" sudo tailscale up
    fi
}
