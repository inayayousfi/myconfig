#!/usr/bin/env bash
set -euo pipefail

script_dir=$(dirname "$(readlink -f "${BASH_SOURCE[0]}")")
source_file="$script_dir/windows.cs"
launcher="$script_dir/windows.ps1"
cache_dir=${XDG_CACHE_HOME:-"$HOME/.cache"}/computah
cache_file="$cache_dir/windows-helper"
read -r source_hash _ < <(sha256sum "$source_file")
helper=

if [[ -f $cache_file ]]; then
    readarray -t cached <"$cache_file"
    if [[ ${cached[0]:-} == "$source_hash" && -f ${cached[1]:-} ]]; then
        helper=${cached[1]}
    fi
fi

if [[ -z $helper ]]; then
    windows_launcher=$(wslpath -w "$launcher")
    windows_helper=$(powershell.exe -NoProfile -ExecutionPolicy Bypass -File "$windows_launcher" helper-path | tr -d '\r')
    helper=$(wslpath -u "$windows_helper")
    mkdir -p "$cache_dir"
    printf '%s\n%s\n' "$source_hash" "$helper" >"$cache_file"
fi

command=${1:-}
case $command in
    capture)
        output=${2:-${TMPDIR:-/tmp}/computah/shot.png}
        output=$(realpath -m "$output")
        mkdir -p "$(dirname "$output")"
        "$helper" capture "$(wslpath -w "$output")" >/dev/null
        printf '%s\n' "$output"
        ;;
    crop)
        [[ $# -eq 7 ]] || {
            printf 'computah: crop requires INPUT OUTPUT X Y WIDTH HEIGHT\n' >&2
            exit 2
        }
        input=$(realpath "$2")
        output=$(realpath -m "$3")
        mkdir -p "$(dirname "$output")"
        "$helper" crop "$(wslpath -w "$input")" "$(wslpath -w "$output")" "${@:4}" >/dev/null
        printf '%s\n' "$output"
        ;;
    *) "$helper" "$@" ;;
esac
