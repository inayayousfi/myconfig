#!/usr/bin/env bash

declare -Ag MYCONFIG_APT_REPOSITORIES=()

register_apt_repository() {
    local id="$1"
    shift
    [ "$#" -eq 4 ] || myconfig_fail "apt repository $id needs a key URL, URI, suite and component"
    [ -z "${MYCONFIG_APT_REPOSITORIES[$id]+set}" ] || myconfig_fail "duplicate apt repository: $id"
    MYCONFIG_APT_REPOSITORIES["$id"]="$*"
}

source "$MYCONFIG_REPO_ROOT/linux/registry/apt-repositories.sh"

adapter_supports_source() {
    [ "$1" = official ] || [ -n "${MYCONFIG_APT_REPOSITORIES[$1]+set}" ]
}

adapter_prepare() {
    require_command sudo
    require_command apt-get
    sudo -v
    myconfig_log "Updating apt package databases"
    sudo apt-get update
}

# Writes the repository's key and source file, and sets APT_REPOSITORY_CHANGED
# when either changed, so apt rereads its package databases.
apt_repository_install() {
    local id="$1"
    local key_url uri suite component codename
    read -r key_url uri suite component <<<"${MYCONFIG_APT_REPOSITORIES[$id]}"
    codename="$(. /etc/os-release && printf '%s' "$VERSION_CODENAME")"
    [ -n "$codename" ] || myconfig_fail "this system does not report a release code name" || return 1
    key_url="${key_url//\{codename\}/$codename}"
    suite="${suite//\{codename\}/$codename}"

    local key_path="/etc/apt/keyrings/myconfig-$id.gpg"
    local source_path="/etc/apt/sources.list.d/myconfig-$id.sources"
    local work
    work="$(mktemp -d)" || return 1

    if ! curl -fsSL "$key_url" -o "$work/key"; then
        rm -rf -- "$work"
        myconfig_fail "could not download the $id repository key"
        return 1
    fi
    # apt reads an ASCII-armored key only from a file named .asc.
    if grep -q -- '-----BEGIN PGP PUBLIC KEY BLOCK-----' "$work/key"; then
        key_path="${key_path%.gpg}.asc"
    fi
    cat >"$work/source" <<EOF
Types: deb
URIs: $uri
Suites: $suite
Components: $component
Architectures: $(dpkg --print-architecture)
Signed-By: $key_path
EOF

    if ! cmp -s "$work/key" "$key_path" 2>/dev/null; then
        sudo install -Dm644 "$work/key" "$key_path" || return 1
        APT_REPOSITORY_CHANGED=true
    fi
    if ! cmp -s "$work/source" "$source_path" 2>/dev/null; then
        sudo install -Dm644 "$work/source" "$source_path" || return 1
        APT_REPOSITORY_CHANGED=true
    fi
    rm -rf -- "$work"
}

adapter_install_specs() {
    local packages=()
    local -A repositories=()
    local spec source package id

    for spec in "$@"; do
        source="${spec%%:*}"
        package="${spec#*:}"
        adapter_supports_source "$source" || myconfig_fail "unsupported apt package source: $source"
        [ "$source" = official ] || repositories["$source"]=1
        packages+=("$package")
    done

    APT_REPOSITORY_CHANGED=false
    for id in "${!repositories[@]}"; do
        apt_repository_install "$id"
    done
    if $APT_REPOSITORY_CHANGED; then
        sudo apt-get update
    fi

    if ((${#packages[@]})); then
        sudo apt-get install -y "${packages[@]}"
    fi
}

adapter_remove_specs() {
    local packages=()
    local spec source package status

    for spec in "$@"; do
        source="${spec%%:*}"
        package="${spec#*:}"
        adapter_supports_source "$source" || myconfig_fail "unsupported apt package source: $source"

        status="$(dpkg-query -W -f='${Status}' "$package" 2>/dev/null || true)"
        if [ "$status" = "install ok installed" ]; then
            packages+=("$package")
        fi
    done

    if ((${#packages[@]})); then
        sudo apt-get remove -y "${packages[@]}"
    fi
}
