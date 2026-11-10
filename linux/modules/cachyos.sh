#!/usr/bin/env bash

cachyos_mkinitcpio_config() {
    printf '%s\n' /etc/mkinitcpio.conf
}

cachyos_refind_linux_config() {
    printf '%s\n' /boot/refind_linux.conf
}

# Prints a mkinitcpio configuration without the plymouth hook.
cachyos_without_plymouth_hook() {
    sed -E '/^HOOKS=/{s/([( ])plymouth([ )])/\1\2/g; s/ {2,}/ /g; s/\( /(/; s/ \)/)/}'
}

# Prints a refind_linux.conf whose boot entries no longer pass the kernel
# options that the CachyOS installer adds to a plain Arch boot: quiet,
# nowatchdog and splash.
cachyos_without_splash_options() {
    awk -F'"' -v OFS='"' '
        /^[[:space:]]*(#|$)/ || NF < 5 { print; next }
        {
            count = split($4, options, /[[:space:]]+/)
            kept = ""
            for (i = 1; i <= count; i++) {
                if (options[i] == "" || options[i] ~ /^(quiet|nowatchdog|splash)$/) {
                    continue
                }
                kept = kept == "" ? options[i] : kept " " options[i]
            }
            $4 = kept
            print
        }
    '
}

# Leaves the boot as a plain Arch installation shows it. The hook leaves
# mkinitcpio before its package goes, so no initramfs build can look for a
# missing hook.
cachyos_remove_boot_splash() {
    local mkinitcpio_config refind_linux_config filtered hooks_changed=false
    mkinitcpio_config="$(cachyos_mkinitcpio_config)"
    refind_linux_config="$(cachyos_refind_linux_config)"
    filtered="$(mktemp)"
    trap 'rm -f -- "$filtered"; trap - RETURN' RETURN

    myconfig_log "Removing the CachyOS boot splash"
    cachyos_without_plymouth_hook <"$mkinitcpio_config" >"$filtered"
    if ! cmp -s "$filtered" "$mkinitcpio_config"; then
        sudo cp "$filtered" "$mkinitcpio_config"
        hooks_changed=true
    fi

    remove_package_ids plymouth_kcm cachyos_plymouth_bootanimation cachyos_plymouth_theme plymouth

    if sudo test -f "$refind_linux_config"; then
        sudo cat "$refind_linux_config" | cachyos_without_splash_options >"$filtered"
        if ! sudo cmp -s "$filtered" "$refind_linux_config"; then
            sudo cp "$filtered" "$refind_linux_config"
        fi
    else
        myconfig_log "Skipping the kernel options because $refind_linux_config was not found"
    fi

    if [ "$hooks_changed" = true ]; then
        sudo mkinitcpio -P
    fi
}

module_cachyos() {
    [ "$MYCONFIG_PROFILE" = cachyos ] || return 0

    myconfig_log "Configuring CachyOS stable kernel tools"
    install_package_ids cachyos_kernel_manager linux_cachyos noto_fonts_cjk
    remove_package_ids konsole alacritty cachyos_hello cachyos_zsh_config vim \
        fish cachyos_fish_config fish_autopair fish_pure_prompt fisher \
        firefox firefox_i18n_fr meslo_font \
        cachyos_emerald_kde_theme cachyos_iridescent_kde cachyos_nord_kde_theme \
        kate micro cachyos_micro_settings nano nano_syntax_highlighting meld \
        glances duf btop tealdeer filelight pavucontrol kcalc shelly \
        cachyos_packageinstaller expac cachyos_wallpapers hwdetect qtscrcpy \
        fzf bat eza haruna mpvqt mpv kdeconnect partitionmanager ark gwenview
    cachyos_remove_boot_splash
}
