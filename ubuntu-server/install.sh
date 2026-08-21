#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(dirname "$SCRIPT_DIR")"

if [[ "$(uname -s)" != Linux ]] || ! command -v apt-get >/dev/null 2>&1; then
    printf 'This installer requires an apt-based Linux system.\n' >&2
    exit 1
fi

exec "$REPO_ROOT/linux/install.sh" ubuntu-server
