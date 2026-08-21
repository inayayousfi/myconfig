#!/usr/bin/env bash

module_runtimes() {
    myconfig_log "Installing development runtimes"
    install_package_ids go bun python jdk maven llvm make cmake nodejs npm node_gyp

    rustup default stable
}
