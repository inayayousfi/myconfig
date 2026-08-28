#!/usr/bin/env bash

refind_theme_asset() {
    printf '%s\n' "$MYCONFIG_REPO_ROOT/linux/assets/refind/blacknpink"
}

refind_config_path() {
    local candidate
    for candidate in \
        /boot/EFI/refind/refind.conf \
        /boot/efi/EFI/refind/refind.conf \
        /efi/EFI/refind/refind.conf; do
        if sudo test -f "$candidate"; then
            printf '%s\n' "$candidate"
            return 0
        fi
    done

    return 1
}

module_refind_validate() {
    local source
    source="$(refind_theme_asset)"

    [ -f "$source/refind.conf" ] || myconfig_fail "rEFInd theme configuration was not found"
    [ -f "$source/background.png" ] || myconfig_fail "rEFInd theme background was not found"
    [ -f "$source/selection-big.png" ] || myconfig_fail "rEFInd large selection image was not found"
    [ -f "$source/selection-small.png" ] || myconfig_fail "rEFInd small selection image was not found"
    [ -f "$source/font.png" ] || myconfig_fail "rEFInd theme font was not found"
}

module_refind() {
    module_refind_validate || return 1
    require_command awk
    require_command rsync

    local source config refind_dir destination config_tmp backup
    source="$(refind_theme_asset)"
    if ! config="$(refind_config_path)"; then
        myconfig_log "Skipping the rEFInd theme because no installation was found"
        return 0
    fi
    refind_dir="$(dirname "$config")"
    destination="$refind_dir/themes/blacknpink"
    backup="$config.pre-blacknpink"
    config_tmp="$(mktemp)"
    trap 'rm -f -- "$config_tmp"' RETURN

    myconfig_log "Installing the Black & Pink rEFInd theme"
    sudo install -d "$destination/icons"
    sudo rsync -rt --delete --exclude='*.svg' --exclude='refind.conf' "$source/" "$destination/"

    sudo awk '
        $0 == "# BEGIN MYCONFIG BLACKNPINK" { managed = 1; next }
        $0 == "# END MYCONFIG BLACKNPINK" { managed = 0; next }
        !managed { print }
    ' "$config" > "$config_tmp"
    printf '\n' >> "$config_tmp"
    cat "$source/refind.conf" >> "$config_tmp"

    if ! sudo cmp -s "$config_tmp" "$config"; then
        if ! sudo test -e "$backup"; then
            sudo cp -a "$config" "$backup"
        fi
        sudo cp "$config_tmp" "$config"
    fi
}
