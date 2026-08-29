#!/usr/bin/env bash

run_profile() {
    module_base
    module_ssh
    module_cli
    module_runtimes
    module_zsh
    module_neovim
    module_terminal_tools
    module_tmux
    module_tailscale
    module_agents_packages
    module_dotfiles
    module_agents_configure
    module_authentication
    write_environment_inventory
}
