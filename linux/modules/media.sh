#!/usr/bin/env bash

module_media() {
    myconfig_log "Installing the VLC media player and the Loupe image viewer"
    install_package_ids vlc loupe

    local vlc_types loupe_types
    vlc_types="$(desktop_entry_mime_types /usr/share/applications/vlc.desktop)" || return 1
    loupe_types="$(desktop_entry_mime_types /usr/share/applications/org.gnome.Loupe.desktop)" || return 1
    # VLC also claims disc images and playlists; it only becomes the player.
    # shellcheck disable=SC2046
    set_default_application vlc.desktop $(grep -E '^(audio|video)/' <<<"$vlc_types") \
        || myconfig_fail "could not make VLC the default player"
    # shellcheck disable=SC2086
    set_default_application org.gnome.Loupe.desktop $loupe_types \
        || myconfig_fail "could not make Loupe the default image viewer"
}
