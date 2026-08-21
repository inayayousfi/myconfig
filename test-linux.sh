#!/usr/bin/env bash

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
TEST_HOME="$(mktemp -d)"
trap 'rm -rf "$TEST_HOME"' EXIT

export HOME="$TEST_HOME/home"
export MYCONFIG_PROFILE=ubuntu-server
export MYCONFIG_ADAPTER=test
export MYCONFIG_REPO_ROOT="$REPO_ROOT"
mkdir -p "$HOME"

source "$REPO_ROOT/linux/lib/common.sh"
source "$REPO_ROOT/linux/lib/packages.sh"

adapter_supports_source() {
    [ "$1" = official ]
}

adapter_install_specs() {
    printf '%s\n' "$@"
}

source "$REPO_ROOT/linux/registry/packages.sh"

[ "$(resolve_package fd)" = official:fd-find ] \
    || myconfig_fail "Ubuntu package override did not resolve"

MYCONFIG_PROFILE=cachyos
[ "$(resolve_package github_cli)" = official:github-cli ] \
    || myconfig_fail "GitHub CLI package did not resolve"
[ "$(resolve_package tailscale)" = official:tailscale ] \
    || myconfig_fail "Tailscale package did not resolve"

if resolve_package hunk >/dev/null 2>&1; then
    myconfig_fail "unsupported AUR source was accepted"
fi

if resolve_package missing_package >/dev/null 2>&1; then
    myconfig_fail "missing package mapping was accepted"
fi

source "$REPO_ROOT/linux/modules/dotfiles.sh"

dotfiles_dir="$HOME/dotfiles"
mkdir -p "$dotfiles_dir/zsh" "$HOME/.config"
printf 'managed\n' >"$dotfiles_dir/zsh/.zshrc"
printf 'existing\n' >"$HOME/.zshrc"

backup_dotfile_conflicts "$dotfiles_dir" zsh

[ ! -e "$HOME/.zshrc" ] || myconfig_fail "conflicting dotfile was not moved"

shopt -s nullglob
backups=("$HOME"/.dotfiles-conflicts.backup.*/.zshrc)
[ "${#backups[@]}" -eq 1 ] || myconfig_fail "conflicting dotfile backup was not created"
[ "$(cat "${backups[0]}")" = existing ] || myconfig_fail "dotfile backup content changed"

mkdir -p "$dotfiles_dir/nvim/.config/nvim" "$HOME/external-nvim"
printf 'managed\n' >"$dotfiles_dir/nvim/.config/nvim/init.lua"
printf 'external\n' >"$HOME/external-nvim/init.lua"
mkdir -p "$HOME/.config"
ln -s "$HOME/external-nvim" "$HOME/.config/nvim"

backup_dotfile_conflicts "$dotfiles_dir" nvim

[ "$(cat "$HOME/external-nvim/init.lua")" = external ] \
    || myconfig_fail "a symlinked external directory was modified"
[ ! -L "$HOME/.config/nvim" ] || myconfig_fail "conflicting parent symlink was not backed up"

mkdir -p "$HOME/dotfiles-existing" "$TEST_HOME/incomplete-source/zsh"
printf 'keep\n' >"$HOME/dotfiles-existing/keep"

if validate_dotfile_packages "$TEST_HOME/incomplete-source" zsh nvim >/dev/null 2>&1; then
    myconfig_fail "incomplete dotfile source passed validation"
fi
[ "$(cat "$HOME/dotfiles-existing/keep")" = keep ] \
    || myconfig_fail "validation changed the existing dotfiles tree"

plugin="$REPO_ROOT/dotfiles/zsh/.oh-my-zsh/custom/plugins/inaya/inaya.plugin.zsh"
connect_output="$(zsh -f -c '
    source "$1" >/dev/null
    t3u() { print -r -- repair; }
    t3() { print -r -- "t3:$*"; }
    t3c status
' _ "$plugin")"
[ "$connect_output" = $'repair\nt3:connect status' ] \
    || myconfig_fail "t3c did not repair before forwarding Connect arguments"

link_output="$(zsh -f -c '
    source "$1" >/dev/null
    t3u() { print -r -- repair; }
    t3() { print -r -- "t3:$*"; }
    t3c
' _ "$plugin")"
[ "$link_output" = $'repair\nt3:connect link' ] \
    || myconfig_fail "bare t3c did not repair before linking Connect"

tailscale_output="$(zsh -f -c '
    source "$1" >/dev/null
    t3u() { print -r -- repair; }
    t3() { print -r -- "t3:$*"; }
    t3t --ttl 1h
' _ "$plugin")"
[ "$tailscale_output" = $'repair\nt3:pair --tailscale --ttl 1h' ] \
    || myconfig_fail "t3t did not repair before Tailscale pairing"

source "$REPO_ROOT/linux/modules/authentication.sh"
auth_output="$(offer_authentication Test "test login" false)"
[[ "$auth_output" == *"Test is not authenticated. Run: test login"* ]] \
    || myconfig_fail "noninteractive authentication instructions were not printed"

printf 'Linux installer tests passed.\n'
