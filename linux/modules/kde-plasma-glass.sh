#!/usr/bin/env bash

install_kde_plasma_glass() {
    local data_dir="${1:-/usr/share/myconfig/kde-glass}"
    local expected kwin_version previous_effect="" built_for=""
    [[ ! -r "$data_dir/effect-id" ]] || previous_effect="$(<"$data_dir/effect-id")"
    [[ ! -r "$data_dir/kwin-version" ]] || built_for="$(<"$data_dir/kwin-version")"
    kwin_version="$(pacman -Q kwin)" || return 1
    expected="$(bash -c 'source "$1"; printf "%s %s-%s" "$pkgname" "$pkgver" "$pkgrel"' _ "$MYCONFIG_REPO_ROOT/linux/assets/kde-glass/PKGBUILD")" || return 1
    if [[ "$(pacman -Q myconfig-kde-glass 2>/dev/null)" != "$expected" || "$built_for" != "$kwin_version" ]]; then
        require_command makepkg || return 1
        local build_dir
        build_dir="$(mktemp -d)" || return 1
        cp "$MYCONFIG_REPO_ROOT/linux/assets/kde-glass/"* "$build_dir/" || return 1
        if ! (cd "$build_dir" && makepkg --syncdeps --noconfirm); then
            myconfig_fail "Glass build failed; build files retained in $build_dir"
            return 1
        fi
        "${MYCONFIG_GLASS_ELEVATE:-sudo}" pacman -U --noconfirm "$build_dir"/myconfig-kde-glass-*.pkg.tar.zst || return 1
        rm -rf "$build_dir"
    fi

    local key value effect_id
    [[ -r "$data_dir/effect-id" ]] || {
        myconfig_fail "Glass package has no effect identifier"
        return 1
    }
    effect_id="$(<"$data_dir/effect-id")"
    [[ "$effect_id" == myconfig_glass_* ]] || {
        myconfig_fail "Invalid Glass effect identifier: $effect_id"
        return 1
    }
    while IFS='=' read -r key value; do
        [[ -z "$key" || "$key" = \[* ]] && continue
        kwriteconfig6 --file kwinrc --group Effect-blurplus --key "$key" "$value" || return 1
    done <"$MYCONFIG_REPO_ROOT/dotfiles/kde-plasma/.local/share/myconfig/kde-plasma/glass.conf"
    for key in blur glass myconfig_glass "$previous_effect"; do
        [[ -z "$key" || "$key" == "$effect_id" ]] && continue
        kwriteconfig6 --file kwinrc --group Plugins --key "${key}Enabled" false || return 1
    done
    kwriteconfig6 --file kwinrc --group Plugins --key "${effect_id}Enabled" true
}

activate_kde_plasma_glass() {
    local data_dir="${1:-/usr/share/myconfig/kde-glass}"
    if ! qdbus6 org.kde.KWin /KWin >/dev/null 2>&1; then
        myconfig_log "Glass will load at the next KDE Plasma login"
        return 0
    fi
    local effect_id loaded effect ready=false
    [[ -r "$data_dir/effect-id" ]] || {
        myconfig_fail "Glass package has no effect identifier"
        return 1
    }
    effect_id="$(<"$data_dir/effect-id")"
    [[ "$effect_id" == myconfig_glass_* ]] || {
        myconfig_fail "Invalid Glass effect identifier: $effect_id"
        return 1
    }
    loaded="$(qdbus6 org.kde.KWin /Effects org.kde.kwin.Effects.loadedEffects)" || return 1
    # KWin's blur capability is a boolean, not a count of loaded providers.
    # Retiring any old provider after loading the replacement disables it for clients.
    while IFS= read -r effect; do
        case "$effect" in
            blur | glass | myconfig_glass | myconfig_glass_*)
                qdbus6 org.kde.KWin /Effects org.kde.kwin.Effects.unloadEffect "$effect" || return 1
                [[ "$effect" == "$effect_id" ]] && continue
                kwriteconfig6 --file kwinrc --group Plugins --key "${effect}Enabled" false || return 1
                ;;
        esac
    done <<<"$loaded"
    if [[ "$(qdbus6 org.kde.KWin /Effects org.kde.kwin.Effects.loadEffect "$effect_id")" == true ]]; then
        if qdbus6 org.kde.KWin /Effects org.kde.kwin.Effects.reconfigureEffect "$effect_id" \
            && [[ "$(qdbus6 org.kde.KWin /Effects org.kde.kwin.Effects.debug "$effect_id" '')" == 'valid=1 shaders=1 '* ]]; then
            ready=true
        fi
    fi
    if [[ "$ready" != true ]]; then
        qdbus6 org.kde.KWin /Effects org.kde.kwin.Effects.unloadEffect "$effect_id" || true
        kwriteconfig6 --file kwinrc --group Plugins --key "${effect_id}Enabled" false
        if [[ "$(qdbus6 org.kde.KWin /Effects org.kde.kwin.Effects.loadEffect blur)" == true ]]; then
            kwriteconfig6 --file kwinrc --group Plugins --key blurEnabled true
            myconfig_log "Restored KDE's standard blur after Glass failed to initialize"
        fi
        myconfig_fail "KWin could not initialize Glass: $effect_id"
        return 1
    fi
}
