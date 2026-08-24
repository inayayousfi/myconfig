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

sudo_keepalive_log="$TEST_HOME/sudo-keepalive.log"
(
    sudo() {
        printf 'sudo:%s\n' "$*" >>"$sudo_keepalive_log"
    }
    sleep() {
        /usr/bin/sleep 0.01
    }
    start_sudo_keepalive
    /usr/bin/sleep 0.03
    stop_sudo_keepalive
)
grep -Fq 'sudo:-v' "$sudo_keepalive_log" \
    || myconfig_fail "installer did not validate sudo before starting"
grep -Fq 'sudo:-n true' "$sudo_keepalive_log" \
    || myconfig_fail "installer did not refresh its sudo credential"

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
[ "$(resolve_package pyside6)" = official:pyside6 ] \
    || myconfig_fail "Axidev OSK PySide6 dependency did not resolve"
[ "$(resolve_package layer_shell_qt)" = official:layer-shell-qt ] \
    || myconfig_fail "Axidev OSK LayerShellQt dependency did not resolve"

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

source "$REPO_ROOT/linux/modules/axidev-osk.sh"

osk_test_root="$TEST_HOME/axidev-osk"
osk_test_bin="$osk_test_root/bin"
osk_test_log="$osk_test_root/commands.log"
mkdir -p "$osk_test_bin"
: >"$osk_test_root/tty"

cat >"$osk_test_root/app" <<'EOF'
#!/usr/bin/env bash
printf 'app:%s\n' "$*" >>"$AXIDEV_OSK_TEST_LOG"
EOF

cat >"$osk_test_root/lifecycle" <<'EOF'
#!/usr/bin/env bash
printf 'lifecycle:%s\n' "$*" >>"$AXIDEV_OSK_TEST_LOG"
EOF

cat >"$osk_test_root/installer" <<'EOF'
#!/usr/bin/env bash
printf 'installer:%s\n' "$*" >>"$AXIDEV_OSK_TEST_LOG"
cp "$AXIDEV_OSK_TEST_ROOT/app" "$AXIDEV_OSK_TEST_BIN/axidev-osk"
cp "$AXIDEV_OSK_TEST_ROOT/lifecycle" "$AXIDEV_OSK_TEST_BIN/axidev-osk-install"
chmod +x "$AXIDEV_OSK_TEST_BIN/axidev-osk" "$AXIDEV_OSK_TEST_BIN/axidev-osk-install"
EOF

cat >"$osk_test_bin/curl" <<'EOF'
#!/usr/bin/env bash
printf 'curl:%s\n' "$*" >>"$AXIDEV_OSK_TEST_LOG"
while [ "$#" -gt 0 ]; do
    if [ "$1" = --output ]; then
        cp "$AXIDEV_OSK_TEST_ROOT/installer" "$2"
        exit 0
    fi
    shift
done
exit 1
EOF

cat >"$osk_test_bin/sudo" <<'EOF'
#!/usr/bin/env bash
printf 'sudo:%s\n' "$*" >>"$AXIDEV_OSK_TEST_LOG"
"$@"
EOF

chmod +x \
    "$osk_test_root/app" "$osk_test_root/lifecycle" "$osk_test_root/installer" \
    "$osk_test_bin/curl" "$osk_test_bin/sudo"

export AXIDEV_OSK_TEST_ROOT="$osk_test_root"
export AXIDEV_OSK_TEST_BIN="$osk_test_bin"
export AXIDEV_OSK_TEST_LOG="$osk_test_log"
export MYCONFIG_TTY_PATH="$osk_test_root/tty"
export PATH="$osk_test_bin:/usr/bin:/bin"
MYCONFIG_PROFILE=cachyos

install_package_ids() {
    printf 'packages:%s\n' "$*" >>"$AXIDEV_OSK_TEST_LOG"
}

_configure_axidev_osk \
    "$osk_test_bin/axidev-osk-install" \
    "$osk_test_bin/axidev-osk"

grep -Fq 'packages:python pyside6 qt6_wayland layer_shell_qt libinput systemd libxkbcommon' "$osk_test_log" \
    || myconfig_fail "Axidev OSK host dependencies were not requested"
grep -Fq 'curl:--fail --location --show-error --silent --output' "$osk_test_log" \
    || myconfig_fail "Axidev OSK first install did not download the lifecycle installer"
grep -Fq 'installer:install --user ' "$osk_test_log" \
    || myconfig_fail "Axidev OSK first install did not run the lifecycle installer"
grep -Fq 'app:linux setup-permissions --user ' "$osk_test_log" \
    || myconfig_fail "Axidev OSK permissions were not configured"
grep -Fq 'app:linux setup-autostart --user ' "$osk_test_log" \
    || myconfig_fail "Axidev OSK desktop autostart was not configured"
grep -Fq 'app:linux setup-greeter' "$osk_test_log" \
    || myconfig_fail "Axidev OSK greeter startup was not configured"
grep -Fq 'app:linux status-permissions --user ' "$osk_test_log" \
    || myconfig_fail "Axidev OSK permissions were not verified"
grep -Fq 'app:linux status-autostart --user ' "$osk_test_log" \
    || myconfig_fail "Axidev OSK desktop autostart was not verified"
grep -Fq 'app:linux status-greeter' "$osk_test_log" \
    || myconfig_fail "Axidev OSK greeter startup was not verified"

: >"$osk_test_log"
_configure_axidev_osk \
    "$osk_test_bin/axidev-osk-install" \
    "$osk_test_bin/axidev-osk"

grep -Fq 'lifecycle:upgrade --user ' "$osk_test_log" \
    || myconfig_fail "Axidev OSK rerun did not use the lifecycle upgrade"
if grep -Fq 'curl:' "$osk_test_log"; then
    myconfig_fail "Axidev OSK rerun downloaded a bootstrap installer"
fi

cachyos_profile="$(
    source "$REPO_ROOT/linux/profiles/cachyos.sh"
    declare -f run_profile
)"
arch_wsl_profile="$(
    source "$REPO_ROOT/linux/profiles/arch-wsl.sh"
    declare -f run_profile
)"
ubuntu_profile="$(
    source "$REPO_ROOT/linux/profiles/ubuntu-server.sh"
    declare -f run_profile
)"
[[ "$cachyos_profile" == *module_axidev_osk* ]] \
    || myconfig_fail "CachyOS profile does not include Axidev OSK"
[[ "$arch_wsl_profile" != *module_axidev_osk* ]] \
    || myconfig_fail "Arch WSL profile includes Axidev OSK"
[[ "$ubuntu_profile" != *module_axidev_osk* ]] \
    || myconfig_fail "Ubuntu Server profile includes Axidev OSK"

module_source="$(declare -f module_axidev_osk)"
[[ "$module_source" == *'/usr/local/sbin/axidev-osk-install'* ]] \
    || myconfig_fail "Axidev OSK module does not use the canonical lifecycle path"
[[ "$module_source" == *'/usr/local/bin/axidev-osk'* ]] \
    || myconfig_fail "Axidev OSK module does not use the canonical application path"

MYCONFIG_PROFILE=arch-wsl
if module_axidev_osk >/dev/null 2>&1; then
    myconfig_fail "Axidev OSK module accepted a non-CachyOS profile"
fi

source "$REPO_ROOT/linux/modules/agents.sh"
inventory_home="$TEST_HOME/inventory-home"
mkdir -p "$inventory_home"
HOME="$inventory_home"
MYCONFIG_PROFILE=cachyos
write_environment_inventory
grep -Fq 'Axidev OSK with desktop and login-screen startup' "$HOME/environment.md" \
    || myconfig_fail "CachyOS inventory omitted Axidev OSK"

MYCONFIG_PROFILE=arch-wsl
write_environment_inventory
if grep -Fq 'Axidev OSK' "$HOME/environment.md"; then
    myconfig_fail "Arch WSL inventory included Axidev OSK"
fi

paru_test_root="$TEST_HOME/paru-bootstrap"
paru_test_bin="$paru_test_root/bin"
paru_test_log="$paru_test_root/commands.log"
paru_test_toolchain="$paru_test_root/stable-toolchain"
mkdir -p "$paru_test_bin"
: >"$paru_test_log"

cat >"$paru_test_bin/sudo" <<'EOF'
#!/usr/bin/env bash
printf 'sudo:%s\n' "$*" >>"$PARU_TEST_LOG"
"$@"
EOF

cat >"$paru_test_bin/pacman" <<'EOF'
#!/usr/bin/env bash
printf 'pacman:%s\n' "$*" >>"$PARU_TEST_LOG"
EOF

cat >"$paru_test_bin/cargo" <<'EOF'
#!/usr/bin/env bash
[ -f "$PARU_TEST_TOOLCHAIN" ]
EOF

cat >"$paru_test_bin/rustup" <<'EOF'
#!/usr/bin/env bash
printf 'rustup:%s\n' "$*" >>"$PARU_TEST_LOG"
[ "$*" = 'default stable' ] && touch "$PARU_TEST_TOOLCHAIN"
EOF

cat >"$paru_test_bin/git" <<'EOF'
#!/usr/bin/env bash
printf 'git:%s\n' "$*" >>"$PARU_TEST_LOG"
[ "$1" = clone ] && mkdir -p "$3"
EOF

cat >"$paru_test_bin/makepkg" <<'EOF'
#!/usr/bin/env bash
[ -f "$PARU_TEST_TOOLCHAIN" ] || exit 1
printf 'makepkg:%s\n' "$*" >>"$PARU_TEST_LOG"
if [ "$1" = --packagelist ]; then
    printf '%s\n' "$PWD/paru-test.pkg.tar.zst"
fi
EOF

chmod +x "$paru_test_bin"/*
export PARU_TEST_LOG="$paru_test_log"
export PARU_TEST_TOOLCHAIN="$paru_test_toolchain"

(
    export PATH="$paru_test_bin:/usr/bin:/bin"
    source "$REPO_ROOT/linux/adapters/arch.sh"
    command() {
        if [ "$1" = -v ] && [ "$2" = paru ]; then
            return 1
        fi
        builtin command "$@"
    }
    adapter_ensure_paru
)

grep -Fq 'pacman:-S --needed --noconfirm base-devel git rustup' "$paru_test_log" \
    || myconfig_fail "Paru bootstrap did not install its Rust build dependency"
grep -Fq 'rustup:default stable' "$paru_test_log" \
    || myconfig_fail "Paru bootstrap did not configure a missing Rust toolchain"
grep -Fq 'makepkg:--noconfirm' "$paru_test_log" \
    || myconfig_fail "Paru bootstrap did not build after configuring Rust"
grep -Fq 'pacman:-U --needed --noconfirm ' "$paru_test_log" \
    || myconfig_fail "Paru bootstrap did not install through the cached sudo credential"

source "$REPO_ROOT/linux/modules/authentication.sh"
auth_output="$(offer_authentication Test "test login" false)"
[[ "$auth_output" == *"Test is not authenticated. Run: test login"* ]] \
    || myconfig_fail "noninteractive authentication instructions were not printed"

printf 'Linux installer tests passed.\n'
