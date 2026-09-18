#!/usr/bin/env bash

module_android_phone() {
    [ "$MYCONFIG_PROFILE" = cachyos ] || {
        myconfig_fail "Android phone support is only available in the CachyOS profile"
        return 1
    }

    myconfig_log "Installing Android Wireless Debugging tools"
    install_package_ids android_sdk_platform_tools scrcpy
    require_command adb
    require_command scrcpy
    require_command timeout
    [ -x "$HOME/.local/bin/phone" ] || myconfig_fail "phone was not stowed as an executable"
}
