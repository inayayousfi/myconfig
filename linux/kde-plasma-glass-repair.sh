#!/usr/bin/env bash

set -euo pipefail

MYCONFIG_REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
MYCONFIG_PROFILE=cachyos
export MYCONFIG_REPO_ROOT MYCONFIG_PROFILE

source "$MYCONFIG_REPO_ROOT/linux/lib/common.sh"
source "$MYCONFIG_REPO_ROOT/linux/modules/kde-plasma-glass.sh"

if install_kde_plasma_glass && activate_kde_plasma_glass; then
    qdbus6 org.kde.KWin /KWin org.kde.KWin.reconfigure
    myconfig_log "Glass matches the running KWin version"
else
    kwriteconfig6 --file kwinrc --group Plugins --key blurEnabled true || true
    qdbus6 org.kde.KWin /Effects org.kde.kwin.Effects.loadEffect blur >/dev/null 2>&1 || true
    myconfig_fail "Glass repair did not complete; KDE standard blur was requested"
    exit 1
fi
