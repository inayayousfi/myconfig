#!/usr/bin/env bash

module_appimage() {
    myconfig_log "Installing the AppImage runtime"
    # Most AppImages mount themselves through libfuse2, which fuse3 does not provide.
    install_package_ids fuse2
}
