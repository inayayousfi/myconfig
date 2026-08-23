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

    if ! sudo sshd -t || ! sudo systemctl enable sshd.service || ! sudo systemctl restart sshd.service; then
        sudo rm -f \
            /etc/ssh/sshd_config.d/10-myconfig.conf \
            /etc/ssh/sshd_config.d/10-local-only.conf
        if $had_config; then
            sudo install -Dm644 "$backup_dir/10-myconfig.conf" /etc/ssh/sshd_config.d/10-myconfig.conf
        fi
        if $had_legacy; then
            sudo install -Dm644 "$backup_dir/10-local-only.conf" /etc/ssh/sshd_config.d/10-local-only.conf
        fi
        if ! sudo sshd -t || ! sudo systemctl restart sshd.service; then
            myconfig_fail "OpenSSH failed and its previous configuration could not be restored"
            return 1
        fi
        myconfig_fail "OpenSSH failed; its previous configuration was restored"
        return 1
    fi

    rm -f -- "$config_tmp"
    rm -rf -- "$backup_dir"
    config_tmp=""
    backup_dir=""
    trap - RETURN
}
