#!/usr/bin/env bash

module_axidev_osk() {
    [ "$MYCONFIG_PROFILE" = cachyos ] || {
        myconfig_fail "Axidev OSK is only supported by the CachyOS profile"
        return 1
    }

    _configure_axidev_osk \
        /usr/local/sbin/axidev-osk-install \
        /usr/local/bin/axidev-osk
}

_configure_axidev_osk() {
    local lifecycle="$1"
    local app="$2"

    myconfig_log "Installing and configuring Axidev OSK"
    install_package_ids \
        python pyside6 qt6_wayland layer_shell_qt libinput systemd libxkbcommon
    require_command curl
    require_command sudo

    local greeter_tty="${MYCONFIG_TTY_PATH:-/dev/tty}"
    if ! { exec 9<>"$greeter_tty"; } 2>/dev/null; then
        myconfig_fail "Axidev OSK greeter setup requires a terminal; rerun this profile from a terminal"
        return 1
    fi

    local target_user
    target_user="$(id -un)"

    if [ -x "$lifecycle" ]; then
        sudo "$lifecycle" upgrade --user "$target_user"
    else
        if ! (
            local install_dir installer
            install_dir="$(mktemp -d)"
            trap 'rm -rf "$install_dir"' EXIT
            installer="$install_dir/axidev-osk-install"

            curl --fail --location --show-error --silent \
                --output "$installer" \
                https://github.com/axide-dev/axidev-osk/releases/latest/download/axidev-osk-install
            chmod +x "$installer"
            sudo "$installer" install --user "$target_user"
        ); then
            exec 9>&-
            myconfig_fail "Axidev OSK installation failed"
            return 1
        fi
    fi

    [ -x "$app" ] || {
        exec 9>&-
        myconfig_fail "Axidev OSK executable was not installed at $app"
        return 1
    }
    local greeter_status=0

    sudo "$app" linux setup-permissions --user "$target_user"
    "$app" linux setup-autostart --user "$target_user"
    sudo "$app" linux setup-greeter <&9 >&9 2>&9 || greeter_status=$?
    exec 9>&-
    [ "$greeter_status" -eq 0 ] || return "$greeter_status"

    sudo "$app" linux status-permissions --user "$target_user"
    "$app" linux status-autostart --user "$target_user"
    sudo "$app" linux status-greeter
}
