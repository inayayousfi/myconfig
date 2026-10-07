#!/usr/bin/env bash

module_ssh() {
    myconfig_log "Configuring the OpenSSH server"

    [ -d /run/systemd/system ] || myconfig_fail "systemd is not running"
    install_package_ids openssh

    local config_tmp backup_dir had_config=false had_legacy=false
    config_tmp="$(mktemp)"
    backup_dir="$(mktemp -d)"
    trap 'rm -f -- "$config_tmp"; rm -rf -- "$backup_dir"' RETURN

    cat >"$config_tmp" <<EOF
ListenAddress 0.0.0.0
ListenAddress ::
AllowUsers $USER
EOF

    sudo ssh-keygen -A
    sudo sshd -t -f "$config_tmp"

    if [ -e /etc/ssh/sshd_config.d/10-myconfig.conf ]; then
        cp /etc/ssh/sshd_config.d/10-myconfig.conf "$backup_dir/10-myconfig.conf"
        had_config=true
    fi
    if [ -e /etc/ssh/sshd_config.d/10-local-only.conf ]; then
        cp /etc/ssh/sshd_config.d/10-local-only.conf "$backup_dir/10-local-only.conf"
        had_legacy=true
    fi

    sudo install -Dm644 "$config_tmp" /etc/ssh/sshd_config.d/10-myconfig.conf
    sudo rm -f /etc/ssh/sshd_config.d/10-local-only.conf

    if ! sudo sshd -t; then
        sudo rm -f \
            /etc/ssh/sshd_config.d/10-myconfig.conf \
            /etc/ssh/sshd_config.d/10-local-only.conf
        if $had_config; then
            sudo install -Dm644 "$backup_dir/10-myconfig.conf" /etc/ssh/sshd_config.d/10-myconfig.conf
        fi
        if $had_legacy; then
            sudo install -Dm644 "$backup_dir/10-local-only.conf" /etc/ssh/sshd_config.d/10-local-only.conf
        fi
        myconfig_fail "OpenSSH rejected its new configuration; the previous one was restored"
        return 1
    fi

    rm -f -- "$config_tmp"
    rm -rf -- "$backup_dir"
    config_tmp=""
    backup_dir=""
    trap - RETURN
}

ssh_start_server() {
    # Another program on port 22, such as a second WSL distribution sharing the
    # same network, must not stop the rest of the install.
    if ! sudo systemctl enable --now sshd.service; then
        myconfig_log "WARNING: the OpenSSH server did not start. See: journalctl -u sshd.service"
        myconfig_log "Retry with: sudo systemctl enable --now sshd.service"
    fi
}

# A running server only needs the configuration module_ssh just wrote. A server
# that is off stays off unless the user asks for it.
module_ssh_server() {
    if systemctl is-active --quiet sshd.service; then
        myconfig_log "Restarting the OpenSSH server with its new configuration"
        if ! sudo systemctl restart sshd.service; then
            myconfig_log "WARNING: the OpenSSH server did not restart. See: journalctl -u sshd.service"
        fi
        return 0
    fi

    offer_action "The OpenSSH server is off." "Turn it on now?" \
        "sudo systemctl enable --now sshd.service" ssh_start_server
}
