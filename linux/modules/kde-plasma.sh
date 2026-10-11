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

configure_kde_plasma_window_rules() {
    local rule=myconfig-picture-in-picture
    local rules
    rules="$(kreadconfig6 --file kwinrulesrc --group General --key rules)"
    case ",$rules," in
        *",$rule,"*) ;;
        *) rules="${rules:+$rules,}$rule" ;;
    esac
    local rule_ids
    IFS=, read -ra rule_ids <<<"$rules"

    # KWin rule values: titlematch 3 is a regular expression, and a rule type
    # of 2 forces the property. Firefox titles its French player "Incrustation vidéo".
    kwriteconfig6 --file kwinrulesrc --group "$rule" --key Description "Hide picture-in-picture windows from screen capture"
    kwriteconfig6 --file kwinrulesrc --group "$rule" --key title '(?i)picture[ -]?in[ -]?picture|incrustation vidéo'
    kwriteconfig6 --file kwinrulesrc --group "$rule" --key titlematch 3
    kwriteconfig6 --file kwinrulesrc --group "$rule" --key excludefromcapture true
    kwriteconfig6 --file kwinrulesrc --group "$rule" --key excludefromcapturerule 2
    kwriteconfig6 --file kwinrulesrc --group General --key rules "$rules"
    kwriteconfig6 --file kwinrulesrc --group General --key count "${#rule_ids[@]}"
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
    local theme=glassy-blacknpink
    local size=40

    kwriteconfig6 --file kcminputrc --group Mouse --key cursorSize "$size"
    QT_QPA_PLATFORM=offscreen plasma-apply-cursortheme --size "$size" "$theme" || return 1

    # Plasma's theme tool updates the KDE and X11 settings, but leaves these
    # generated GTK settings at the old size in an existing session.
    local file
    for file in "$HOME/.gtkrc-2.0" "$HOME/.config/gtk-3.0/settings.ini" "$HOME/.config/gtk-4.0/settings.ini"; do
        if [ -f "$file" ]; then
            sed -i -E "s/^(gtk-cursor-theme-size=)[0-9]+$/\1$size/" "$file"
        fi
    done
    file="$HOME/.config/xsettingsd/xsettingsd.conf"
    if [ -f "$file" ]; then
        sed -i -E "s/^(Gtk\/CursorThemeSize )[0-9]+$/\1$size/" "$file"
    fi
    if command -v gsettings >/dev/null 2>&1 \
        && gsettings list-keys org.gnome.desktop.interface 2>/dev/null | grep -Fxq cursor-size; then
        gsettings set org.gnome.desktop.interface cursor-size "$size"
    fi
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

    myconfig_log "Configuring KDE Plasma with Glassy Black & Pink"
    install_package_ids iosevka_font libinput

    require_command plasma-apply-cursortheme
    require_command plasma-apply-lookandfeel
    require_command kwriteconfig6
    require_command kreadconfig6
    require_command qdbus6
    require_command fc-match
    require_command systemctl
    require_command sudo

    local theme="$MYCONFIG_REPO_ROOT/linux/assets/glassy-blacknpink-plasma"
    [ -f "$theme/install.sh" ] || {
        myconfig_fail "Glassy Black & Pink is missing; run: git submodule update --init"
        return 1
    }
    source "$theme/install.sh"
    glassy_blacknpink_install || {
        myconfig_fail "Glassy Black & Pink installation failed"
        return 1
    }

    local libinput_plugin libinput_plugins=("$MYCONFIG_REPO_ROOT"/linux/assets/libinput/*.lua)
    [ -f "${libinput_plugins[0]}" ] \
        || myconfig_fail "libinput plugins were not found"
    for libinput_plugin in "${libinput_plugins[@]}"; do
        sudo install -Dm644 \
            "$libinput_plugin" \
            "/etc/libinput/plugins/${libinput_plugin##*/}"
    done

    fc-match "Iosevka Nerd Font" | grep -Fq 'Iosevka' \
        || myconfig_fail "Iosevka Nerd Font is not available after installation"

    QT_QPA_PLATFORM=offscreen plasma-apply-lookandfeel --apply glassy-blacknpink.desktop
    configure_kde_plasma_fonts
    configure_kde_plasma_desktops
    configure_kde_plasma_window_rules
    configure_kde_plasma_input
    configure_kde_plasma_cursor_theme

    export XDG_RUNTIME_DIR="${XDG_RUNTIME_DIR:-/run/user/$(id -u)}"
    export DBUS_SESSION_BUS_ADDRESS="${DBUS_SESSION_BUS_ADDRESS:-unix:path=$XDG_RUNTIME_DIR/bus}"
    systemctl --user daemon-reload
    if qdbus6 org.kde.KWin /KWin >/dev/null 2>&1; then
        glassy_blacknpink_activate || {
            myconfig_fail "KWin could not load the Glassy Black & Pink glass"
            return 1
        }
        qdbus6 org.kde.KWin /KWin org.kde.KWin.reconfigure
    fi

    local layout_status=0
    glassy-blacknpink-layout || layout_status=$?
    case "$layout_status" in
        0)
            systemctl --user try-restart plasma-plasmashell.service || return 1
            myconfig_log "Applied and reloaded the Glassy Black & Pink layout and glass"
            ;;
        75) myconfig_log "KDE Plasma is not active; the layout will apply at the next KDE Plasma login" ;;
        *) myconfig_fail "Glassy Black & Pink layout failed with status $layout_status" ;;
    esac
}
