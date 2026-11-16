#!/usr/bin/env bash
# Prints the state lists of a Linux guest in the form that test/vm/src/snapshot.rs reads.
#
#   linux.sh user                          the user's enabled services and groups
#   linux.sh system <pacman|dpkg> <home>   packages, system services and files; runs as root
#
# Package versions are left out: the installer upgrades the whole system, and removing a
# module does not downgrade what it upgraded.

set -euo pipefail

# Folders and files carry their mode and owner, because an undo that brings back the content
# of a file but not its mode or owner leaves the machine changed.
files() {
    local root=$1 modes
    [ -e "$root" ] || return 0
    # A path that disappears while the list is made is left out of the list.
    find "$root" -xdev -type d -printf '%p\tdirectory %m %u:%g\n' 2>/dev/null || true
    find "$root" -xdev -type l -printf '%p\tlink %l\n' 2>/dev/null || true
    modes=$(mktemp)
    find "$root" -xdev -type f -printf '%p\t%m %u:%g\n' 2>/dev/null >"$modes" || true
    { find "$root" -xdev -type f -print0 2>/dev/null || true; } \
        | { xargs -0 -r sha256sum 2>/dev/null || true; } \
        | sed -E 's/^\\?([0-9a-f]{64}) [ *](.*)$/\2\t\1/' \
        | awk -F '\t' 'FILENAME == ARGV[1] { mode[$1] = $2; next } { print $1 "\t" $2 (($1 in mode) ? " " mode[$1] : "") }' "$modes" -
    rm -f "$modes"
}

case ${1:-} in
user)
    echo '## user-services'
    systemctl --user list-unit-files --state=enabled --no-legend --plain | awk '{ print $1 }'
    echo '## groups'
    id -nG | tr ' ' '\n'
    ;;
system)
    tool=$2
    home=$3
    echo '## packages'
    case $tool in
    pacman) pacman -Qq ;;
    dpkg) dpkg-query -W -f='${db:Status-Abbrev} ${Package}\n' | awk '$1 == "ii" { print $2 }' ;;
    *)
        echo "unknown package tool: $tool" >&2
        exit 2
        ;;
    esac
    echo '## system-services'
    systemctl list-unit-files --state=enabled --no-legend --plain | awk '{ print $1 }'
    echo '## files'
    files /etc
    files /boot
    files /usr/local
    files /opt
    files /var/lib/flatpak
    files "$home"
    ;;
*)
    echo "usage: $0 user | system <pacman|dpkg> <home>" >&2
    exit 2
    ;;
esac
