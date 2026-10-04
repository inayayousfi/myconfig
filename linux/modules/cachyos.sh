#!/usr/bin/env bash

module_cachyos() {
    [ "$MYCONFIG_PROFILE" = cachyos ] || return 0

    myconfig_log "Configuring CachyOS stable kernel tools"
    install_package_ids cachyos_kernel_manager linux_cachyos noto_fonts_cjk
    remove_package_ids konsole alacritty cachyos_hello cachyos_zsh_config vim \
        fish cachyos_fish_config fish_autopair fish_pure_prompt fisher \
        firefox firefox_i18n_fr meslo_font \
        cachyos_emerald_kde_theme cachyos_iridescent_kde cachyos_nord_kde_theme \
        kate micro cachyos_micro_settings nano nano_syntax_highlighting meld \
        glances duf tealdeer filelight pavucontrol kcalc shelly \
        cachyos_packageinstaller expac cachyos_wallpapers hwdetect qtscrcpy
}
