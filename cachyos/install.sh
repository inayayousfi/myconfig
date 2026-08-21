#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(dirname "$SCRIPT_DIR")"

if [ ! -f /etc/os-release ]; then
    printf 'Cannot verify CachyOS without /etc/os-release.\n' >&2
    exit 1
fi

source /etc/os-release
if [ "${ID:-}" != cachyos ]; then
    printf 'This installer requires CachyOS; detected %s.\n' "${ID:-unknown}" >&2
    exit 1
fi

exec "$REPO_ROOT/linux/install.sh" cachyos
