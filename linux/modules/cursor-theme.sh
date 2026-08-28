#!/usr/bin/env bash

BLACKNPINK_CURSOR_THEME=blacknpink-crosshair

blacknpink_cursor_theme_asset() {
    printf '%s\n' "$MYCONFIG_REPO_ROOT/linux/assets/cursors/$BLACKNPINK_CURSOR_THEME"
}

module_cursor_theme_validate() {
    local source
    source="$(blacknpink_cursor_theme_asset)"

    [ -f "$source/index.theme" ] \
        || myconfig_fail "Black & Pink Crosshair theme metadata was not found"
    [ -f "$source/cursors/default" ] \
        || myconfig_fail "Black & Pink Crosshair default cursor was not found"
    [ -f "$source/cursors/crosshair" ] \
        || myconfig_fail "Black & Pink Crosshair precision cursor was not found"
}

module_cursor_theme() {
    module_cursor_theme_validate || return 1
    require_command rsync

    myconfig_log "Installing the Black & Pink Crosshair cursor theme"
    local destination="$HOME/.local/share/icons/$BLACKNPINK_CURSOR_THEME"
    mkdir -p "$destination"
    rsync -a --delete "$(blacknpink_cursor_theme_asset)/" "$destination/"
}
