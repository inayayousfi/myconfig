#!/usr/bin/env bash

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
STATE_ROOT="${MYCONFIG_VM_STATE_ROOT:-${XDG_DATA_HOME:-$HOME/.local/share}/myconfig/cachyos-vm}"
CACHE_ROOT="${MYCONFIG_VM_CACHE_ROOT:-${XDG_CACHE_HOME:-$HOME/.cache}/myconfig/cachyos-vm}"
OVMF_CODE="${MYCONFIG_OVMF_CODE:-/usr/share/edk2/x64/OVMF_CODE.4m.fd}"
OVMF_VARS="${MYCONFIG_OVMF_VARS:-/usr/share/edk2/x64/OVMF_VARS.4m.fd}"
KVM_PATH="${MYCONFIG_KVM_PATH:-/dev/kvm}"

BASE_DISK="$STATE_ROOT/base.qcow2"
BASE_VARS="$STATE_ROOT/base-vars.fd"
BASE_READY="$STATE_ROOT/base.ready"
TEST_DISK="$STATE_ROOT/test.qcow2"
TEST_VARS="$STATE_ROOT/test-vars.fd"
LOCK_PATH="$STATE_ROOT/operation.lock"
SSH_KEY="$STATE_ROOT/ssh/id_ed25519"
SSH_KNOWN_HOSTS="$STATE_ROOT/ssh/known_hosts"
SSH_GUEST_USER="$STATE_ROOT/ssh/guest-user"
SSH_PREPARED="$STATE_ROOT/ssh/prepared"
SSH_PREPARATION_VERSION=2
SSH_PORT=2222
GUEST_ROOT_MOUNT="/dev/sda2:/:subvol=@:btrfs"
GUEST_HOME_MOUNT="/dev/sda2:/home:subvol=@home:btrfs"

CACHYOS_DOWNLOAD_PAGE="https://cachyos.org/download/"
CACHYOS_ISO_PATTERN='https://cdn77\.cachyos\.org/ISO/desktop/[0-9]+/cachyos-desktop-linux-[0-9]+\.iso'

vm_log() {
    printf '[myconfig-vm] %s\n' "$*" >&2
}

vm_fail() {
    printf '[myconfig-vm] ERROR: %s\n' "$*" >&2
    return 1
}

usage() {
    cat <<'EOF'
Usage: vm/cachyos.sh <action>

Actions:
  install  Download and verify the latest Desktop ISO, then install CachyOS.
  seal     Boot the installed base once and mark it ready after shutdown.
  run      Boot the disposable test system with the live repository mounted.
  run-multi-display
           Boot the disposable test system with two displays and the live repository mounted.
  reset    Delete the disposable test system and preserve the installed base.
EOF
}

require_command() {
    command -v "$1" >/dev/null 2>&1 || vm_fail "required command not found: $1"
}

require_vm_host() {
    local command
    for command in qemu-system-x86_64 qemu-img curl sha256sum grep sort flock; do
        require_command "$command" || return 1
    done
    [ -r "$KVM_PATH" ] && [ -w "$KVM_PATH" ] || {
        vm_fail "KVM acceleration is unavailable at $KVM_PATH"
        return 1
    }
    [ -r "$OVMF_CODE" ] || {
        vm_fail "UEFI firmware code not found: $OVMF_CODE"
        return 1
    }
    [ -r "$OVMF_VARS" ] || {
        vm_fail "UEFI firmware variables not found: $OVMF_VARS"
        return 1
    }

    if ! command -v remote-viewer >/dev/null 2>&1; then
        require_command sudo || return 1
        require_command pacman || {
            vm_fail "install virt-viewer before starting the CachyOS VM"
            return 1
        }
        vm_log "Installing the SPICE display client"
        sudo pacman -S --needed --noconfirm virt-viewer
        require_command remote-viewer || return 1
    fi
}

acquire_lock() {
    mkdir -p "$STATE_ROOT"
    exec 8>"$LOCK_PATH"
    flock -n 8 || vm_fail "another CachyOS VM operation is running"
}

discover_iso_url() {
    local page
    page="$(curl --fail --location --show-error --silent "$CACHYOS_DOWNLOAD_PAGE")" || {
        vm_fail "could not read the CachyOS download page"
        return 1
    }

    local urls=()
    mapfile -t urls < <(printf '%s' "$page" | grep -oE "$CACHYOS_ISO_PATTERN" | sort -u)
    [ "${#urls[@]}" -eq 1 ] || {
        vm_fail "expected one CachyOS Desktop ISO URL, found ${#urls[@]}"
        return 1
    }
    printf '%s\n' "${urls[0]}"
}

ensure_iso() {
    mkdir -p "$CACHE_ROOT"

    local url filename iso checksum
    url="$(discover_iso_url)" || return 1
    filename="${url##*/}"
    iso="$CACHE_ROOT/$filename"
    checksum="$iso.sha256"

    vm_log "Downloading checksum for $filename"
    curl --fail --location --show-error --silent \
        --output "$checksum" "$url.sha256"

    if [ ! -f "$iso" ]; then
        vm_log "Downloading $filename"
        curl --fail --location --show-error --continue-at - \
            --output "$iso" "$url"
    fi

    local expected manifest_name remainder actual
    read -r expected manifest_name remainder <"$checksum" || {
        vm_fail "could not read the CachyOS ISO checksum"
        return 1
    }
    manifest_name="${manifest_name#\*}"
    if [[ ! "$expected" =~ ^[0-9a-fA-F]{64}$ ]] \
        || [ "$manifest_name" != "$filename" ] \
        || [ -n "$remainder" ]; then
        vm_fail "CachyOS checksum does not name the discovered ISO"
        return 1
    fi

    actual="$(sha256sum "$iso")"
    actual="${actual%% *}"
    if [ "${actual,,}" != "${expected,,}" ]; then
        rm -f "$iso"
        vm_fail "CachyOS ISO checksum verification failed"
        return 1
    fi

    printf '%s\n' "$iso"
}

run_qemu() {
    local disk="$1"
    local variables="$2"
    local iso="${3:-}"
    local share_repository="${4:-false}"
    local forward_ssh="${5:-false}"
    local display_count="${6:-1}"

    local command=(
        qemu-system-x86_64
        -name myconfig-cachyos
        -machine q35,accel=kvm
        -enable-kvm
        -cpu host
        -m 8192
        -smp 6
        -drive "if=pflash,format=raw,readonly=on,file=$OVMF_CODE"
        -drive "if=pflash,format=raw,file=$variables"
        -drive "if=virtio,format=qcow2,file=$disk,cache=writeback,discard=unmap"
        -device "qxl-vga,xres=1920,yres=1080,max_outputs=$display_count"
        -display spice-app
        -device virtio-serial-pci
        -chardev spicevmc,id=vdagent,name=vdagent
        -device virtserialport,chardev=vdagent,name=com.redhat.spice.0
        -device qemu-xhci
        -device usb-tablet
        -device virtio-rng-pci
    )

    if [ "$forward_ssh" = true ]; then
        command+=(
            -netdev "user,id=network,hostfwd=tcp:127.0.0.1:$SSH_PORT-:22"
            -device virtio-net-pci,netdev=network
        )
    else
        command+=(-nic user,model=virtio-net-pci)
    fi

    if [ "$share_repository" = true ]; then
        command+=(
            -virtfs "local,path=$REPO_ROOT,mount_tag=myconfig,security_model=mapped-xattr"
        )
    fi

    if [ -n "$iso" ]; then
        command+=(
            -drive "media=cdrom,format=raw,readonly=on,file=$iso"
            -boot once=d,menu=on
        )
    else
        command+=(-boot order=c,menu=on)
    fi

    "${command[@]}"
}

guestfs_kernel_available() {
    compgen -G '/boot/vmlinuz-*' >/dev/null \
        || compgen -G '/lib/modules/*/vmlinuz' >/dev/null
}

ensure_guestfs_tools() {
    if command -v virt-cat >/dev/null 2>&1 \
        && command -v guestfish >/dev/null 2>&1 \
        && guestfs_kernel_available; then
        return 0
    fi

    require_command sudo || return 1
    require_command pacman || {
        vm_fail "install linux, libguestfs, and guestfs-tools before sealing the VM"
        return 1
    }
    vm_log "Installing host tools and helper kernel for automatic VM SSH setup"
    sudo pacman -S --needed --noconfirm linux libguestfs guestfs-tools
    require_command virt-cat
    require_command guestfish
    guestfs_kernel_available || {
        vm_fail "libguestfs has no host kernel available for its helper VM"
        return 1
    }
}

generate_ssh_key() {
    require_command ssh-keygen || return 1
    mkdir -p "$(dirname "$SSH_KEY")"
    if [ ! -f "$SSH_KEY" ]; then
        vm_log "Generating the dedicated CachyOS VM SSH key"
        ssh-keygen -q -t ed25519 -N '' -f "$SSH_KEY"
    elif [ ! -f "$SSH_KEY.pub" ]; then
        ssh-keygen -y -f "$SSH_KEY" >"$SSH_KEY.pub"
    fi
    chmod 600 "$SSH_KEY"
}

detect_guest_user() {
    local passwd
    passwd="$(LIBGUESTFS_BACKEND=direct virt-cat \
        -a "$BASE_DISK" \
        -m "$GUEST_ROOT_MOUNT" \
        /etc/passwd)" || {
        vm_fail "could not read the default unencrypted CachyOS Btrfs root"
        return 1
    }

    local users=()
    local name password uid gid description home shell
    while IFS=: read -r name password uid gid description home shell; do
        if [[ "$uid" =~ ^[0-9]+$ ]] \
            && [ "$uid" -ge 1000 ] \
            && [ "$uid" -lt 65534 ] \
            && [[ "$home" == /home/* ]] \
            && [[ "$shell" != */nologin ]] \
            && [[ "$shell" != */false ]]; then
            users+=("$name")
        fi
    done <<<"$passwd"

    [ "${#users[@]}" -eq 1 ] || {
        vm_fail "expected one installed desktop user, found ${#users[@]}"
        return 1
    }
    printf '%s\n' "${users[0]}"
}

prepare_guest_ssh() {
    if [ -f "$SSH_PREPARED" ] \
        && [ "$(<"$SSH_PREPARED")" = "$SSH_PREPARATION_VERSION" ] \
        && [ -s "$SSH_KEY" ] \
        && [ -s "$SSH_GUEST_USER" ]; then
        return 0
    fi

    ensure_guestfs_tools || return 1
    generate_ssh_key || return 1

    local guest_user
    guest_user="$(detect_guest_user)" || return 1
    local guest_record uid gid home
    guest_record="$(LIBGUESTFS_BACKEND=direct virt-cat \
        -a "$BASE_DISK" \
        -m "$GUEST_ROOT_MOUNT" \
        /etc/passwd | grep -E "^${guest_user}:")"
    IFS=: read -r _ _ uid gid _ home _ <<<"$guest_record"

    if [ "$(LIBGUESTFS_BACKEND=direct guestfish --ro \
        -a "$BASE_DISK" \
        -m "$GUEST_ROOT_MOUNT" \
        exists /usr/bin/sshd)" != true ]; then
        vm_fail "the installed CachyOS system does not include OpenSSH"
        return 1
    fi

    local public_key ssh_dir="$home/.ssh" authorized_keys="$home/.ssh/authorized_keys"
    public_key="$(<"$SSH_KEY.pub")"
    vm_log "Preparing automatic SSH access for $guest_user"
    LIBGUESTFS_BACKEND=direct guestfish --rw \
        -a "$BASE_DISK" \
        -m "$GUEST_ROOT_MOUNT" \
        -m "$GUEST_HOME_MOUNT" \
        mkdir-p "$ssh_dir" : \
        write "$authorized_keys" "$public_key" : \
        chmod 0700 "$ssh_dir" : \
        chmod 0600 "$authorized_keys" : \
        chown "$uid" "$gid" "$ssh_dir" : \
        chown "$uid" "$gid" "$authorized_keys" : \
        mkdir-p /etc/systemd/system/multi-user.target.wants : \
        ln-sf /usr/lib/systemd/system/sshd.service \
        /etc/systemd/system/multi-user.target.wants/sshd.service

    if [ "$(LIBGUESTFS_BACKEND=direct guestfish --ro \
        -a "$BASE_DISK" \
        -m "$GUEST_ROOT_MOUNT" \
        exists /usr/bin/ufw)" = true ]; then
        local firewall_service=/etc/systemd/system/myconfig-vm-ssh-firewall.service
        local firewall_link=/etc/systemd/system/multi-user.target.wants/myconfig-vm-ssh-firewall.service
        local firewall_unit
        firewall_unit="[Unit]
Description=Allow SSH through the VM firewall
After=ufw.service
Before=sshd.service

[Service]
Type=oneshot
ExecStart=/usr/bin/ufw allow 22/tcp
ExecStartPost=/usr/bin/rm -f $firewall_link"
        LIBGUESTFS_BACKEND=direct guestfish --rw \
            -a "$BASE_DISK" \
            -m "$GUEST_ROOT_MOUNT" \
            write "$firewall_service" "$firewall_unit" : \
            chmod 0644 "$firewall_service" : \
            ln-sf "$firewall_service" "$firewall_link"
    fi

    printf '%s\n' "$guest_user" >"$SSH_GUEST_USER"
    printf '%s\n' "$SSH_PREPARATION_VERSION" >"$SSH_PREPARED"
}

prepare_guest_greeter_display() {
    local guest_user passwd guest_record greeter_record guest_home greeter_uid greeter_gid greeter_home
    guest_user="$(<"$SSH_GUEST_USER")"
    passwd="$(LIBGUESTFS_BACKEND=direct virt-cat \
        -a "$BASE_DISK" \
        -m "$GUEST_ROOT_MOUNT" \
        /etc/passwd)" || {
        vm_fail "could not read guest accounts while preparing the login display"
        return 1
    }
    guest_record="$(grep -E "^${guest_user}:" <<<"$passwd")"
    greeter_record="$(grep -E '^plasmalogin:' <<<"$passwd")"
    [ -n "$guest_record" ] && [ -n "$greeter_record" ] || {
        vm_fail "the installed CachyOS system has no Plasma Login greeter account"
        return 1
    }
    IFS=: read -r _ _ _ _ _ guest_home _ <<<"$guest_record"
    IFS=: read -r _ _ greeter_uid greeter_gid _ greeter_home _ <<<"$greeter_record"

    local source="$guest_home/.config/kwinoutputconfig.json"
    local destination="$greeter_home/.config/kwinoutputconfig.json"
    if [ "$(LIBGUESTFS_BACKEND=direct guestfish --ro \
        -a "$BASE_DISK" \
        -m "$GUEST_ROOT_MOUNT" \
        -m "$GUEST_HOME_MOUNT" \
        exists "$source")" != true ]; then
        vm_fail "Plasma did not save the desktop display layout during the seal boot"
        return 1
    fi

    vm_log "Copying the validated desktop display layout to Plasma Login"
    LIBGUESTFS_BACKEND=direct guestfish --rw \
        -a "$BASE_DISK" \
        -m "$GUEST_ROOT_MOUNT" \
        -m "$GUEST_HOME_MOUNT" \
        mkdir-p "$greeter_home/.config" : \
        cp "$source" "$destination" : \
        chmod 0600 "$destination" : \
        chown "$greeter_uid" "$greeter_gid" "$destination"
}

wait_for_ssh() {
    local qemu_pid="$1"
    local guest_user
    guest_user="$(<"$SSH_GUEST_USER")"

    local attempt
    for ((attempt = 0; attempt < 600; attempt++)); do
        if ssh \
            -i "$SSH_KEY" \
            -p "$SSH_PORT" \
            -o BatchMode=yes \
            -o ConnectTimeout=1 \
            -o IdentitiesOnly=yes \
            -o "UserKnownHostsFile=$SSH_KNOWN_HOSTS" \
            -o StrictHostKeyChecking=accept-new \
            "$guest_user@127.0.0.1" true >/dev/null 2>&1; then
            return 0
        fi
        if ! kill -0 "$qemu_pid" 2>/dev/null; then
            wait "$qemu_pid" 2>/dev/null || true
            vm_fail "QEMU stopped before SSH became available"
            return 1
        fi
        sleep 1
    done

    vm_fail "SSH did not become available within 10 minutes"
    return 1
}

boot_base_for_seal() {
    (
        local qemu_pid
        run_qemu "$BASE_DISK" "$BASE_VARS" "" false true &
        qemu_pid=$!
        trap 'kill "$qemu_pid" 2>/dev/null || true; wait "$qemu_pid" 2>/dev/null || true' EXIT

        wait_for_ssh "$qemu_pid" || return 1
        vm_log "SSH is ready. Log in, confirm the desktop display, then shut down CachyOS normally."
        wait "$qemu_pid"
        trap - EXIT
    )
}

boot_test_and_install() {
    local display_count="${1:-1}"
    (
        local qemu_pid
        run_qemu "$TEST_DISK" "$TEST_VARS" "" true true "$display_count" &
        qemu_pid=$!
        trap 'kill "$qemu_pid" 2>/dev/null || true; wait "$qemu_pid" 2>/dev/null || true' EXIT

        wait_for_ssh "$qemu_pid" || return 1
        local guest_user remote_status=0
        guest_user="$(<"$SSH_GUEST_USER")"
        vm_log "SSH is ready. Mounting the working tree and starting the CachyOS profile."
        ssh \
            -tt \
            -i "$SSH_KEY" \
            -p "$SSH_PORT" \
            -o IdentitiesOnly=yes \
            -o "UserKnownHostsFile=$SSH_KNOWN_HOSTS" \
            -o StrictHostKeyChecking=accept-new \
            "$guest_user@127.0.0.1" \
            'sudo mkdir -p /mnt/myconfig && sudo mount -t 9p -o trans=virtio,version=9p2000.L,rw myconfig /mnt/myconfig && bash /mnt/myconfig/cachyos/install.sh' \
            || remote_status=$?

        if [ "$remote_status" -eq 0 ]; then
            vm_log "Profile completed. Keep the VM open for graphical checks, then shut it down."
        else
            vm_log "Profile failed with status $remote_status. The VM remains open for inspection."
        fi
        wait "$qemu_pid"
        trap - EXIT
        return "$remote_status"
    )
}

confirm_seal() {
    if ! { exec 9<>/dev/tty; } 2>/dev/null; then
        vm_fail "sealing requires a terminal confirmation"
        return 1
    fi

    local answer
    printf 'Did the installed CachyOS desktop boot successfully? [y/N] ' >&9
    IFS= read -r answer <&9 || answer=""
    exec 9>&-
    case "$answer" in
        y | Y | yes | Yes | YES) return 0 ;;
        *) return 1 ;;
    esac
}

install_action() {
    require_vm_host
    acquire_lock
    [ ! -e "$BASE_READY" ] || {
        vm_fail "the CachyOS base is already sealed; use run or reset"
        return 1
    }

    local iso
    iso="$(ensure_iso)" || return 1

    if [ ! -f "$BASE_DISK" ]; then
        vm_log "Creating 100 GiB sparse base disk"
        qemu-img create -f qcow2 "$BASE_DISK" 100G
    fi
    if [ ! -f "$BASE_VARS" ]; then
        cp "$OVMF_VARS" "$BASE_VARS"
    fi

    vm_log "Starting the CachyOS installer"
    run_qemu "$BASE_DISK" "$BASE_VARS" "$iso" false false
    vm_log "Installer closed. Run: ./vm/cachyos.sh seal"
}

seal_action() {
    require_vm_host
    acquire_lock
    [ -f "$BASE_DISK" ] && [ -f "$BASE_VARS" ] || {
        vm_fail "install CachyOS before sealing the base"
        return 1
    }
    [ ! -e "$BASE_READY" ] || {
        vm_fail "the CachyOS base is already sealed"
        return 1
    }

    prepare_guest_ssh || return 1
    require_command ssh || return 1
    vm_log "Booting the installed base for automatic SSH verification"
    boot_base_for_seal || return 1
    confirm_seal || {
        vm_fail "the CachyOS base was not confirmed and remains unsealed"
        return 1
    }
    prepare_guest_greeter_display || return 1
    touch "$BASE_READY"
    vm_log "The CachyOS base is sealed"
}

run_action() {
    local display_count="${1:-1}"
    require_vm_host
    acquire_lock
    [ -f "$BASE_READY" ] && [ -f "$BASE_DISK" ] && [ -f "$BASE_VARS" ] || {
        vm_fail "install and seal CachyOS before starting a test"
        return 1
    }

    if [ ! -f "$TEST_DISK" ]; then
        vm_log "Creating disposable test overlay"
        qemu-img create -f qcow2 -F qcow2 -b "$BASE_DISK" "$TEST_DISK"
    fi
    if [ ! -f "$TEST_VARS" ]; then
        cp "$BASE_VARS" "$TEST_VARS"
    fi

    [ -f "$SSH_PREPARED" ] \
        && [ "$(<"$SSH_PREPARED")" = "$SSH_PREPARATION_VERSION" ] \
        && [ -s "$SSH_KEY" ] \
        && [ -s "$SSH_GUEST_USER" ] || {
        vm_fail "the sealed base is missing automatic SSH configuration"
        return 1
    }
    require_command ssh || return 1
    vm_log "WARNING: the guest has write access to $REPO_ROOT"
    boot_test_and_install "$display_count"
}

reset_action() {
    acquire_lock
    rm -f "$TEST_DISK" "$TEST_VARS"
    vm_log "Disposable CachyOS test state was reset"
}

main() {
    [ "$#" -eq 1 ] || {
        usage >&2
        return 1
    }

    case "$1" in
        install) install_action ;;
        seal) seal_action ;;
        run) run_action ;;
        run-multi-display) run_action 2 ;;
        reset) reset_action ;;
        -h | --help | help) usage ;;
        *)
            usage >&2
            vm_fail "unknown action: $1"
            return 1
            ;;
    esac
}

if [[ "${BASH_SOURCE[0]}" == "$0" ]]; then
    main "$@"
fi
