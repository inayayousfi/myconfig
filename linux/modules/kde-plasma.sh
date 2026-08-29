#!/usr/bin/env bash

validate_kde_plasma_version() {
    local output="$1"

    if [[ ! "$output" =~ ([0-9]+)\.([0-9]+)(\.([0-9]+))? ]]; then
        myconfig_fail "could not parse KDE Plasma version from: $output"
        return 1
    fi

    local major="${BASH_REMATCH[1]}"
    local minor="${BASH_REMATCH[2]}"
    if [ "$major" -ne 6 ] || [ "$minor" -lt 7 ]; then
        myconfig_fail "KDE Plasma 6.7 through 6.x is required; found ${BASH_REMATCH[0]}"
        return 1
    fi
}

configure_kde_plasma_fonts() {
    local font="Iosevka Nerd Font,12,-1,5,50,0,0,0,0,0"
    local small_font="Iosevka Nerd Font,10,-1,5,50,0,0,0,0,0"

    kwriteconfig6 --file kdeglobals --group General --key font "$font"
    kwriteconfig6 --file kdeglobals --group General --key fixed "$font"
    kwriteconfig6 --file kdeglobals --group General --key menuFont "$font"
    kwriteconfig6 --file kdeglobals --group General --key toolBarFont "$font"
    kwriteconfig6 --file kdeglobals --group General --key smallestReadableFont "$small_font"
    kwriteconfig6 --file kdeglobals --group WM --key activeFont "$font"
}

configure_kde_plasma_desktops() {
    kwriteconfig6 --file kwinrc --group Windows --key PerOutputVirtualDesktops true
    kwriteconfig6 --file kwinrc --group Windows --key ElectricBorderPushbackPixels 0
    kwriteconfig6 --file kwinrc --group EdgeBarrier --key CornerBarrier false
    kwriteconfig6 --file kwinrc --group EdgeBarrier --key EdgeBarrier 0
    kwriteconfig6 --file kwinrc --group Effect-overview --key BorderActivate 9
}

configure_kde_plasma_input() {
    local pointer_group=(--group Libinput --group Defaults --group Pointer)
    local touchpad_group=(--group Libinput --group Defaults --group Touchpad)

    kwriteconfig6 --file kcminputrc "${pointer_group[@]}" --key PointerAcceleration 1.000
    kwriteconfig6 --file kcminputrc "${pointer_group[@]}" --key PointerAccelerationProfile 1
    kwriteconfig6 --file kcminputrc "${touchpad_group[@]}" --key PointerAcceleration 1.000
    kwriteconfig6 --file kcminputrc "${touchpad_group[@]}" --key PointerAccelerationProfile 1
    kwriteconfig6 --file kcminputrc "${touchpad_group[@]}" --key NaturalScroll true
    kwriteconfig6 --file kcminputrc "${touchpad_group[@]}" --key TapDragLock true
    kwriteconfig6 --file kcminputrc "${touchpad_group[@]}" --key ClickMethod 2
}

configure_kde_plasma_cursor_theme() {
    local theme="${BLACKNPINK_CURSOR_THEME:-blacknpink-crosshair}"
    [ -f "$HOME/.local/share/icons/$theme/cursors/default" ] \
        || myconfig_fail "Black & Pink Crosshair cursor theme was not installed"

    kwriteconfig6 --file kcminputrc --group Mouse --key cursorSize 32
    QT_QPA_PLATFORM=offscreen plasma-apply-cursortheme --size 32 "$theme"
}

module_kde_plasma_validate() {
    [ "$MYCONFIG_PROFILE" = cachyos ] || {
        myconfig_fail "KDE Plasma configuration is only supported by the CachyOS profile"
        return 1
    }

    require_command plasmashell
    require_command pacman

    local plasma_version
    plasma_version="$(pacman -Q plasma-workspace)"
    validate_kde_plasma_version "$plasma_version"
}

module_kde_plasma() {
    module_kde_plasma_validate || return 1

    myconfig_log "Configuring KDE Plasma panels and Black & Pink appearance"
    install_package_ids iosevka_font desktop_file_utils libinput

    require_command plasma-apply-cursortheme
    require_command plasma-apply-lookandfeel
    require_command kwriteconfig6
    require_command qdbus6
    require_command fc-match
    require_command desktop-file-validate
    require_command kscreen-doctor
    require_command jq
    require_command flock
    require_command systemctl
    require_command sudo

    local pointer_plugin="$MYCONFIG_REPO_ROOT/linux/assets/libinput/90-myconfig-pointer-sensitivity.lua"
    [ -f "$pointer_plugin" ] \
        || myconfig_fail "libinput pointer-sensitivity plugin was not found"
    sudo install -Dm644 \
        "$pointer_plugin" \
        /etc/libinput/plugins/90-myconfig-pointer-sensitivity.lua

    [ -x "$HOME/.local/bin/myconfig-kde-plasma-layout" ] \
        || myconfig_fail "KDE Plasma layout command was not stowed"
    [ -f "$HOME/.local/share/color-schemes/BlackPink.colors" ] \
        || myconfig_fail "Black & Pink KDE color scheme was not stowed"
    [ -f "$HOME/.local/share/plasma/desktoptheme/blacknpink/metadata.json" ] \
        || myconfig_fail "Black & Pink Plasma theme metadata was not stowed"
    [ -f "$HOME/.local/share/plasma/desktoptheme/blacknpink/widgets/panel-background.svg" ] \
        || myconfig_fail "Black & Pink Plasma panel background was not stowed"
    [ -f "$HOME/.local/share/plasma/look-and-feel/org.myconfig.blacknpink.desktop/metadata.json" ] \
        || myconfig_fail "Black & Pink global theme metadata was not stowed"
    [ -f "$HOME/.local/share/plasma/look-and-feel/org.myconfig.blacknpink.desktop/contents/defaults" ] \
        || myconfig_fail "Black & Pink global theme defaults were not stowed"
    local widget
    for widget in overview session power; do
        [ -f "$HOME/.local/share/plasma/plasmoids/myconfig.$widget/metadata.json" ] \
            || myconfig_fail "MyConfig $widget Plasma metadata was not stowed"
        [ -f "$HOME/.local/share/plasma/plasmoids/myconfig.$widget/contents/ui/main.qml" ] \
            || myconfig_fail "MyConfig $widget Plasma widget was not stowed"
    done
    [ -f "$HOME/.local/share/kwin/scripts/myconfig-plasma-panels/metadata.json" ] \
        || myconfig_fail "MyConfig Plasma Panels KWin metadata was not stowed"
    [ -f "$HOME/.local/share/kwin/scripts/myconfig-plasma-panels/contents/code/main.js" ] \
        || myconfig_fail "MyConfig Plasma Panels KWin script was not stowed"
    [ -x "$HOME/.local/bin/myconfig-kde-overview" ] \
        || myconfig_fail "KWin Overview scaling command was not stowed"
    [ -f "$HOME/.config/systemd/user/myconfig-kde-plasma-layout.service" ] \
        || myconfig_fail "KDE Plasma layout user service was not stowed"
    desktop-file-validate "$HOME/.config/autostart/myconfig-kde-plasma-layout.desktop"
    fc-match "Iosevka Nerd Font" | grep -Fq 'Iosevka' \
        || myconfig_fail "Iosevka Nerd Font is not available after installation"

    QT_QPA_PLATFORM=offscreen plasma-apply-lookandfeel --apply org.myconfig.blacknpink.desktop
    configure_kde_plasma_fonts
    configure_kde_plasma_desktops
    configure_kde_plasma_input
    configure_kde_plasma_cursor_theme
    kwriteconfig6 --file kwinrc --group Plugins --key myconfig-plasma-panelsEnabled true

    export XDG_RUNTIME_DIR="${XDG_RUNTIME_DIR:-/run/user/$(id -u)}"
    export DBUS_SESSION_BUS_ADDRESS="${DBUS_SESSION_BUS_ADDRESS:-unix:path=$XDG_RUNTIME_DIR/bus}"
    systemctl --user daemon-reload
    if qdbus6 org.kde.KWin /KWin >/dev/null 2>&1; then
        qdbus6 org.kde.KWin /KWin org.kde.KWin.reconfigure
    fi

    local layout_status=0
    "$HOME/.local/bin/myconfig-kde-plasma-layout" || layout_status=$?
    case "$layout_status" in
        0) myconfig_log "Applied the KDE Plasma layout to the active session" ;;
        75) myconfig_log "KDE Plasma is not active; the layout will apply at the next KDE Plasma login" ;;
        *) myconfig_fail "KDE Plasma layout failed with status $layout_status" ;;
    esac
}
