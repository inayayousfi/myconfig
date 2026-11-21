#!/bin/bash
# Builds a disk like the laptop's on the live USB system, as root: a GPT disk with a 2 GiB EFI
# partition mounted at /boot that holds the kernel and the microcode, and one ext4 partition
# with an Arch base system and the home folder of <user>, whose uid and gid are both <id>. The
# checksums of the test files in that home folder, relative to /home, go to <checksums>, and
# the filesystem UUID of the EFI partition goes to <efi-uuid>.
#
#   laptop-disk.sh <device> <user> <id> <checksums> <efi-uuid>
set -euo pipefail

device=$1
user=$2
id=$3
checksums=$4
efi_uuid=$5
root=/mnt

echo "==> Partitioning $device"
sgdisk --zap-all "$device"
sgdisk -n 1:0:+2G -t 1:EF00 -c 1:EFI -n 2:0:0 -t 2:8300 -c 2:arch "$device"
udevadm settle
efi=$(lsblk -nrpo NAME "$device" | sed -n 2p)
system=$(lsblk -nrpo NAME "$device" | sed -n 3p)
mkfs.fat -F 32 -n EFI "$efi"
blkid -s UUID -o value "$efi" >"$efi_uuid"
mkfs.ext4 -F -L arch "$system"
mount "$system" "$root"
mount --mkdir "$efi" "$root/boot"

echo "==> Installing an Arch base system"
# The live system fills pacman's keyring at boot; starting the unit waits until it has.
systemctl start pacman-init.service
pacman -Sy --noconfirm
pacstrap -K "$root" base linux intel-ucode
genfstab -U "$root" >>"$root/etc/fstab"
arch-chroot "$root" groupadd -g "$id" "$user"
arch-chroot "$root" useradd -m -u "$id" -g "$id" "$user"

echo "==> Writing the test files into /home/$user"
files=$root/home/$user/Documents/inayos-test
mkdir -p "$files/nested/deeper" "$root/home/$user/Pictures/inayos-test"
printf 'a note kept across the install\n' >"$files/note.txt"
printf 'a name with spaces\n' >"$files/with spaces.txt"
head -c 16M /dev/urandom >"$files/nested/random.bin"
head -c 1M /dev/urandom >"$files/nested/deeper/small.bin"
head -c 4M /dev/urandom >"$root/home/$user/Pictures/inayos-test/photo.raw"
arch-chroot "$root" chown -R "$user:$user" "/home/$user"
(
    cd "$root/home"
    find "$user/Documents/inayos-test" "$user/Pictures/inayos-test" -type f -print0 |
        sort -z | xargs -0 sha256sum
) >"$checksums"

echo "==> The old system's boot files on the EFI partition"
ls "$root/boot"
umount -R "$root"
