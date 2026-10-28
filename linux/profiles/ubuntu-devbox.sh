#!/usr/bin/env bash

# A personal Ubuntu machine for coding and long-running agents. Editors reach it
# over the SSH access the machine already has, so the profile leaves SSH alone.
run_profile() {
    module_base
    module_cli
    module_system_monitor
    module_runtimes
    module_zsh
    module_terminal_tools
    module_tailscale
    module_agents_packages
    module_dotfiles
    module_docker
    module_agents_configure
    module_t3code
    module_authentication
    write_environment_inventory
}
