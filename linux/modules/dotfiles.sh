#!/usr/bin/env bash

dotfile_packages_for_profile() {
    case "$MYCONFIG_PROFILE" in
        cachyos) printf '%s\n' zsh yazi lazygit hunk ai herdr nvim opencode kanata kanata-kde kde-plasma ;;
        arch-wsl) printf '%s\n' zsh yazi lazygit hunk ai herdr nvim opencode ;;
        ubuntu-server) printf '%s\n' zsh ;;
        *) myconfig_fail "dotfile packages are undefined for $MYCONFIG_PROFILE" ;;
    esac
}

DOTFILE_CONFLICT_BACKUP_ROOT=""

backup_dotfile_path() {
    local path="$1"
    local relative="$2"

    if [ -z "$DOTFILE_CONFLICT_BACKUP_ROOT" ]; then
        DOTFILE_CONFLICT_BACKUP_ROOT="$(unique_backup_path "$HOME/.dotfiles-conflicts")"
        mkdir -p "$DOTFILE_CONFLICT_BACKUP_ROOT"
        myconfig_log "Backing up conflicting dotfiles to $DOTFILE_CONFLICT_BACKUP_ROOT"
    fi

    mkdir -p "$DOTFILE_CONFLICT_BACKUP_ROOT/$(dirname "$relative")"
    mv "$path" "$DOTFILE_CONFLICT_BACKUP_ROOT/$relative"
}

backup_dotfile_conflicts() {
    local dotfiles_dir="$1"
    shift

    local package file relative target expected actual ancestor ancestor_relative
    local -a parts

    DOTFILE_CONFLICT_BACKUP_ROOT=""

    for package in "$@"; do
        while IFS= read -r -d '' file; do
            relative="${file#"$dotfiles_dir/$package/"}"
            target="$HOME/$relative"

            IFS=/ read -r -a parts <<<"$relative"
            ancestor="$HOME"
            ancestor_relative=""

            local index
            for ((index = 0; index < ${#parts[@]} - 1; index++)); do
                ancestor="$ancestor/${parts[$index]}"
                ancestor_relative="${ancestor_relative:+$ancestor_relative/}${parts[$index]}"

                if [ -L "$ancestor" ]; then
                    expected="$(readlink -f "$dotfiles_dir/$package/$ancestor_relative" 2>/dev/null || true)"
                    actual="$(readlink -f "$ancestor" 2>/dev/null || true)"
                    [ -n "$expected" ] && [ "$actual" = "$expected" ] && continue

                    backup_dotfile_path "$ancestor" "$ancestor_relative"
                    mkdir -p "$ancestor"
                elif [ -e "$ancestor" ] && [ ! -d "$ancestor" ]; then
                    backup_dotfile_path "$ancestor" "$ancestor_relative"
                    mkdir -p "$ancestor"
                fi
            done

            [ -e "$target" ] || [ -L "$target" ] || continue

            if [ -e "$target" ] && [ "$target" -ef "$file" ]; then
                continue
            fi

            if [ -L "$target" ]; then
                expected="$(readlink -f "$file" 2>/dev/null || true)"
                actual="$(readlink -f "$target" 2>/dev/null || true)"
                [ -n "$expected" ] && [ "$actual" = "$expected" ] && continue
            fi

            backup_dotfile_path "$target" "$relative"
        done < <(find "$dotfiles_dir/$package" -type f -print0)
    done
}

validate_dotfile_packages() {
    local source_dir="$1"
    shift

    local package
    for package in "$@"; do
        [ -d "$source_dir/$package" ] || {
            myconfig_fail "dotfile package not found: $package"
            return 1
        }
    done
}

validate_staged_dotfiles() {
    local source_dir="$1"
    local staging_dir="$2"
    shift 2

    local package differences file
    for package in "$@"; do
        if ! differences="$(
            rsync -a --checksum --delete --dry-run --itemize-changes \
                "$source_dir/$package/" "$staging_dir/$package/"
        )"; then
            myconfig_fail "could not verify staged dotfile package: $package"
            return 1
        fi
        [ -z "$differences" ] || {
            myconfig_fail "staged dotfile package differs from source: $package"
            return 1
        }

        while IFS= read -r -d '' file; do
            if LC_ALL=C grep -Iq $'\r' "$file"; then
                myconfig_fail "dotfile contains CRLF or CR line endings: ${file#"$staging_dir/"}"
                return 1
            fi
        done < <(find "$staging_dir/$package" -type f -print0)
    done
}

module_dotfiles() {
    myconfig_log "Copying and stowing dotfiles"
    require_command rsync
    require_command stow

    local source_dir="${MYCONFIG_DOTFILES_SOURCE:-$MYCONFIG_REPO_ROOT/dotfiles}"
    local dotfiles_dir="$HOME/dotfiles"
    local staging_dir
    local packages=()
    mapfile -t packages < <(dotfile_packages_for_profile)

    [ -d "$source_dir" ] || myconfig_fail "dotfiles source not found: $source_dir"
    validate_dotfile_packages "$source_dir" "${packages[@]}"

    staging_dir="$(mktemp -d "$HOME/.dotfiles-stage.XXXXXX")"
    trap 'if [ -n "${staging_dir:-}" ]; then rm -rf "$staging_dir"; fi' RETURN

    local package
    for package in "${packages[@]}"; do
        mkdir -p "$staging_dir/$package"
        rsync -a --delete "$source_dir/$package/" "$staging_dir/$package/"
    done

    if ! validate_staged_dotfiles "$source_dir" "$staging_dir" "${packages[@]}"; then
        rm -rf "$staging_dir"
        staging_dir=""
        trap - RETURN
        return 1
    fi

    if [ -e "$dotfiles_dir" ] || [ -L "$dotfiles_dir" ]; then
        local dotfiles_backup
        dotfiles_backup="$(unique_backup_path "$dotfiles_dir")"
        myconfig_log "Backing up existing dotfiles to $dotfiles_backup"
        mv "$dotfiles_dir" "$dotfiles_backup"
    fi

    mv "$staging_dir" "$dotfiles_dir"
    staging_dir=""
    trap - RETURN

    backup_dotfile_conflicts "$dotfiles_dir" "${packages[@]}"

    for package in "${packages[@]}"; do
        stow --dir "$dotfiles_dir" --target "$HOME" --restow "$package"
    done
}
