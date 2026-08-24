#!/usr/bin/env bash

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TEST_ROOT="$(mktemp -d)"
trap 'rm -rf "$TEST_ROOT"' EXIT

MOCK_BIN="$TEST_ROOT/bin"
FIXTURES="$TEST_ROOT/fixtures"
COMMAND_LOG="$TEST_ROOT/commands.log"
mkdir -p "$MOCK_BIN" "$FIXTURES"
: >"$COMMAND_LOG"

ISO_NAME="cachyos-desktop-linux-260809.iso"
ISO_URL="https://cdn77.cachyos.org/ISO/desktop/260809/$ISO_NAME"
printf 'test iso contents\n' >"$FIXTURES/$ISO_NAME"
printf '%s  %s\n' \
    "$(sha256sum "$FIXTURES/$ISO_NAME" | cut -d' ' -f1)" "$ISO_NAME" \
    >"$FIXTURES/$ISO_NAME.sha256"
printf '<a href="%s">direct</a><span>%s</span>\n' "$ISO_URL" "$ISO_URL" \
    >"$FIXTURES/download.html"

cat >"$MOCK_BIN/curl" <<'EOF'
#!/usr/bin/env bash
output=""
url=""
while [ "$#" -gt 0 ]; do
    case "$1" in
        --output)
            output="$2"
            shift 2
            ;;
        --continue-at)
            shift 2
            ;;
        --*) shift ;;
        *) url="$1"; shift ;;
    esac
done

if [ -z "$output" ]; then
    cat "$CACHYOS_VM_TEST_PAGE"
elif [[ "$url" == *.sha256 ]]; then
    cp "$CACHYOS_VM_TEST_CHECKSUM" "$output"
else
    cp "$CACHYOS_VM_TEST_ISO" "$output"
fi
EOF

cat >"$MOCK_BIN/qemu-img" <<'EOF'
#!/usr/bin/env bash
printf 'qemu-img:%s\n' "$*" >>"$CACHYOS_VM_TEST_LOG"
target=""
for argument in "$@"; do
    case "$argument" in
        *.qcow2) target="$argument" ;;
    esac
done
[ -n "$target" ] || exit 1
: >"$target"
EOF

cat >"$MOCK_BIN/qemu-system-x86_64" <<'EOF'
#!/usr/bin/env bash
printf 'qemu:' >>"$CACHYOS_VM_TEST_LOG"
printf ' <%s>' "$@" >>"$CACHYOS_VM_TEST_LOG"
printf '\n' >>"$CACHYOS_VM_TEST_LOG"
EOF

cat >"$MOCK_BIN/virt-cat" <<'EOF'
#!/usr/bin/env bash
printf 'root:x:0:0:root:/root:/bin/bash\n'
printf 'plasmalogin:x:957:957:Plasma Login:/var/lib/plasmalogin:/usr/bin/nologin\n'
printf 'tester:x:1000:1000:Test User:/home/tester:/bin/bash\n'
EOF

cat >"$MOCK_BIN/guestfish" <<'EOF'
#!/usr/bin/env bash
printf 'guestfish:%s\n' "$*" >>"$CACHYOS_VM_TEST_LOG"
if [[ " $* " == *" exists /usr/bin/sshd "* ]] \
    || [[ " $* " == *" exists /usr/bin/ufw "* ]] \
    || [[ " $* " == *" exists /home/tester/.config/kwinoutputconfig.json "* ]]; then
    printf 'true\n'
fi
EOF

cat >"$MOCK_BIN/ssh-keygen" <<'EOF'
#!/usr/bin/env bash
key=""
derive=false
while [ "$#" -gt 0 ]; do
    [ "$1" = -y ] && derive=true
    if [ "$1" = -f ]; then
        key="$2"
        break
    fi
    shift
done
[ -n "$key" ] || exit 1
if [ "$derive" = true ]; then
    printf 'ssh-ed25519 test-key myconfig-vm\n'
    exit 0
fi
printf 'private key\n' >"$key"
printf 'ssh-ed25519 test-key myconfig-vm\n' >"$key.pub"
EOF

cat >"$MOCK_BIN/ssh" <<'EOF'
#!/usr/bin/env bash
printf 'ssh:%s\n' "$*" >>"$CACHYOS_VM_TEST_LOG"
EOF

cat >"$MOCK_BIN/remote-viewer" <<'EOF'
#!/usr/bin/env bash
exit 0
EOF

cat >"$MOCK_BIN/sudo" <<'EOF'
#!/usr/bin/env bash
printf 'sudo:%s\n' "$*" >>"$CACHYOS_VM_TEST_LOG"
"$@"
EOF

cat >"$MOCK_BIN/pacman" <<'EOF'
#!/usr/bin/env bash
printf 'pacman:%s\n' "$*" >>"$CACHYOS_VM_TEST_LOG"
touch "$CACHYOS_VM_TEST_KERNEL"
EOF

chmod +x \
    "$MOCK_BIN/curl" "$MOCK_BIN/qemu-img" "$MOCK_BIN/qemu-system-x86_64" \
    "$MOCK_BIN/virt-cat" "$MOCK_BIN/guestfish" "$MOCK_BIN/ssh-keygen" "$MOCK_BIN/ssh" \
    "$MOCK_BIN/remote-viewer" "$MOCK_BIN/sudo" "$MOCK_BIN/pacman"

export CACHYOS_VM_TEST_PAGE="$FIXTURES/download.html"
export CACHYOS_VM_TEST_CHECKSUM="$FIXTURES/$ISO_NAME.sha256"
export CACHYOS_VM_TEST_ISO="$FIXTURES/$ISO_NAME"
export CACHYOS_VM_TEST_LOG="$COMMAND_LOG"
export MYCONFIG_VM_STATE_ROOT="$TEST_ROOT/state"
export MYCONFIG_VM_CACHE_ROOT="$TEST_ROOT/cache"
export MYCONFIG_OVMF_CODE="$TEST_ROOT/OVMF_CODE.fd"
export MYCONFIG_OVMF_VARS="$TEST_ROOT/OVMF_VARS.fd"
export MYCONFIG_KVM_PATH="$TEST_ROOT/kvm"
export PATH="$MOCK_BIN:/usr/bin:/bin"

printf 'code\n' >"$MYCONFIG_OVMF_CODE"
printf 'vars\n' >"$MYCONFIG_OVMF_VARS"
printf 'kvm\n' >"$MYCONFIG_KVM_PATH"

source "$REPO_ROOT/vm/cachyos.sh"

export CACHYOS_VM_TEST_KERNEL="$TEST_ROOT/guestfs-kernel"
guestfs_kernel_available() {
    [ -f "$CACHYOS_VM_TEST_KERNEL" ]
}

[ "$(discover_iso_url)" = "$ISO_URL" ] \
    || vm_fail "latest CachyOS Desktop ISO URL was not discovered"

: >"$CACHYOS_VM_TEST_PAGE"
if discover_iso_url >/dev/null 2>&1; then
    vm_fail "missing CachyOS Desktop ISO URL was accepted"
fi

printf '%s\n%s\n' \
    "$ISO_URL" \
    "https://cdn77.cachyos.org/ISO/desktop/260810/cachyos-desktop-linux-260810.iso" \
    >"$CACHYOS_VM_TEST_PAGE"
if discover_iso_url >/dev/null 2>&1; then
    vm_fail "multiple CachyOS Desktop ISO URLs were accepted"
fi

printf '<a href="%s">direct</a>\n' "$ISO_URL" >"$CACHYOS_VM_TEST_PAGE"
main install

[ -f "$BASE_DISK" ] || vm_fail "install did not create the base disk"
[ -f "$BASE_VARS" ] || vm_fail "install did not create base UEFI variables"
[ ! -e "$BASE_READY" ] || vm_fail "install sealed the base without confirmation"
grep -Fq "file=$CACHE_ROOT/$ISO_NAME" "$COMMAND_LOG" \
    || vm_fail "install did not attach the verified CachyOS ISO"
grep -Fq '<spice-app>' "$COMMAND_LOG" \
    || vm_fail "QEMU did not use the SPICE display client"
grep -Fq '<spicevmc,id=vdagent,name=vdagent>' "$COMMAND_LOG" \
    || vm_fail "QEMU did not expose the SPICE guest-agent channel"
grep -Fq '<qxl-vga,xres=1920,yres=1080>' "$COMMAND_LOG" \
    || vm_fail "QEMU did not request a 1920x1080 guest framebuffer"
if grep -Fq 'mount_tag=myconfig' "$COMMAND_LOG"; then
    vm_fail "install exposed the host repository"
fi
if grep -Fq 'hostfwd=' "$COMMAND_LOG"; then
    vm_fail "install exposed the SSH forwarding port"
fi

main install
[ "$(grep -Fc "qemu-img:create -f qcow2 $BASE_DISK 100G" "$COMMAND_LOG")" -eq 1 ] \
    || vm_fail "resumed installation recreated the base disk"

confirm_seal() {
    return 1
}
if main seal >/dev/null 2>&1; then
    vm_fail "seal accepted a rejected boot confirmation"
fi
[ ! -e "$BASE_READY" ] || vm_fail "rejected confirmation marked the base ready"

confirm_seal() {
    return 0
}
main seal
[ -f "$BASE_READY" ] || vm_fail "seal did not mark the base ready"
if grep -Fq 'mount_tag=myconfig' "$COMMAND_LOG"; then
    vm_fail "seal exposed the host repository"
fi
[ -f "$SSH_KEY" ] || vm_fail "seal did not generate a dedicated SSH key"
[ "$(<"$SSH_GUEST_USER")" = tester ] || vm_fail "seal did not detect the installed desktop user"
grep -Fq 'guestfish:--rw' "$COMMAND_LOG" \
    || vm_fail "seal did not customize the stopped guest disk"
grep -Fq 'pacman:-S --needed --noconfirm linux libguestfs guestfs-tools' "$COMMAND_LOG" \
    || vm_fail "seal did not install the libguestfs helper kernel and tools"
grep -Fq 'write /home/tester/.ssh/authorized_keys ssh-ed25519 test-key myconfig-vm' "$COMMAND_LOG" \
    || vm_fail "seal did not inject the dedicated SSH key"
grep -Fq 'ln-sf /usr/lib/systemd/system/sshd.service /etc/systemd/system/multi-user.target.wants/sshd.service' "$COMMAND_LOG" \
    || vm_fail "seal did not enable guest SSH"
grep -Fq 'ExecStart=/usr/bin/ufw allow 22/tcp' "$COMMAND_LOG" \
    || vm_fail "seal did not configure the guest firewall for SSH"
grep -Fq 'cp /home/tester/.config/kwinoutputconfig.json /var/lib/plasmalogin/.config/kwinoutputconfig.json' "$COMMAND_LOG" \
    || vm_fail "seal did not copy the validated desktop layout to Plasma Login"
grep -Fq 'chown 957 957 /var/lib/plasmalogin/.config/kwinoutputconfig.json' "$COMMAND_LOG" \
    || vm_fail "seal did not give the Plasma Login account its display layout"
[ "$(<"$SSH_PREPARED")" = "$SSH_PREPARATION_VERSION" ] \
    || vm_fail "seal did not record the SSH preparation version"
grep -Fq 'hostfwd=tcp:127.0.0.1:2222-:22' "$COMMAND_LOG" \
    || vm_fail "seal did not forward the localhost SSH port"

main run
[ -f "$TEST_DISK" ] || vm_fail "run did not create the test overlay"
[ -f "$TEST_VARS" ] || vm_fail "run did not create test UEFI variables"
grep -Fq "qemu-img:create -f qcow2 -F qcow2 -b $BASE_DISK $TEST_DISK" "$COMMAND_LOG" \
    || vm_fail "run did not create a copy-on-write overlay"
grep -Fq 'mount_tag=myconfig,security_model=mapped-xattr' "$COMMAND_LOG" \
    || vm_fail "run did not expose the writable repository mount"
grep -Fq 'sudo mount -t 9p -o trans=virtio,version=9p2000.L,rw myconfig /mnt/myconfig' "$COMMAND_LOG" \
    || vm_fail "run did not mount the live repository over SSH"
grep -Fq 'bash /mnt/myconfig/cachyos/install.sh' "$COMMAND_LOG" \
    || vm_fail "run did not start the CachyOS profile over SSH"

main run
[ "$(grep -Fc "qemu-img:create -f qcow2 -F qcow2 -b $BASE_DISK $TEST_DISK" "$COMMAND_LOG")" -eq 1 ] \
    || vm_fail "repeated run recreated the test overlay"

cached_iso="$CACHE_ROOT/$ISO_NAME"
main reset
[ ! -e "$TEST_DISK" ] && [ ! -e "$TEST_VARS" ] \
    || vm_fail "reset preserved disposable test state"
[ -f "$BASE_DISK" ] && [ -f "$BASE_READY" ] && [ -f "$cached_iso" ] && [ -f "$SSH_KEY" ] \
    || vm_fail "reset removed the base, ISO cache, or SSH key"

rm -rf "$STATE_ROOT" "$CACHE_ROOT"
mkdir -p "$CACHE_ROOT"
printf '%s  wrong-name.iso\n' \
    "$(sha256sum "$CACHYOS_VM_TEST_ISO" | cut -d' ' -f1)" \
    >"$CACHYOS_VM_TEST_CHECKSUM"
: >"$COMMAND_LOG"
if main install >/dev/null 2>&1; then
    vm_fail "checksum for another ISO passed verification"
fi
if grep -Fq 'qemu:' "$COMMAND_LOG"; then
    vm_fail "checksum failure started QEMU"
fi

printf 'CachyOS VM tests passed.\n'
