#!/usr/bin/env bash

TMUX_ATELIER_REPOSITORY="https://github.com/inayayousfi/tmux-atelier.git"
TMUX_ATELIER_DIR="$HOME/.tmux/plugins/tmux-atelier"

module_tmux() {
    myconfig_log "Installing tmux and tmux-atelier"
    install_package_ids tmux
    remove_package_ids herdr

    if [ "$MYCONFIG_PROFILE" = cachyos ]; then
        install_package_ids wl_clipboard
    fi

    if [ -d "$HOME/dotfiles/herdr" ]; then
        stow --dir "$HOME/dotfiles" --target "$HOME" --delete herdr
    fi

    mkdir -p "$(dirname "$TMUX_ATELIER_DIR")"
    local staging_dir
    staging_dir="$(mktemp -d "$HOME/.tmux/plugins/.tmux-atelier.XXXXXX")"

    if ! git clone --depth 1 --branch main "$TMUX_ATELIER_REPOSITORY" "$staging_dir/repo"; then
        rm -rf "$staging_dir"
        myconfig_fail "could not clone tmux-atelier"
        return 1
    fi

    if [ ! -f "$staging_dir/repo/tmux-atelier.tmux" ] \
        || [ ! -x "$staging_dir/repo/bin/tmux-atelier" ]; then
        rm -rf "$staging_dir"
        myconfig_fail "tmux-atelier checkout is incomplete"
        return 1
    fi

    rm -rf "$TMUX_ATELIER_DIR"
    mv "$staging_dir/repo" "$TMUX_ATELIER_DIR"
    rmdir "$staging_dir"
}
