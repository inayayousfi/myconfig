#!/usr/bin/env bash

set -euo pipefail

LINUX_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
MYCONFIG_REPO_ROOT="$(dirname "$LINUX_DIR")"
MYCONFIG_PROFILE="${1:-}"

case "$MYCONFIG_PROFILE" in
    cachyos | arch-wsl) MYCONFIG_ADAPTER=arch ;;
    ubuntu-server) MYCONFIG_ADAPTER=apt ;;
    *)
        printf 'Unsupported internal profile: %s\n' "$MYCONFIG_PROFILE" >&2
        exit 1
        ;;
esac

export MYCONFIG_REPO_ROOT MYCONFIG_PROFILE MYCONFIG_ADAPTER

source "$LINUX_DIR/lib/common.sh"
source "$LINUX_DIR/lib/packages.sh"
source "$LINUX_DIR/lib/input-access.sh"
source "$LINUX_DIR/adapters/$MYCONFIG_ADAPTER.sh"

require_function adapter_supports_source
require_function adapter_prepare
require_function adapter_install_specs
require_function adapter_remove_specs

source "$LINUX_DIR/registry/packages.sh"

for module in base ssh cli runtimes zsh neovim terminal-tools ghostty axidev-osk kanata kanata-kde handy tailscale agents dotfiles cursor-theme refind kde-plasma authentication; do
    source "$LINUX_DIR/modules/$module.sh"
done

source "$LINUX_DIR/profiles/$MYCONFIG_PROFILE.sh"
require_function run_profile

start_sudo_keepalive
adapter_prepare
run_profile
myconfig_log "Profile completed successfully"
