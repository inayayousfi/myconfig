#!/usr/bin/env bash

set -euo pipefail

MYCONFIG_REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
MYCONFIG_PROFILE=cachyos
export MYCONFIG_REPO_ROOT MYCONFIG_PROFILE

source "$MYCONFIG_REPO_ROOT/linux/lib/common.sh"
source "$MYCONFIG_REPO_ROOT/linux/modules/kde-plasma-glass.sh"

if install_kde_plasma_glass && activate_kde_plasma_glass; then
    # Plasma may be stowed from a copied ~/dotfiles tree rather than linked
    # directly to this repository. Refresh the clients whose outer geometry
    # must agree with the shader before restarting Plasma.
    install -m644 \
        "$MYCONFIG_REPO_ROOT/dotfiles/kde-plasma/.local/share/plasma/plasmoids/myconfig.island/contents/ui/main.qml" \
        "$HOME/.local/share/plasma/plasmoids/myconfig.island/contents/ui/main.qml"
    install -m644 \
        "$MYCONFIG_REPO_ROOT/dotfiles/kde-plasma/.local/share/plasma/desktoptheme/blacknpink/widgets/panel-background.svg" \
        "$HOME/.local/share/plasma/desktoptheme/blacknpink/widgets/panel-background.svg"
    install -m644 \
        "$MYCONFIG_REPO_ROOT/dotfiles/kde-plasma/.local/share/plasma/desktoptheme/blacknpink/dialogs/background.svg" \
        "$HOME/.local/share/plasma/desktoptheme/blacknpink/dialogs/background.svg"
    qdbus6 org.kde.KWin /KWin org.kde.KWin.reconfigure
    # Reloading the KWin effect temporarily removes the advertised blur
    # capability. Existing Wayland clients then lose their blur regions and
    # do not automatically submit them again when the replacement loads.
    # Restart Plasma so custom windows such as MyConfig Island register their
    # regions against the newly active effect.
    systemctl --user try-restart plasma-plasmashell.service
    myconfig_log "Glass matches the running KWin version"
else
    kwriteconfig6 --file kwinrc --group Plugins --key blurEnabled true || true
    qdbus6 org.kde.KWin /Effects org.kde.kwin.Effects.loadEffect blur >/dev/null 2>&1 || true
    myconfig_fail "Glass repair did not complete; KDE standard blur was requested"
    exit 1
fi
