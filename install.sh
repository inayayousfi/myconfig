#!/usr/bin/env bash
# Installs the myconfig binary for this Linux platform and opens it.
#
# Usage:
#   curl -fsSL https://raw.githubusercontent.com/inayayousfi/myconfig/main/install.sh | bash
#   curl -fsSL https://raw.githubusercontent.com/inayayousfi/myconfig/main/install.sh | bash -s -- cachyos
#   curl -fsSL https://raw.githubusercontent.com/inayayousfi/myconfig/main/install.sh | bash -s -- cachyos install
#
# The first argument may name the platform (cachyos, arch-wsl, ubuntu-server). Every
# other argument goes to the binary; with none, the binary opens its screen.

set -euo pipefail

RELEASE_URL="${MYCONFIG_RELEASE_URL:-https://github.com/inayayousfi/myconfig/releases/latest/download}"
BINARY="${HOME}/.local/bin/myconfig"

log() {
    printf '[myconfig] %s\n' "$*" >&2
}

fail() {
    log "error: $*"
    exit 1
}

# Questions read from the terminal, because under `curl | bash` standard input is the script.
has_terminal() {
    { : </dev/tty; } 2>/dev/null
}

ask() {
    local answer
    printf '%s' "$1" >/dev/tty
    IFS= read -r answer </dev/tty
    printf '%s' "$answer"
}

normalize_platform() {
    case "$(printf '%s' "$1" | tr '[:upper:]' '[:lower:]')" in
        cachyos) printf 'cachyos' ;;
        arch-wsl | wsl) printf 'arch-wsl' ;;
        ubuntu-server | ubuntu | linux | server) printf 'ubuntu-server' ;;
        *) return 1 ;;
    esac
}

detect_platform() {
    local id=""
    [ -r /etc/os-release ] && id="$(. /etc/os-release && printf '%s' "${ID:-}")"
    local kernel=""
    [ -r /proc/sys/kernel/osrelease ] && kernel="$(tr '[:upper:]' '[:lower:]' </proc/sys/kernel/osrelease)"
    if [ "$id" = cachyos ]; then
        printf 'cachyos'
    elif [ "$id" = arch ] && { [ -n "${WSL_DISTRO_NAME:-}" ] || [[ "$kernel" == *microsoft* ]] || [[ "$kernel" == *wsl* ]]; }; then
        printf 'arch-wsl'
    elif [ "$id" = ubuntu ]; then
        printf 'ubuntu-server'
    fi
    return 0
}

choose_platform() {
    has_terminal || fail "could not detect the platform; pass one of: cachyos, arch-wsl, ubuntu-server"
    while true; do
        local choice
        choice="$(ask $'Choose the platform:\n  1) CachyOS\n  2) Arch WSL\n  3) Ubuntu Server\nChoice (1-3): ')"
        case "$choice" in
            1)
                printf 'cachyos'
                return
                ;;
            2)
                printf 'arch-wsl'
                return
                ;;
            3)
                printf 'ubuntu-server'
                return
                ;;
            *) printf 'Enter 1, 2 or 3.\n' >/dev/tty ;;
        esac
    done
}

main() {
    [ "$(uname -s)" = Linux ] || fail "this script installs the Linux binaries; on Windows use install.ps1"
    [ "$(uname -m)" = x86_64 ] || fail "only x86_64 binaries are published"
    command -v curl >/dev/null || fail "curl is required"
    command -v sha256sum >/dev/null || fail "sha256sum is required"

    local platform=""
    if [ "$#" -gt 0 ] && platform="$(normalize_platform "$1")"; then
        shift
    else
        platform="$(detect_platform)"
        if [ -z "$platform" ]; then
            platform="$(choose_platform)"
        elif has_terminal; then
            case "$(ask "Detected platform: ${platform}. Is this correct? (Y/n): ")" in
                [Nn]*) platform="$(choose_platform)" ;;
            esac
        fi
    fi
    log "Platform: ${platform}"
    if ! has_terminal && [ "$#" -eq 0 ]; then
        fail "no terminal for the screen; pass a command such as: install"
    fi

    local asset="myconfig-${platform}-x86_64-unknown-linux-musl"
    local download
    download="$(mktemp -d)"
    trap 'rm -rf "$download"' EXIT
    log "Downloading ${asset}"
    curl -fsSL -o "${download}/${asset}" "${RELEASE_URL}/${asset}" \
        || fail "could not download ${RELEASE_URL}/${asset}"
    curl -fsSL -o "${download}/${asset}.sha256" "${RELEASE_URL}/${asset}.sha256" \
        || fail "could not download ${RELEASE_URL}/${asset}.sha256"
    (cd "$download" && sha256sum --check --status "${asset}.sha256") \
        || fail "${asset} does not match its published SHA-256"

    mkdir -p "$(dirname "$BINARY")"
    install -m 0755 "${download}/${asset}" "${BINARY}.new"
    mv -f "${BINARY}.new" "$BINARY"
    log "Installed ${BINARY}; run it again later as: myconfig verify, myconfig remove <module>"
    # exec replaces this shell, so the download directory goes before it.
    rm -rf "$download"
    trap - EXIT

    if has_terminal; then
        exec "$BINARY" "$@" </dev/tty
    fi
    exec "$BINARY" "$@"
}

main "$@"
