#!/bin/bash
# Makes a pacman transaction that rebuilds the boot files broken, as root on the installed
# system. It writes the mkinitcpio drop-in <drop-in>, which leaves every kernel module out of
# the initramfs, then reinstalls the installed linux package, which runs the hooks of snap-pac,
# mkinitcpio and the guard.
#
#   break-boot.sh <drop-in>
set -euo pipefail

dropin=$1

mkdir -p "$(dirname "$dropin")"
# The base hook alone adds no kernel module, and an empty MODULES adds none either. mkinitcpio
# warns that no module was added and still builds the image, so the transaction succeeds with
# an initramfs whose usr/lib/modules/<version>/ holds no module.
printf '%s\n' 'MODULES=()' 'HOOKS=(base)' >"$dropin"

version=$(pacman -Q linux | cut -d' ' -f2)
package=/var/cache/pacman/pkg/linux-$version-x86_64.pkg.tar.zst
if [[ -f $package ]]; then
    pacman -U --noconfirm "$package"
else
    # Without the package in the cache, the sync database still names the installed version.
    pacman -S --noconfirm linux
fi
