#!/usr/bin/env bash

module_vscodium() {
    myconfig_log "Installing VSCodium"
    install_package_ids vscodium

    local config="$HOME/.config/VSCodium"
    [ -L "$config/User/settings.json" ] || myconfig_fail "VSCodium settings were not stowed"
    [ -L "$config/User/keybindings.json" ] || myconfig_fail "VSCodium key bindings were not stowed"
    [ -f "$config/extensions.txt" ] || myconfig_fail "VSCodium extension list was not stowed"
    [ -L "$config/blacknpink-theme" ] || myconfig_fail "VSCodium Black and Pink theme was not stowed"
    set_default_application vscodium.desktop \
        text/plain text/markdown application/json application/x-yaml text/x-cmake application/x-docbook+xml \
        || myconfig_fail "could not make VSCodium the default text editor"

    # VSCodium forgets extension folders it did not install itself, so the
    # theme goes in as a package: a zip with the extension under extension/.
    local theme_dir
    theme_dir="$(mktemp -d)"
    bsdtar --format zip -cf "$theme_dir/blacknpink-theme.vsix" -s ',^\.,extension,' \
        -C "$config/blacknpink-theme" . || myconfig_fail "could not package the VSCodium theme"

    # Forced, so an edited theme replaces the installed one of the same version.
    codium --force --install-extension "$theme_dir/blacknpink-theme.vsix" \
        || myconfig_fail "could not install the VSCodium theme"
    rm -rf "$theme_dir"

    local extension
    local arguments=(--install-extension jeanp413.open-remote-ssh)
    while IFS= read -r extension; do
        [ -z "$extension" ] || arguments+=(--install-extension "$extension")
    done <"$config/extensions.txt"
    codium "${arguments[@]}" || myconfig_fail "could not install the VSCodium extensions"
}
