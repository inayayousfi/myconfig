#!/usr/bin/env bash
# shellcheck disable=SC2034
# This file replaces releng's profiledef.sh. It reads releng's settings from the
# archiso installed where the image is built, so the image follows that archiso
# version, and changes only what InayOS needs.

releng_profiledef=/usr/share/archiso/configs/releng/profiledef.sh
if [[ ! -f "${releng_profiledef}" ]]; then
  echo "InayOS profile: ${releng_profiledef} is missing; install archiso." >&2
  exit 1
fi
# shellcheck source=/dev/null
. "${releng_profiledef}"

iso_name="inayos"
iso_label="INAYOS_$(date --date="@${SOURCE_DATE_EPOCH:-$(date +%s)}" +%Y%m)"
iso_publisher="InayOS"
iso_application="InayOS installer and repair system"

# mkarchiso copies airootfs without its modes, so every added file that needs a
# mode other than 644 is listed here, including the binaries the xtask places.
file_permissions+=(
  ["/etc/sudoers.d/10-inayos-live"]="0:0:440"
  ["/usr/local/bin/inayos-live-welcome"]="0:0:755"
  ["/usr/local/bin/inayos-start-installer"]="0:0:755"
  ["/usr/local/bin/inayos-installer"]="0:0:755"
  ["/usr/local/lib/inayos/inayos"]="0:0:755"
  ["/opt/axidev-osk/bin/axidev-osk"]="0:0:755"
)
