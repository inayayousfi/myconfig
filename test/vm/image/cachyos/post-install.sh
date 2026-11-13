#!/bin/bash
# The post_install script of cachyos-installer. It runs in the live session while the new
# system is still mounted at $1, and lets the xtask log in to that system over SSH.
set -euo pipefail

target=$1
user=@USER@
home=/home/$user

arch-chroot "$target" pacman -S --needed --noconfirm openssh
install -d -m 700 "$target$home/.ssh"
install -m 600 /root/myconfig-authorized_keys "$target$home/.ssh/authorized_keys"
arch-chroot "$target" chown -R "$user:$user" "$home/.ssh"
arch-chroot "$target" systemctl enable sshd.service

# ufw needs its running kernel rules to change, so a one-time service opens the SSH port at
# the first boot and then disables itself.
if [ -x "$target/usr/bin/ufw" ]; then
    unit=myconfig-vm-ssh-firewall.service
    cat >"$target/etc/systemd/system/$unit" <<UNIT
[Unit]
Description=Allow SSH through the VM firewall
After=ufw.service
Before=sshd.service

[Service]
Type=oneshot
ExecStart=/usr/bin/ufw allow 22/tcp
ExecStartPost=/usr/bin/systemctl disable $unit

[Install]
WantedBy=multi-user.target
UNIT
    arch-chroot "$target" systemctl enable "$unit"
fi

touch /root/myconfig-post-install-done
echo 'myconfig-image: SSH access prepared'
