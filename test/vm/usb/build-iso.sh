#!/bin/bash
# Builds the InayOS USB stick image inside the Arch build guest. It runs as root.
#
#   build-iso.sh <overlay> <installer> <system-binary> <guard-binary> <osk-url> <iso>
#
# The profile is archiso's releng profile with the overlay copied over it: every overlay file
# replaces or adds the file at the same path, except packages.x86_64, whose lines are appended
# to releng's list. The finished image is written to <iso>.
set -euo pipefail

overlay=$1
installer=$2
system_binary=$3
guard_binary=$4
osk_url=$5
iso=$6

# mkarchiso needs several GB of work space, more than the /tmp memory file system holds.
build=/var/tmp/inayos-usb
profile=$build/profile
releng=/usr/share/archiso/configs/releng

echo "==> Upgrading the build guest and installing archiso"
pacman -Syu --noconfirm --needed archiso

rm -rf "$build"
mkdir -p "$build"
echo "==> Assembling the profile from $releng and the overlay"
cp -r --preserve=mode,timestamps,links "$releng" "$profile"
cp "$profile/packages.x86_64" "$build/releng-packages.x86_64"
# Files from the guest user's copy of the overlay belong to root in the profile.
cp -rT --preserve=mode,timestamps,links "$overlay" "$profile"
if [[ -f $overlay/packages.x86_64 ]]; then
    cat "$build/releng-packages.x86_64" "$overlay/packages.x86_64" >"$profile/packages.x86_64"
fi

echo "==> Placing the installer binaries"
install -D -m 755 "$installer" "$profile/airootfs/usr/local/bin/inayos-installer"
install -D -m 755 "$system_binary" "$profile/airootfs/usr/local/lib/inayos/inayos"
install -D -m 755 "$guard_binary" "$profile/airootfs/usr/local/lib/inayos/inayos-guard"

echo "==> Adding Axidev OSK from $osk_url"
curl --fail --location --show-error --silent --proto '=https' --output "$build/axidev-osk.tar.gz" "$osk_url"
mkdir -p "$profile/airootfs/opt"
tar -xzf "$build/axidev-osk.tar.gz" -C "$profile/airootfs/opt" --no-same-owner
if [[ ! -x $profile/airootfs/opt/axidev-osk/bin/axidev-osk ]]; then
    echo "error: the Axidev OSK release has no axidev-osk/bin/axidev-osk" >&2
    exit 1
fi

echo "==> Running mkarchiso"
mkarchiso -v -w "$build/work" -o "$build/out" "$profile"
shopt -s nullglob
built=("$build"/out/*.iso)
if [[ ${#built[@]} -ne 1 ]]; then
    echo "error: mkarchiso wrote ${#built[@]} images to $build/out, not one" >&2
    exit 1
fi
mv "${built[0]}" "$iso"
chmod 644 "$iso"
echo "==> The image is $iso"
