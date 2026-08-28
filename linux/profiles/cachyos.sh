#!/usr/bin/env bash

run_profile() {
    module_kde_plasma_validate
    module_base
    module_ssh
    module_cli
    module_runtimes
    module_zsh
    module_neovim
    module_terminal_tools
    module_ghostty
    module_axidev_osk
    module_tailscale
    module_agents_packages
    module_dotfiles
    module_cursor_theme
    module_kanata
    module_kde_plasma
    module_kanata_kde
    module_handy
    module_agents_configure
    module_authentication
    write_environment_inventory
}
