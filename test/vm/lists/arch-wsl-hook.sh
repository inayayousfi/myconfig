#!/usr/bin/env bash
# The Arch WSL installer that the arch-wsl module of the Windows installer runs during a VM
# cycle, named by MYCONFIG_ARCH_WSL_BINARY. The module runs it on every install, after its own
# setup of the distribution. Before it starts the fresh Arch WSL installer, it saves the state
# lists of that distribution. The cycle reads the lists of the first install, when the
# distribution was just created, and compares them with the lists after the Arch WSL remove. The xtask fills in the @NAME@ values when it copies this file into the guest.

set -euo pipefail

if [ "${1:-}" = install ]; then
    saved='@SAVED@'
    if ! {
        bash '@LISTS@' user
        sudo -n bash '@LISTS@' system pacman '@HOME@'
    } >"$saved.part"; then
        echo "could not save the state lists of the distribution" >&2
        exit 1
    fi
    mv "$saved.part" "$saved"
fi

# The module removes the folder of this copy after the run. A copy runs whatever the mount
# options of C: are.
installer="$(dirname "$0")/myconfig-arch-wsl"
install -m 0755 '@INSTALLER@' "$installer"
exec "$installer" "$@"
