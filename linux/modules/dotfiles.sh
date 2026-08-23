#!/usr/bin/env bash

dotfile_packages_for_profile() {
    case "$MYCONFIG_PROFILE" in
        cachyos | arch-wsl) printf '%s\n' zsh yazi lazygit ai herdr nvim opencode ;;
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
    trap 'rm -rf "$staging_dir"' RETURN

    local package
    for package in "${packages[@]}"; do
        mkdir -p "$staging_dir/$package"
        rsync -a --delete "$source_dir/$package/" "$staging_dir/$package/"
    done

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
