#!/bin/bash
# Runs as root in the CachyOS live session, from the seed disc. It installs CachyOS
# unattended from settings.json and powers the machine off when the installation succeeded.
# The xtask reads the first serial port: every line of output goes there too, and the
# lines that start with "inayos-image:" report the progress.

# The xtask types the start command again until it sees "started", so a later copy stops here.
mkdir /run/inayos-image 2>/dev/null || exit 0

exec > >(tee /dev/ttyS0) 2>&1
echo 'inayos-image: started'

seed=$(dirname "$(readlink -f "$0")")
# The installer reads settings.json and runs the post-install script from writable paths,
# because the seed disc is read-only.
install -m 600 "$seed/settings.json" /root/settings.json
install -m 755 "$seed/post-install.sh" /root/inayos-post-install.sh
install -m 644 "$seed/authorized_keys" /root/inayos-authorized_keys

if ! command -v cachyos-installer >/dev/null; then
    pacman -Sy --noconfirm --needed cachyos-cli-installer-new || {
        echo 'inayos-image: failed: could not install cachyos-cli-installer-new'
        exit 1
    }
fi

if ! cachyos-installer --config /root/settings.json; then
    echo 'inayos-image: failed: cachyos-installer reported an error'
    exit 1
fi
# The installer unmounts the new system before it runs its own post_install hook, so this
# script mounts the root and home subvolumes itself for the SSH setup.
mount -o subvol=@ /dev/vda2 /mnt && mount -o subvol=@home /dev/vda2 /mnt/home || {
    echo 'inayos-image: failed: could not mount the installed system'
    exit 1
}
if ! /root/inayos-post-install.sh /mnt; then
    umount -R /mnt
    echo 'inayos-image: failed: the post-install script did not finish'
    exit 1
fi
umount -R /mnt
sync
echo 'inayos-image: done'
systemctl poweroff
