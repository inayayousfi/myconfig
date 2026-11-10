#!/usr/bin/env bash

bootvine_package_dir() {
    printf '%s\n' "$MYCONFIG_REPO_ROOT/linux/assets/bootvine"
}

bootvine_mkinitcpio_config() {
    printf '%s\n' /etc/mkinitcpio.conf
}

bootvine_refind_linux_config() {
    printf '%s\n' /boot/refind_linux.conf
}

# Prints a mkinitcpio configuration whose HOOKS run sd-bootvine right after
# systemd.
bootvine_with_hook() {
    sed -E '/^HOOKS=/{/[( ]sd-bootvine[ )]/!s/([( ])systemd([ )])/\1systemd sd-bootvine\2/}'
}

# Prints a refind_linux.conf whose first boot entry passes quiet. bootvine
# silences the console only once it runs, so quiet hides the kernel messages
# before it. The other entries keep them for diagnosis.
bootvine_with_quiet_default() {
    awk -F'"' -v OFS='"' '
        !done && !/^[[:space:]]*(#|$)/ && NF >= 5 {
            done = 1
            if ((" " $4 " ") !~ /[[:space:]]quiet[[:space:]]/) {
                $4 = $4 == "" ? "quiet" : "quiet " $4
            }
        }
        { print }
    '
}

module_bootvine() {
    local package_dir mkinitcpio_config refind_linux_config filtered build_dir version
    local hook_changed=false
    local packages=()
    package_dir="$(bootvine_package_dir)"
    mkinitcpio_config="$(bootvine_mkinitcpio_config)"
    refind_linux_config="$(bootvine_refind_linux_config)"
    filtered="$(mktemp)"
    build_dir="$(mktemp -d)"
    trap 'rm -f -- "$filtered"; rm -rf -- "$build_dir"; trap - RETURN' RETURN

    myconfig_log "Showing the boot as a live bootvine tree"
    if ! grep -Eq '^HOOKS=.*[( ]systemd[ )]' "$mkinitcpio_config"; then
        myconfig_fail "bootvine needs the systemd hook in $mkinitcpio_config"
        return 1
    fi

    version="$(
        # shellcheck source=/dev/null
        source "$package_dir/PKGBUILD"
        # shellcheck disable=SC2154
        printf '%s-%s\n' "$pkgver" "$pkgrel"
    )"
    if [ "$(pacman -Q bootvine 2>/dev/null)" != "bootvine $version" ]; then
        cp "$package_dir/PKGBUILD" "$build_dir/"
        (cd "$build_dir" && makepkg --noconfirm)
        mapfile -t packages < <(cd "$build_dir" && makepkg --packagelist)
    fi

    # HOOKS names sd-bootvine only once its package is built, and before the
    # package installs, so the rebuild that pacman runs includes it.
    bootvine_with_hook <"$mkinitcpio_config" >"$filtered"
    if ! cmp -s "$filtered" "$mkinitcpio_config"; then
        sudo cp "$filtered" "$mkinitcpio_config"
        hook_changed=true
    fi

    if ((${#packages[@]})); then
        sudo pacman -U --noconfirm "${packages[@]}"
    elif [ "$hook_changed" = true ]; then
        sudo mkinitcpio -P
    fi
    sudo systemctl enable bootvine.service bootvine-shutdown.service

    if sudo test -f "$refind_linux_config"; then
        sudo cat "$refind_linux_config" | bootvine_with_quiet_default >"$filtered"
        if ! sudo cmp -s "$filtered" "$refind_linux_config"; then
            sudo cp "$filtered" "$refind_linux_config"
        fi
    else
        myconfig_log "Skipping the kernel options because $refind_linux_config was not found"
    fi
}
