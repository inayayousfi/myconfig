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
[ "${MYCONFIG_PACKAGE_DEFAULTS[github_cli]}" = aur:github-cli-git ] \
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

mkdir -p "$dotfiles_dir/yazi/.config/demo" "$HOME/.config"
printf 'managed\n' >"$dotfiles_dir/yazi/.config/demo/config"
ln -s "../dotfiles/yazi/.config/demo" "$HOME/.config/demo"

backup_dotfile_conflicts "$dotfiles_dir" yazi

[ -e "$dotfiles_dir/yazi/.config/demo/config" ] \
    || myconfig_fail "a managed source behind a directory symlink was moved"
[ "$(cat "$dotfiles_dir/yazi/.config/demo/config")" = managed ] \
    || myconfig_fail "a managed source behind a directory symlink was changed"

mkdir -p "$HOME/dotfiles-existing" "$TEST_HOME/incomplete-source/zsh"
printf 'keep\n' >"$HOME/dotfiles-existing/keep"

if validate_dotfile_packages "$TEST_HOME/incomplete-source" zsh nvim >/dev/null 2>&1; then
    myconfig_fail "incomplete dotfile source passed validation"
fi
[ "$(cat "$HOME/dotfiles-existing/keep")" = keep ] \
    || myconfig_fail "validation changed the existing dotfiles tree"

attributes_repo="$TEST_HOME/attributes-repo"
attributes_checkout="$TEST_HOME/attributes-checkout"
mkdir -p "$attributes_repo/dotfiles/zsh"
cp "$REPO_ROOT/.gitattributes" "$attributes_repo/.gitattributes"
printf 'line one\nline two\n' >"$attributes_repo/dotfiles/zsh/config.toml"
printf '\0binary\r\n' >"$attributes_repo/dotfiles/zsh/image.data"
git -C "$attributes_repo" init -q
git -C "$attributes_repo" config user.email test@example.com
git -C "$attributes_repo" config user.name Test
git -C "$attributes_repo" add .
git -C "$attributes_repo" commit -qm fixture
git -c core.autocrlf=true clone -q "$attributes_repo" "$attributes_checkout"

if LC_ALL=C grep -q $'\r' "$attributes_checkout/dotfiles/zsh/config.toml"; then
    myconfig_fail "core.autocrlf=true produced CRLF Linux dotfiles"
fi
cmp "$attributes_repo/dotfiles/zsh/image.data" "$attributes_checkout/dotfiles/zsh/image.data" \
    || myconfig_fail "the LF attribute changed a binary dotfile"

validation_home="$TEST_HOME/validation-home"
validation_source="$TEST_HOME/crlf-source"
mkdir -p "$validation_home/dotfiles" "$validation_source/zsh"
printf 'keep\n' >"$validation_home/dotfiles/keep"
printf 'bad\r\n' >"$validation_source/zsh/.zshrc"
HOME="$validation_home"
MYCONFIG_PROFILE=ubuntu-server
MYCONFIG_DOTFILES_SOURCE="$validation_source"

if module_dotfiles >/dev/null 2>&1; then
    myconfig_fail "CRLF dotfiles passed staged validation"
fi
[ "$(cat "$validation_home/dotfiles/keep")" = keep ] \
    || myconfig_fail "failed staged validation replaced the live dotfiles tree"

profile_home="$TEST_HOME/profile-home"
HOME="$profile_home"
MYCONFIG_PROFILE=arch-wsl
MYCONFIG_DOTFILES_SOURCE="$REPO_ROOT/dotfiles"
mkdir -p "$HOME"

module_dotfiles
module_dotfiles

mapfile -t profile_packages < <(dotfile_packages_for_profile)
for package in "${profile_packages[@]}"; do
    diff -r "$REPO_ROOT/dotfiles/$package" "$HOME/dotfiles/$package" \
        || myconfig_fail "profile rerun changed the staged $package package"
done

shopt -s nullglob
profile_conflict_backups=("$HOME"/.dotfiles-conflicts.backup.*)
[ "${#profile_conflict_backups[@]}" -eq 0 ] \
    || myconfig_fail "profile rerun treated managed dotfiles as conflicts"

zsh -n "$REPO_ROOT/dotfiles/zsh/.zshrc"
zsh -n "$REPO_ROOT/dotfiles/zsh/.oh-my-zsh/custom/plugins/inaya/inaya.plugin.zsh"
zsh -n "$REPO_ROOT/dotfiles/zsh/.oh-my-zsh/custom/themes/blacknpink.zsh-theme"

source "$REPO_ROOT/linux/modules/authentication.sh"
auth_output="$(offer_authentication Test "test login" false)"
[[ "$auth_output" == *"Test is not authenticated. Run: test login"* ]] \
    || myconfig_fail "noninteractive authentication instructions were not printed"

printf 'Linux installer tests passed.\n'
