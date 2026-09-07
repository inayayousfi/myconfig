#!/usr/bin/env bash

module_cachyos() {
    [ "$MYCONFIG_PROFILE" = cachyos ] || return 0

    myconfig_log "Configuring CachyOS stable kernel tools"
    install_package_ids cachyos_kernel_manager linux_cachyos
    remove_package_ids konsole alacritty cachyos_hello cachyos_zsh_config vim \
        fish cachyos_fish_config fish_autopair fish_pure_prompt fisher
}
