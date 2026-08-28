#!/usr/bin/env bash

refind_asset_root() {
    printf '%s\n' "$MYCONFIG_REPO_ROOT/linux/assets/refind"
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
    local source theme
    source="$(refind_asset_root)"
    theme="$source/themes/black-pink"

    [ -f "$source/global.conf" ] || myconfig_fail "rEFInd global configuration was not found"
    [ -f "$theme/theme.conf" ] || myconfig_fail "rEFInd theme configuration was not found"
    [ -x "$theme/generate-theme-assets.sh" ] \
        || myconfig_fail "rEFInd theme asset generator was not found"
}

module_refind() {
    module_refind_validate || return 1
    require_command awk

    local source theme_source config refind_dir theme_dir config_tmp theme_tmp backup
    source="$(refind_asset_root)"
    theme_source="$source/themes/black-pink"
    if ! config="$(refind_config_path)"; then
        myconfig_log "Skipping the rEFInd theme because no installation was found"
        return 0
    fi
    refind_dir="$(dirname "$config")"
    theme_dir="$refind_dir/themes/black-pink"
    backup="$config.pre-blacknpink"
    config_tmp="$(mktemp)"
    theme_tmp="$(mktemp -d)"
    trap 'rm -f -- "$config_tmp"; rm -rf -- "$theme_tmp"' RETURN

    myconfig_log "Installing the restored Black & Pink rEFInd theme"
    "$theme_source/generate-theme-assets.sh" "$theme_tmp"
    sudo rm -rf -- "$refind_dir/themes/blacknpink"
    sudo install -d "$theme_dir"
    sudo install -m 0644 "$theme_source/theme.conf" "$theme_dir/theme.conf"
    sudo install -m 0644 "$theme_tmp/banner.png" "$theme_dir/banner.png"
    sudo install -m 0644 "$theme_tmp/selection_big.png" "$theme_dir/selection_big.png"
    sudo install -m 0644 "$theme_tmp/selection_small.png" "$theme_dir/selection_small.png"
    sudo install -m 0644 "$source/global.conf" "$refind_dir/managed.conf"

    sudo awk '
        $0 == "# BEGIN MYCONFIG BLACKNPINK" || $0 == "# BEGIN MYCONFIG REFIND" { managed = 1; next }
        $0 == "# END MYCONFIG BLACKNPINK" || $0 == "# END MYCONFIG REFIND" { managed = 0; next }
        !managed { print }
    ' "$config" > "$config_tmp"
    printf '\n' >> "$config_tmp"
    printf '%s\n' \
        '# BEGIN MYCONFIG REFIND' \
        'include managed.conf' \
        'include themes/black-pink/theme.conf' \
        '# END MYCONFIG REFIND' \
        >> "$config_tmp"

    if ! sudo cmp -s "$config_tmp" "$config"; then
        if ! sudo test -e "$backup"; then
            sudo cp -a "$config" "$backup"
        fi
        sudo cp "$config_tmp" "$config"
    fi
}
