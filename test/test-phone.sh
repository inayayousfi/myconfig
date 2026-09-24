#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TEST_ROOT="$(mktemp -d)"
trap 'rm -rf "$TEST_ROOT"' EXIT
mkdir -p "$TEST_ROOT/bin"
export TEST_ROOT
# Keep the fixture tools isolated from the host login environment.
sed 's@source /etc/profile@:@' "$REPO_ROOT/dotfiles/phone/.local/bin/phone" >"$TEST_ROOT/phone"
cat >"$TEST_ROOT/bin/adb" <<'EOF'
#!/usr/bin/env bash
printf '%s\n' "$*" >>"$TEST_ROOT/calls"
case "$1" in
    version) echo 'fixture adb' ;;
    start-server) ;;
    server-status) echo 'mdns_enabled: true' ;;
    mdns)
        echo 'List of discovered mdns services'
        if [[ -f "$TEST_ROOT/wifi-reconnected" ]]; then
            echo 'phone _adb-tls-connect._tcp 192.0.2.1:44444'
        elif [[ "$SCENARIO" == absent ]]; then
            :
        elif [[ "$SCENARIO" != refresh || -f "$TEST_ROOT/restarted" ]]; then
            echo 'phone _adb-tls-connect._tcp 192.0.2.1:33385'
        fi
        ;;
    devices)
        echo 'List of devices attached'
        if [[ -f "$TEST_ROOT/connected" ]]; then
            printf '%s device\n' "$(cat "$TEST_ROOT/connected")"
        elif [[ "$SCENARIO" == existing ]]; then
            echo '192.0.2.1:33385 device'
        elif [[ "$SCENARIO" == offline ]]; then
            echo 'other-device offline'
        fi
        ;;
    connect)
        if [[ "$SCENARIO" == refresh || "$SCENARIO" == direct || ( -f "$TEST_ROOT/wifi-reconnected" && "$SCENARIO" != wifi-unreachable ) ]]; then
            echo "$2" >"$TEST_ROOT/connected"
            echo "connected to $2"
        elif [[ "$SCENARIO" == auth ]]; then
            echo 'failed to authenticate'
        elif [[ "$SCENARIO" == wifi-timeout ]]; then
            exit 124
        else
            # Real adb can report connection failure with exit status zero.
            echo "failed to connect to '$2': No route to host"
        fi
        ;;
    kill-server) touch "$TEST_ROOT/restarted" ;;
    -s) echo device ;;
    *) exit 1 ;;
esac
EOF
cat >"$TEST_ROOT/bin/scrcpy" <<'EOF'
#!/usr/bin/env bash
printf '%s\n' "$*" >"$TEST_ROOT/launched"
EOF
cat >"$TEST_ROOT/bin/ip" <<'EOF'
#!/usr/bin/env bash
if [[ "$1" == route ]]; then
    echo '192.0.2.1 dev wlan0 src 192.0.2.2'
    exit 0
fi
echo '2: wlan0 inet 192.0.2.2/24 scope global wlan0'
EOF
cat >"$TEST_ROOT/bin/nmcli" <<'EOF'
#!/usr/bin/env bash
printf 'nmcli %s\n' "$*" >>"$TEST_ROOT/calls"
if [[ "$1" == --wait ]]; then
    [[ "$*" == '--wait 30 connection up uuid 12345678-1234-1234-1234-123456789abc ifname wlan0' ]] || exit 2
    if [[ "$SCENARIO" == wifi-fail ]]; then
        echo 'Connection activation failed'
        exit 1
    fi
    touch "$TEST_ROOT/wifi-reconnected"
elif [[ "$2" == GENERAL.CON-UUID ]]; then
    echo '12345678-1234-1234-1234-123456789abc'
else
    case "$SCENARIO" in
        wifi*|auth) echo wifi ;;
        *) echo ethernet ;;
    esac
    echo '12345678-1234-1234-1234-123456789abc'
fi
EOF
cat >"$TEST_ROOT/bin/sleep" <<'EOF'
#!/usr/bin/env bash
exit 0
EOF
chmod +x "$TEST_ROOT/bin/"*
export PATH="$TEST_ROOT/bin:$PATH"

for SCENARIO in direct refresh unreachable existing offline wifi wifi-timeout wifi-fail wifi-unreachable auth absent; do
    export SCENARIO
    rm -f "$TEST_ROOT/connected" "$TEST_ROOT/restarted" "$TEST_ROOT/launched" "$TEST_ROOT/wifi-reconnected"
    : >"$TEST_ROOT/calls"
    status=0
    bash "$TEST_ROOT/phone" --no-audio >"$TEST_ROOT/output" 2>&1 || status=$?
    case "$SCENARIO" in
        direct | refresh | existing)
            [[ "$status" == 0 ]]
            grep -qx -- '-s 192.0.2.1:33385 --no-audio' "$TEST_ROOT/launched"
            ;;
        wifi | wifi-timeout)
            [[ "$status" == 0 ]]
            grep -qx -- '-s 192.0.2.1:44444 --no-audio' "$TEST_ROOT/launched"
            ;;
        unreachable | offline | wifi-fail | wifi-unreachable)
            [[ "$status" == 1 && ! -f "$TEST_ROOT/launched" ]]
            grep -q 'No route to host' "$TEST_ROOT/output"
            if grep -q 'Pair first' "$TEST_ROOT/output"; then exit 1; fi
            ;;
        auth | absent) [[ "$status" == 1 && ! -f "$TEST_ROOT/launched" ]] ;;
    esac
    case "$SCENARIO" in
        refresh | unreachable | wifi* | auth | absent) [[ "$(grep -c '^kill-server$' "$TEST_ROOT/calls")" == 1 ]] ;;
        *) if grep -q '^kill-server$' "$TEST_ROOT/calls"; then exit 1; fi ;;
    esac
    case "$SCENARIO" in
        wifi*) [[ "$(grep -c '^nmcli --wait ' "$TEST_ROOT/calls")" == 1 ]] ;;
        *) if grep -q '^nmcli --wait ' "$TEST_ROOT/calls"; then exit 1; fi ;;
    esac
    if [[ "$SCENARIO" == wifi-fail ]]; then
        grep -q 'Could not reconnect Wi-Fi' "$TEST_ROOT/output"
    fi
    if [[ "$SCENARIO" == existing ]]; then
        if grep -q '^connect ' "$TEST_ROOT/calls"; then exit 1; fi
    fi
    printf 'PASS: %s\n' "$SCENARIO"
done
