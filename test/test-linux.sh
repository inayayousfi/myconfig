#!/usr/bin/env bash

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TEST_HOME="$(mktemp -d)"
trap 'rm -rf "$TEST_HOME"' EXIT

export HOME="$TEST_HOME/home"
export MYCONFIG_PROFILE=ubuntu-server
export MYCONFIG_ADAPTER=test
export MYCONFIG_REPO_ROOT="$REPO_ROOT"
mkdir -p "$HOME"

source "$REPO_ROOT/linux/lib/common.sh"
source "$REPO_ROOT/linux/lib/packages.sh"
source "$REPO_ROOT/linux/lib/input-access.sh"

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

adapter_remove_specs() {
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
[ "$(resolve_package ghostty)" = official:ghostty ] \
    || myconfig_fail "Ghostty package did not resolve"
[ "$(resolve_package emacs_wayland)" = official:emacs-wayland ] \
    || myconfig_fail "Native Wayland Emacs package did not resolve"
[ "$(resolve_package ufw)" = official:ufw ] \
    || myconfig_fail "UFW package did not resolve"
[ "$(resolve_package sshfs)" = official:sshfs ] \
    || myconfig_fail "SSHFS package did not resolve"
[ "$(resolve_package cachy_update)" = official:cachy-update ] \
    || myconfig_fail "Cachy Update package did not resolve"
[ "$(resolve_package cachyos_hello)" = official:cachyos-hello ] \
    || myconfig_fail "CachyOS Hello package did not resolve"
[ "$(resolve_package cachyos_kernel_manager)" = official:cachyos-kernel-manager ] \
    || myconfig_fail "CachyOS Kernel Manager package did not resolve"
[ "$(resolve_package linux_cachyos)" = official:linux-cachyos ] \
    || myconfig_fail "CachyOS stable kernel package did not resolve"
[ "$(resolve_package cachyos_zsh_config)" = official:cachyos-zsh-config ] \
    || myconfig_fail "CachyOS Zsh configuration package did not resolve"
[ "$(resolve_package vim)" = official:vim ] \
    || myconfig_fail "CachyOS Vim removal package did not resolve"
[ "$(resolve_package fish)" = official:fish ] \
    || myconfig_fail "Fish package did not resolve"
[ "$(resolve_package cachyos_fish_config)" = official:cachyos-fish-config ] \
    || myconfig_fail "CachyOS Fish configuration package did not resolve"
[ "$(resolve_package kde_utilities_meta)" = official:kde-utilities-meta ] \
    || myconfig_fail "KDE utilities metadata package did not resolve"
[ "$(resolve_package iosevka_font)" = official:ttf-iosevka-nerd ] \
    || myconfig_fail "KDE Plasma Iosevka font package did not resolve"
[ "$(resolve_package desktop_file_utils)" = official:desktop-file-utils ] \
    || myconfig_fail "KDE Plasma desktop-entry validation package did not resolve"
[ "$(resolve_package ydotool)" = official:ydotool ] \
    || myconfig_fail "ydotool package did not resolve"
[ "${MYCONFIG_PACKAGE_DEFAULTS[kanata]}" = aur:kanata-bin ] \
    || myconfig_fail "Kanata package did not resolve from the AUR"
[ "${MYCONFIG_PACKAGE_DEFAULTS[handy]}" = aur:handy-bin ] \
    || myconfig_fail "Handy package did not resolve from the AUR"
[ "$(remove_package_ids kitty alacritty wezterm konsole)" = $'official:kitty\nofficial:alacritty\nofficial:wezterm\nofficial:konsole' ] \
    || myconfig_fail "package removal identifiers did not resolve"

source "$REPO_ROOT/linux/modules/cachyos.sh"
cachyos_actions="$({
    install_package_ids() {
        printf 'install:%s\n' "$*"
    }
    remove_package_ids() {
        printf 'remove:%s\n' "$*"
    }
    module_cachyos
})"
[ "$cachyos_actions" = $'[myconfig][cachyos] Configuring CachyOS stable kernel tools\ninstall:cachyos_kernel_manager linux_cachyos noto_fonts_cjk\nremove:konsole alacritty cachyos_hello cachyos_zsh_config vim fish cachyos_fish_config fish_autopair fish_pure_prompt fisher firefox firefox_i18n_fr meslo_font cachyos_emerald_kde_theme cachyos_iridescent_kde cachyos_nord_kde_theme kate micro cachyos_micro_settings nano nano_syntax_highlighting meld glances duf tealdeer filelight pavucontrol kcalc shelly cachyos_packageinstaller expac cachyos_wallpapers hwdetect qtscrcpy' ] \
    || myconfig_fail "CachyOS module did not configure the stable kernel tools and removals"

source "$REPO_ROOT/linux/modules/cli.sh"
source "$REPO_ROOT/linux/modules/terminal-tools.sh"
source "$REPO_ROOT/linux/modules/runtimes.sh"
removed_packages=(
    firefox firefox-i18n-fr ttf-meslo-nerd
    cachyos-emerald-kde-theme-git cachyos-iridescent-kde cachyos-nord-kde-theme-git
    kate micro cachyos-micro-settings nano nano-syntax-highlighting meld
    glances duf tealdeer eza filelight pavucontrol kcalc shelly
    cachyos-packageinstaller expac cachyos-wallpapers fd hwdetect fzf bat
    hunk-bin neovim lazygit tmux qtscrcpy zoxide
)
retired_cli_packages=(fd fzf zoxide eza bat hunk-bin neovim lazygit tmux)

for cleanup_profile in cachyos arch-wsl; do
    profile_actions="$(
        MYCONFIG_PROFILE="$cleanup_profile"
        adapter_supports_source() { return 0; }
        adapter_install_specs() { printf 'install:%s\n' "$@"; }
        adapter_remove_specs() { printf 'remove:%s\n' "$@"; }
        rustup() { :; }
        # Exercise the real profile and package modules without running services,
        # deploying dotfiles, or installing anything on the host.
        for unrelated_module in ssh ssh_server flatpak appimage zsh ghostty axidev_osk tailscale \
            agents_packages dotfiles android_phone emacs refind \
            kanata kde_plasma_validate kde_plasma kanata_kde handy pipewire \
            docker agents_configure t3code authentication; do
            eval "module_${unrelated_module}() { :; }"
        done
        write_environment_inventory() { :; }
        source "$REPO_ROOT/linux/modules/base.sh"
        source "$REPO_ROOT/linux/profiles/$cleanup_profile.sh"
        run_profile
    )"
    profile_removed_packages=("${retired_cli_packages[@]}")
    if [ "$cleanup_profile" = cachyos ]; then
        profile_removed_packages=("${removed_packages[@]}")
    fi
    for package in "${profile_removed_packages[@]}"; do
        grep -Eq "^remove:(official|aur):${package}$" <<<"$profile_actions" \
            || myconfig_fail "$cleanup_profile did not request removal of $package"
    done
    for package in "${removed_packages[@]}"; do
        if [ "$cleanup_profile" = arch-wsl ] \
            && [[ " ${retired_cli_packages[*]} " != *" $package "* ]] \
            && grep -Eq "^remove:(official|aur):${package}$" <<<"$profile_actions"; then
            myconfig_fail "Arch WSL requested CachyOS-specific removal of $package"
        fi
        if grep -Eq "^install:(official|aur):${package}$" <<<"$profile_actions"; then
            myconfig_fail "$cleanup_profile still requested installation of $package"
        fi
    done
    for package in ripgrep jq fastfetch btop tokei jdk-openjdk maven; do
        grep -Fxq "install:official:$package" <<<"$profile_actions" \
            || myconfig_fail "$cleanup_profile omitted retained package $package"
    done
    if [ "$cleanup_profile" = cachyos ]; then
        grep -Fxq 'install:official:noto-fonts-cjk' <<<"$profile_actions" \
            || myconfig_fail "CachyOS omitted multilingual fonts"
    elif grep -Fxq 'install:official:noto-fonts-cjk' <<<"$profile_actions"; then
        myconfig_fail "Arch WSL still requested CachyOS-only fonts"
    fi
    if grep -Fxq 'remove:official:noto-fonts-cjk' <<<"$profile_actions"; then
        myconfig_fail "$cleanup_profile still requested removal of multilingual fonts"
    fi
done

source "$REPO_ROOT/linux/modules/base.sh"
base_actions="$({
    install_package_ids() {
        printf 'install:%s\n' "$*"
    }
    remove_package_ids() {
        printf 'remove:%s\n' "$*"
    }
    module_base
})"
[[ "$base_actions" == *$'remove:cachy_update\n'* ]] \
    || myconfig_fail "CachyOS base module did not remove Cachy Update"

if resolve_package hunk >/dev/null 2>&1; then
    myconfig_fail "unsupported AUR source was accepted"
fi

if resolve_package missing_package >/dev/null 2>&1; then
    myconfig_fail "missing package mapping was accepted"
fi

arch_remove_log="$TEST_HOME/arch-remove.log"
: >"$arch_remove_log"
(
    source "$REPO_ROOT/linux/adapters/arch.sh"
    pacman() {
        if [ "$1" = -Q ]; then
            case "$2" in
                kitty | konsole) return 0 ;;
                *) return 1 ;;
            esac
        fi
    }
    sudo() {
        printf 'sudo:%s\n' "$*" >>"$arch_remove_log"
    }

    adapter_remove_specs official:kitty official:alacritty official:wezterm official:konsole
)
grep -Fxq 'sudo:pacman -R --noconfirm kitty konsole' "$arch_remove_log" \
    || myconfig_fail "Arch adapter did not remove only installed named packages"

: >"$arch_remove_log"
(
    source "$REPO_ROOT/linux/adapters/arch.sh"
    pacman() {
        return 1
    }
    sudo() {
        printf 'sudo:%s\n' "$*" >>"$arch_remove_log"
    }

    adapter_remove_specs official:alacritty official:wezterm
)
[ ! -s "$arch_remove_log" ] \
    || myconfig_fail "Arch adapter invoked pacman when no requested package was installed"

apt_remove_log="$TEST_HOME/apt-remove.log"
: >"$apt_remove_log"
(
    source "$REPO_ROOT/linux/adapters/apt.sh"
    dpkg-query() {
        local package="${!#}"
        case "$package" in
            kitty | konsole) printf 'install ok installed' ;;
            *) return 1 ;;
        esac
    }
    sudo() {
        printf 'sudo:%s\n' "$*" >>"$apt_remove_log"
    }

    adapter_remove_specs official:kitty official:alacritty official:wezterm official:konsole
)
grep -Fxq 'sudo:apt-get remove -y kitty konsole' "$apt_remove_log" \
    || myconfig_fail "apt adapter did not remove only installed named packages"

: >"$apt_remove_log"
(
    source "$REPO_ROOT/linux/adapters/apt.sh"
    dpkg-query() {
        return 1
    }
    sudo() {
        printf 'sudo:%s\n' "$*" >>"$apt_remove_log"
    }

    adapter_remove_specs official:alacritty official:wezterm
)
[ ! -s "$apt_remove_log" ] \
    || myconfig_fail "apt adapter invoked apt-get when no requested package was installed"

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

powershell_source="$TEST_HOME/crlf-powershell-source"
powershell_staging="$TEST_HOME/crlf-powershell-staging"
mkdir -p "$powershell_source/ai" "$powershell_staging/ai"
printf 'Write-Host "ok"\r\n' >"$powershell_source/ai/windows.ps1"
cp "$powershell_source/ai/windows.ps1" "$powershell_staging/ai/windows.ps1"
validate_staged_dotfiles "$powershell_source" "$powershell_staging" ai

(
    HOME="$TEST_HOME/ghostty-profile-home"
    MYCONFIG_PROFILE=cachyos
    MYCONFIG_DOTFILES_SOURCE="$REPO_ROOT/dotfiles"
    mkdir -p "$HOME/.config/ghostty"
    touch "$HOME/.config/ghostty/config.ghostty"

    module_dotfiles
    ghostty_config="$HOME/.config/ghostty/config"
    ghostty_deployed="$HOME/dotfiles/ghostty/.config/ghostty/config"
    [ -L "$ghostty_config" ] \
        && [ "$(readlink -f "$ghostty_config")" = "$ghostty_deployed" ] \
        || myconfig_fail "CachyOS did not link the Ghostty configuration into its deployed package"
    cmp "$REPO_ROOT/dotfiles/ghostty/.config/ghostty/config" "$ghostty_config" \
        || myconfig_fail "CachyOS did not deploy the saved Ghostty configuration"
    [ -f "$HOME/.config/ghostty/config.ghostty" ] \
        && [ ! -s "$HOME/.config/ghostty/config.ghostty" ] \
        || myconfig_fail "Ghostty deployment changed the existing empty config.ghostty"

    module_dotfiles
    [ -L "$ghostty_config" ] \
        && [ "$(readlink -f "$ghostty_config")" = "$ghostty_deployed" ] \
        || myconfig_fail "CachyOS rerun retired the active Ghostty configuration"
    cmp "$REPO_ROOT/dotfiles/ghostty/.config/ghostty/config" "$ghostty_config" \
        || myconfig_fail "CachyOS rerun changed the Ghostty configuration"
)

profile_home="$TEST_HOME/profile-home"
HOME="$profile_home"
MYCONFIG_PROFILE=arch-wsl
MYCONFIG_DOTFILES_SOURCE="$REPO_ROOT/dotfiles"
mkdir -p "$HOME/.config" "$HOME/dotfiles/nvim/.config/nvim"
printf 'retired\n' >"$HOME/dotfiles/nvim/.config/nvim/init.lua"
stow --dir "$HOME/dotfiles" --target "$HOME" nvim
[ -L "$HOME/.config/nvim" ] \
    || myconfig_fail "retired Neovim fixture was not stowed"

module_dotfiles
[ ! -e "$HOME/.config/nvim" ] && [ ! -L "$HOME/.config/nvim" ] \
    || myconfig_fail "Arch WSL migration preserved a retired Neovim dotfile link"
shopt -s nullglob
retired_backups=("$HOME"/dotfiles.backup.*/nvim/.config/nvim/init.lua)
[ "${#retired_backups[@]}" -eq 1 ] \
    || myconfig_fail "Arch WSL migration did not back up its retired Neovim package"
module_dotfiles
[ "$(readlink -f "$HOME/.local/bin")" = "$(readlink -f "$HOME")/.local/bin" ] \
    || myconfig_fail "Arch WSL folded ~/.local/bin into a deployed package"
[ "$(readlink -f "$HOME/.claude")" = "$(readlink -f "$HOME")/.claude" ] \
    || myconfig_fail "Arch WSL folded ~/.claude into a deployed package"
[ -x "$HOME/.local/bin/claude-config-helper" ] \
    && [ "$(readlink -f "$HOME/.local/bin/claude-config-helper")" = "$HOME/dotfiles/ai/.local/bin/claude-config-helper" ] \
    || myconfig_fail "Arch WSL did not link claude-config-helper from the ai package"

mapfile -t profile_packages < <(dotfile_packages_for_profile)
for package in "${profile_packages[@]}"; do
    diff -r "$REPO_ROOT/dotfiles/$package" "$HOME/dotfiles/$package" \
        || myconfig_fail "profile rerun changed the staged $package package"
done

shopt -s nullglob
profile_conflict_backups=("$HOME"/.dotfiles-conflicts.backup.*)
[ "${#profile_conflict_backups[@]}" -eq 0 ] \
    || myconfig_fail "profile rerun treated managed dotfiles as conflicts"

"$REPO_ROOT/dotfiles/ai/.local/bin/claude-config-helper" check \
    "$REPO_ROOT/dotfiles/ai/.claude/settings.json" \
    "$REPO_ROOT/dotfiles/ai/.config/claude-config-helper/mcp-servers.json" \
    || myconfig_fail "Tracked Claude configuration contains personal data"

zsh -n "$REPO_ROOT/dotfiles/zsh/.zshrc"
zsh -n "$REPO_ROOT/dotfiles/zsh/.oh-my-zsh/custom/plugins/inaya/inaya.plugin.zsh"
zsh -n "$REPO_ROOT/dotfiles/zsh/.oh-my-zsh/custom/themes/blacknpink.zsh-theme"

for editor_case in emacs vim vi; do
    HOME="$TEST_HOME/editor-$editor_case" MYCONFIG_EDITOR_CASE="$editor_case" \
        zsh -f -c '
        # Simulate command availability, including a retired editor, without
        # launching programs or using the host shell configuration.
        command() {
            if [[ "$1" = -v ]]; then
                case "$2" in
                    emacs) [[ "$MYCONFIG_EDITOR_CASE" = emacs ]] ;;
                    vim) [[ "$MYCONFIG_EDITOR_CASE" = vim ]] ;;
                    nvim) return 0 ;;
                    *) return 1 ;;
                esac
            else
                builtin command "$@"
            fi
        }
        uname() { print Linux; }
        go() { print "$HOME/go"; }
        bun() { print "$HOME/.bun/bin"; }
        if [[ "$MYCONFIG_EDITOR_CASE" = emacs ]]; then
            unset WSL_DISTRO_NAME
        else
            export WSL_DISTRO_NAME=Arch
        fi
        source "$1" || exit 1
        expected="$MYCONFIG_EDITOR_CASE"
        if [[ "$MYCONFIG_EDITOR_CASE" = emacs ]]; then
            expected="emacsclient --socket-name=remot --alternate-editor=emacs"
        fi
        [[ "$EDITOR" = "$expected" && "$VISUAL" = "$expected" ]] || exit 1
        if [[ "$MYCONFIG_EDITOR_CASE" = emacs ]]; then
            (( ! $+aliases[vim] )) && [[ "${aliases[vi]}" = emacs && "${aliases[v]}" = emacs ]] \
                || exit 1
        else
            (( ! $+aliases[vim] && ! $+aliases[vi] && ! $+aliases[v] )) || exit 1
        fi
        ' editor-selection "$REPO_ROOT/dotfiles/zsh/.oh-my-zsh/custom/plugins/inaya/inaya.plugin.zsh" \
        || myconfig_fail "shell did not select the $editor_case editor without retired-tool aliases"
done

python - "$REPO_ROOT/dotfiles/emacs/.config/emacs/lisp/remot.el" <<'PY' || myconfig_fail "Pinned embedded ghostty-web 0.4.0 assets changed"
import base64
import hashlib
import re
import sys

source = open(sys.argv[1], encoding="utf-8").read()
expected = {
    "ghostty-web-js": "078b3fe37e4ef469d3f3d7772ee263070f8613e5a6f5ff305c85778907f45e72",
    "ghostty-vt-wasm": "d6f0326f1874ad2ce9f289e3a4a0c5f3507d4cb38d8747e4b287def470a0c60a",
    "ghostty-web-license": "5eccd0eeca906db6d661b64dd05e1d4a4b2e49d37d43bbcfd2c8cce4d7832920",
}
for name, digest in expected.items():
    match = re.search(
        rf"\(defconst remot--{name}-base64\n  \(concat\n(.*?)\n   \)\)",
        source,
        re.DOTALL,
    )
    if match is None:
        raise SystemExit(f"missing embedded asset: {name}")
    encoded = "".join(re.findall(r'"([A-Za-z0-9+/=]+)"', match.group(1)))
    actual = hashlib.sha256(base64.b64decode(encoded, validate=True)).hexdigest()
    if actual != digest:
        raise SystemExit(f"checksum mismatch for embedded asset: {name}")
PY

(
    HOME="$TEST_HOME/emacs-module-home"
    MYCONFIG_PROFILE=cachyos
    package_log="$TEST_HOME/emacs-packages.log"
    ufw_log="$TEST_HOME/emacs-ufw.log"

    mkdir -p \
        "$HOME/.config/emacs/lisp" \
        "$HOME/.config/environment.d" \
        "$HOME/.local/bin" \
        "$HOME/.emacs.d"
    touch \
        "$HOME/.config/emacs/early-init.el" \
        "$HOME/.config/emacs/init.el" \
        "$HOME/.config/environment.d/emacs.conf" \
        "$HOME/.emacs.d/legacy"
    install -m 755 /dev/null "$HOME/.local/bin/emt"

    install_package_ids() {
        printf '%s\n' "$@" >>"$package_log"
    }
    sudo() {
        printf '%s\n' "$*" >>"$ufw_log"
    }

    source "$REPO_ROOT/linux/modules/emacs.sh"
    module_emacs

    [ "$(cat "$package_log")" = $'emacs_wayland\nsshfs\niosevka_font\nufw' ] \
        || myconfig_fail "Emacs module installed unexpected packages"
    [ -f "$HOME/.emacs.d.backup."*/legacy ] \
        || myconfig_fail "Emacs module did not preserve the legacy Emacs directory"
    [ "$(cat "$ufw_log")" = $'ufw allow in proto tcp from 10.0.0.0/8 to any port 18080,18081 comment myconfig Emacs browser terminal\nufw allow in proto tcp from 172.16.0.0/12 to any port 18080,18081 comment myconfig Emacs browser terminal\nufw allow in proto tcp from 192.168.0.0/16 to any port 18080,18081 comment myconfig Emacs browser terminal\nufw --force enable' ] \
        || myconfig_fail "Emacs module did not enforce the private-LAN browser firewall policy"

    MYCONFIG_PROFILE=arch-wsl
    if module_emacs >/dev/null 2>&1; then
        myconfig_fail "Emacs module accepted a non-CachyOS profile"
    fi
)

# emt joins the graphical Emacs through Remot's server, starts graphical
# Emacs when none runs, and refuses to start a second Emacs beside one
# that lacks the server, since both would save the same workspaces.
(
    emt_bin="$TEST_HOME/emt-bin"
    emt_log="$TEST_HOME/emt.log"
    mkdir -p "$emt_bin"
    cat >"$emt_bin/emacsclient" <<'SH'
#!/usr/bin/env bash
printf 'emacsclient %s\n' "$*" >>"$EMT_LOG"
[ -e "$EMT_STATE/remot" ]
SH
    cat >"$emt_bin/pgrep" <<'SH'
#!/usr/bin/env bash
[ -e "$EMT_STATE/other-emacs" ]
SH
    cat >"$emt_bin/kstart" <<'SH'
#!/usr/bin/env bash
printf 'kstart %s\n' "$*" >>"$EMT_LOG"
touch "$EMT_STATE/remot"
SH
    chmod +x "$emt_bin"/*
    export PATH="$emt_bin:$PATH" EMT_LOG="$emt_log" EMT_STATE="$TEST_HOME/emt-state"
    emt="$REPO_ROOT/dotfiles/emacs/.local/bin/emt"
    open_line='emacsclient --socket-name=remot --tty --eval (remot-show-in-terminal-frame)'

    reset_emt() { rm -rf "$EMT_STATE" "$EMT_LOG"; mkdir -p "$EMT_STATE"; }

    reset_emt
    touch "$EMT_STATE/remot"
    "$emt"
    [ "$(tail -n 1 "$EMT_LOG")" = "$open_line" ] && ! grep -q kstart "$EMT_LOG" \
        || myconfig_fail "emt did not open a terminal frame in the running Remot Emacs"

    reset_emt
    touch "$EMT_STATE/other-emacs"
    if WAYLAND_DISPLAY=wayland-0 "$emt" 2>/dev/null; then
        myconfig_fail "emt accepted an Emacs without the remot server"
    fi
    ! grep -q -e kstart -e -- --tty "$EMT_LOG" \
        || myconfig_fail "emt started a second Emacs beside one without the remot server"

    reset_emt
    if env -u WAYLAND_DISPLAY -u DISPLAY "$emt" 2>/dev/null; then
        myconfig_fail "emt succeeded without Emacs or a desktop session"
    fi

    reset_emt
    WAYLAND_DISPLAY=wayland-0 "$emt"
    grep -qx 'kstart --application emacs' "$EMT_LOG" \
        && [ "$(tail -n 1 "$EMT_LOG")" = "$open_line" ] \
        || myconfig_fail "emt did not start graphical Emacs and then open a terminal frame"
)

source "$REPO_ROOT/linux/modules/axidev-osk.sh"
source "$REPO_ROOT/linux/modules/ghostty.sh"
source "$REPO_ROOT/linux/modules/refind.sh"
source "$REPO_ROOT/linux/modules/kde-plasma.sh"
source "$REPO_ROOT/linux/modules/kanata.sh"
source "$REPO_ROOT/linux/modules/kanata-kde.sh"
source "$REPO_ROOT/linux/modules/handy.sh"
source "$REPO_ROOT/linux/modules/docker.sh"

(
    id() {
        case "$*" in
            '-un') printf 'test-user\n' ;;
            '-Gn') printf 'users\n' ;;
            '-nG test-user') printf 'users docker\n' ;;
            *) return 1 ;;
        esac
    }
    user_is_in_group docker \
        || myconfig_fail "Docker group check missed configured membership"
)

validate_kde_plasma_version 'plasma-workspace 6.7.0-1'
validate_kde_plasma_version 'plasma-workspace 6.99.4-2'
if validate_kde_plasma_version 'plasma-workspace 6.6.5-1' >/dev/null 2>&1; then
    myconfig_fail "KDE Plasma configuration accepted Plasma older than 6.7"
fi
if validate_kde_plasma_version 'plasma-workspace 7.0.0-1' >/dev/null 2>&1; then
    myconfig_fail "KDE Plasma configuration accepted unverified Plasma 7"
fi
if validate_kde_plasma_version 'unknown version' >/dev/null 2>&1; then
    myconfig_fail "KDE Plasma configuration accepted an unparseable version"
fi
pointer_plugin="$REPO_ROOT/linux/assets/libinput/90-myconfig-pointer-sensitivity.lua"
MYCONFIG_POINTER_PLUGIN="$pointer_plugin" lua -e '
libinput = {
    register = function() return 1 end,
    connect = function(self, name, callback) self.callback = callback end,
}
evdev = { REL_X = 1, REL_Y = 2 }
dofile(os.getenv("MYCONFIG_POINTER_PLUGIN"))
local handler
local device = {
    usages = function() return { [evdev.REL_X] = true, [evdev.REL_Y] = true } end,
    connect = function(self, name, callback) handler = callback end,
}
libinput.callback(device)
local frame = { { usage = evdev.REL_X, value = 1 }, { usage = evdev.REL_Y, value = -1 } }
assert(handler(device, frame) == frame)
assert(frame[1].value == 4 and frame[2].value == -4)
'
grep -Fq 'local multiplier = 4' "$pointer_plugin" \
    || myconfig_fail "libinput pointer-sensitivity multiplier changed"
grep -Fq '"$MYCONFIG_REPO_ROOT"/linux/assets/libinput/*.lua' \
    "$REPO_ROOT/linux/modules/kde-plasma.sh" \
    && grep -Fq '"/etc/libinput/plugins/${libinput_plugin##*/}"' \
        "$REPO_ROOT/linux/modules/kde-plasma.sh" \
    || myconfig_fail "KDE Plasma module does not install the libinput plugins"
refind_asset_root="$REPO_ROOT/linux/assets/refind"
refind_global_config="$refind_asset_root/global.conf"
refind_theme_root="$refind_asset_root/themes/black-pink"
refind_theme_test_dir="$TEST_HOME/refind-theme"
if command -v magick >/dev/null 2>&1; then
    refind_image_tool=magick
else
    refind_image_tool=convert
fi
grep -Fxq 'enable_mouse true' "$refind_global_config" \
    || myconfig_fail "rEFInd configuration does not enable mouse input"
grep -Fxq 'banner themes/black-pink/banner.png' "$refind_theme_root/theme.conf" \
    || myconfig_fail "Black & Pink rEFInd theme does not use its historical banner"
grep -Fxq 'hideui hints,label,singleuser,arrows,badges' "$refind_theme_root/theme.conf" \
    || myconfig_fail "Black & Pink rEFInd theme changed its minimal layout"
if grep -Eq '^(icons_dir|showtools)[[:space:]]' "$refind_theme_root/theme.conf"; then
    myconfig_fail "Black & Pink rEFInd theme overrides built-in icons or tools"
fi
[ ! -d "$refind_theme_root/icons" ] \
    || myconfig_fail "Black & Pink rEFInd theme includes custom icons"
"$refind_theme_root/generate-theme-assets.sh" "$refind_theme_test_dir"
[ "$("$refind_image_tool" "$refind_theme_test_dir/banner.png" -format '%wx%h|%[pixel:p{0,0}]|%[pixel:p{0,1079}]' info:)" = \
    '1920x1080|srgb(0,0,0)|srgb(255,78,173)' ] \
    || myconfig_fail "Black & Pink rEFInd banner changed its historical colors or dimensions"
for selection in selection_big selection_small; do
    [ -s "$refind_theme_test_dir/$selection.png" ] \
        || myconfig_fail "Black & Pink rEFInd theme did not generate $selection"
done

window_rules_config="$TEST_HOME/kwin-window-rules"
mkdir -p "$window_rules_config"
printf '[General]\ncount=1\nrules=user-rule\n\n[user-rule]\nDescription=User rule\n' \
    >"$window_rules_config/kwinrulesrc"
XDG_CONFIG_HOME="$window_rules_config" configure_kde_plasma_window_rules
XDG_CONFIG_HOME="$window_rules_config" configure_kde_plasma_window_rules
[ "$(XDG_CONFIG_HOME="$window_rules_config" kreadconfig6 --file kwinrulesrc --group General --key rules)" \
    = user-rule,myconfig-picture-in-picture ] \
    || myconfig_fail "KDE Plasma window rules did not add the picture-in-picture rule once beside existing rules"
[ "$(XDG_CONFIG_HOME="$window_rules_config" kreadconfig6 --file kwinrulesrc --group General --key count)" = 2 ] \
    || myconfig_fail "KDE Plasma window rules count does not match the rule list"
[ "$(XDG_CONFIG_HOME="$window_rules_config" kreadconfig6 --file kwinrulesrc --group user-rule --key Description)" \
    = 'User rule' ] \
    || myconfig_fail "KDE Plasma window rules removed an existing rule"
[ "$(XDG_CONFIG_HOME="$window_rules_config" kreadconfig6 --file kwinrulesrc --group myconfig-picture-in-picture --key excludefromcapture)" \
    = true ] \
    || myconfig_fail "KDE Plasma picture-in-picture rule does not exclude windows from capture"

MYCONFIG_PROFILE=arch-wsl
if module_kde_plasma >/dev/null 2>&1; then
    myconfig_fail "KDE Plasma module accepted a non-CachyOS profile"
fi

plasma_layout_test_root="$TEST_HOME/kde-plasma"
plasma_layout_home="$plasma_layout_test_root/home"
plasma_layout_bin="$plasma_layout_test_root/bin"
plasma_repo_root="$plasma_layout_test_root/repo"
mkdir -p "$plasma_layout_home/.config" "$plasma_layout_bin" "$plasma_repo_root/linux/assets/glassy-blacknpink-plasma"
ln -s "$REPO_ROOT/linux/assets/libinput" "$plasma_repo_root/linux/assets/libinput"
# Stand-ins for the theme repository's install script and layout command.
cat >"$plasma_repo_root/linux/assets/glassy-blacknpink-plasma/install.sh" <<'EOF'
glassy_blacknpink_install() { printf '%s\n' theme:install >>"$plasma_module_log"; }
glassy_blacknpink_activate() { printf '%s\n' theme:activate >>"$plasma_module_log"; }
EOF
cat >"$plasma_layout_bin/glassy-blacknpink-layout" <<EOF
#!/usr/bin/env bash
printf '%s\n' layout:apply >>"$plasma_layout_test_root/module.log"
EOF
chmod +x "$plasma_layout_bin/glassy-blacknpink-layout"

mkdir -p "$plasma_layout_home/.config/gtk-3.0" "$plasma_layout_home/.config/gtk-4.0" "$plasma_layout_home/.config/xsettingsd"
for file in "$plasma_layout_home/.gtkrc-2.0" "$plasma_layout_home/.config/gtk-3.0/settings.ini" "$plasma_layout_home/.config/gtk-4.0/settings.ini"; do
    printf 'gtk-cursor-theme-size=32\nother-setting=preserved\n' >"$file"
done
printf 'Gtk/CursorThemeSize 32\nOther/Setting 1\n' >"$plasma_layout_home/.config/xsettingsd/xsettingsd.conf"

plasma_module_log="$plasma_layout_test_root/module.log"
: >"$plasma_module_log"
install_package_ids() {
    printf 'packages:%s\n' "$*" >>"$plasma_module_log"
}
plasmashell() {
    myconfig_fail "headless KDE Plasma validation executed plasmashell"
}
pacman() {
    [ "$*" = '-Q plasma-workspace' ] || return 1
    printf 'plasma-workspace 6.7.4-3.1\n'
}
plasma-apply-cursortheme() {
    [ "${QT_QPA_PLATFORM:-}" = offscreen ] || return 1
    printf 'cursors:%s\n' "$*" >>"$plasma_module_log"
}
plasma-apply-lookandfeel() {
    [ "${QT_QPA_PLATFORM:-}" = offscreen ] || return 1
    printf 'global-theme:%s\n' "$*" >>"$plasma_module_log"
}
kwriteconfig6() {
    printf 'config:%s\n' "$*" >>"$plasma_module_log"
}
kreadconfig6() {
    :
}
gsettings() {
    case "$1" in
        list-keys) printf 'cursor-size\n' ;;
        set) printf 'gsettings:%s\n' "$*" >>"$plasma_module_log" ;;
        *) return 1 ;;
    esac
}
fc-match() {
    printf 'IosevkaNerdFont-Regular.ttf: Iosevka Nerd Font\n'
}
systemctl() {
    printf 'systemctl:%s\n' "$*" >>"$plasma_module_log"
}
qdbus6() {
    printf 'qdbus:%s\n' "$*" >>"$plasma_module_log"
}
sudo() {
    printf 'sudo:%s\n' "$*" >>"$plasma_module_log"
}

MYCONFIG_PROFILE=cachyos
(
    MYCONFIG_REPO_ROOT="$plasma_repo_root"
    HOME="$plasma_layout_home" PATH="$plasma_layout_bin:/usr/bin:/bin" module_kde_plasma
)
for file in "$plasma_layout_home/.gtkrc-2.0" "$plasma_layout_home/.config/gtk-3.0/settings.ini" "$plasma_layout_home/.config/gtk-4.0/settings.ini"; do
    grep -Fxq 'gtk-cursor-theme-size=40' "$file" \
        || myconfig_fail "KDE Plasma module did not synchronize GTK cursor size in $file"
    grep -Fxq 'other-setting=preserved' "$file" \
        || myconfig_fail "KDE Plasma module overwrote unrelated GTK settings in $file"
done
grep -Fxq 'Gtk/CursorThemeSize 40' "$plasma_layout_home/.config/xsettingsd/xsettingsd.conf" \
    || myconfig_fail "KDE Plasma module did not synchronize XSettings cursor size"
grep -Fxq 'Other/Setting 1' "$plasma_layout_home/.config/xsettingsd/xsettingsd.conf" \
    || myconfig_fail "KDE Plasma module overwrote unrelated XSettings"
grep -Fxq 'gsettings:set org.gnome.desktop.interface cursor-size 40' "$plasma_module_log" \
    || myconfig_fail "KDE Plasma module did not synchronize GNOME cursor size"
grep -Fxq 'theme:install' "$plasma_module_log" || myconfig_fail "Plasma module omitted the Glassy Black & Pink installation"
grep -Fxq 'theme:activate' "$plasma_module_log" || myconfig_fail "Plasma module omitted the glass activation"
grep -Fxq 'layout:apply' "$plasma_module_log" || myconfig_fail "Plasma module did not apply the theme's panel layout"
grep -Fxq 'systemctl:--user try-restart plasma-plasmashell.service' "$plasma_module_log" \
    || myconfig_fail "Plasma module did not reload the installed widgets"
grep -Fxq 'packages:iosevka_font libinput' "$plasma_module_log" \
    || myconfig_fail "KDE Plasma module omitted its appearance dependencies"
grep -Fxq "sudo:install -Dm644 $plasma_repo_root/linux/assets/libinput/${pointer_plugin##*/} /etc/libinput/plugins/90-myconfig-pointer-sensitivity.lua" \
    "$plasma_module_log" \
    || myconfig_fail "KDE Plasma module did not install the libinput plugin"
grep -Fxq 'global-theme:--apply glassy-blacknpink.desktop' "$plasma_module_log" \
    || myconfig_fail "KDE Plasma module did not apply its global theme headlessly"
grep -Fxq 'cursors:--size 40 glassy-blacknpink' "$plasma_module_log" \
    || myconfig_fail "KDE Plasma module did not apply its cursor theme headlessly"
grep -Fxq 'config:--file kwinrc --group Windows --key ElectricBorderPushbackPixels 0' "$plasma_module_log" \
    || myconfig_fail "KDE Plasma module did not remove KWin edge pushback"
grep -Fxq 'config:--file kwinrc --group EdgeBarrier --key CornerBarrier false' "$plasma_module_log" \
    || myconfig_fail "KDE Plasma module did not disable KWin corner barriers"
grep -Fxq 'config:--file kwinrc --group EdgeBarrier --key EdgeBarrier 0' "$plasma_module_log" \
    || myconfig_fail "KDE Plasma module did not disable KWin edge barriers"
grep -Fxq 'config:--file kwinrc --group Effect-overview --key BorderActivate 9' "$plasma_module_log" \
    || myconfig_fail "KDE Plasma module did not disable the Overview hot corner"
grep -Fxq 'config:--file kwinrc --group Windows --key PerOutputVirtualDesktops true' "$plasma_module_log" \
    || myconfig_fail "KDE Plasma module did not isolate virtual desktops per screen"
grep -Fxq 'config:--file kcminputrc --group Mouse --key cursorSize 40' "$plasma_module_log" \
    || myconfig_fail "KDE Plasma module did not configure the cursor size"
grep -Fxq 'config:--file kcminputrc --group Libinput --group Defaults --group Pointer --key PointerAcceleration 1.000' "$plasma_module_log" \
    || myconfig_fail "KDE Plasma module did not configure pointer sensitivity"
grep -Fxq 'config:--file kcminputrc --group Libinput --group Defaults --group Pointer --key PointerAccelerationProfile 1' "$plasma_module_log" \
    || myconfig_fail "KDE Plasma module did not configure flat pointer acceleration"
grep -Fxq 'config:--file kcminputrc --group Libinput --group Defaults --group Touchpad --key PointerAcceleration 1.000' "$plasma_module_log" \
    || myconfig_fail "KDE Plasma module did not configure touchpad sensitivity"
grep -Fxq 'config:--file kcminputrc --group Libinput --group Defaults --group Touchpad --key PointerAccelerationProfile 1' "$plasma_module_log" \
    || myconfig_fail "KDE Plasma module did not configure flat touchpad acceleration"
grep -Fxq 'config:--file kcminputrc --group Libinput --group Defaults --group Touchpad --key NaturalScroll true' "$plasma_module_log" \
    || myconfig_fail "KDE Plasma module did not enable natural touchpad scrolling"
grep -Fxq 'config:--file kcminputrc --group Libinput --group Defaults --group Touchpad --key TapDragLock true' "$plasma_module_log" \
    || myconfig_fail "KDE Plasma module did not enable touchpad drag lock"
grep -Fxq 'config:--file kcminputrc --group Libinput --group Defaults --group Touchpad --key ClickMethod 2' "$plasma_module_log" \
    || myconfig_fail "KDE Plasma module did not configure touchpad clickfinger mode"
grep -Fxq 'systemctl:--user daemon-reload' "$plasma_module_log" \
    || myconfig_fail "KDE Plasma module did not reload user services"
grep -Fxq 'qdbus:org.kde.KWin /KWin org.kde.KWin.reconfigure' "$plasma_module_log" \
    || myconfig_fail "KDE Plasma module did not reload active KWin configuration"
unset -f sudo

PYTHONPYCACHEPREFIX="$TEST_HOME/pycache" \
    python "$REPO_ROOT/test/test-kanata-tray.py"
PYTHONPYCACHEPREFIX="$TEST_HOME/pycache" \
    python -m py_compile "$REPO_ROOT/dotfiles/kanata-kde/.local/bin/myconfig-kanata-tray"
grep -Fq '(deflayer off' "$REPO_ROOT/dotfiles/kanata/.config/kanata/config.kbd" \
    || myconfig_fail "Kanata Off layer is not the startup layer"
[ "$(grep -Fc 'tap-hold 0 400' "$REPO_ROOT/dotfiles/kanata/.config/kanata/config.kbd")" -eq 18 ] \
    || myconfig_fail "Kanata mappings do not all use the approved 400 ms timing"
grep -Fq 'overview    M-w' "$REPO_ROOT/dotfiles/kanata/.config/kanata/config.kbd" \
    || myconfig_fail "Kanata F19 behavior does not emit the KDE Overview shortcut"

kanata_unit_test_root="$TEST_HOME/kanata-units"
mkdir -p "$kanata_unit_test_root"
sed 's|^ExecStart=.*|ExecStart=/bin/true|' \
    "$REPO_ROOT/dotfiles/kanata/.config/systemd/user/myconfig-kanata.service" \
    >"$kanata_unit_test_root/myconfig-kanata.service"
sed 's|^ExecStart=.*|ExecStart=/bin/true|' \
    "$REPO_ROOT/dotfiles/kanata-kde/.config/systemd/user/myconfig-kanata-tray.service" \
    >"$kanata_unit_test_root/myconfig-kanata-tray.service"
systemd-analyze verify \
    "$kanata_unit_test_root/myconfig-kanata.service" \
    "$kanata_unit_test_root/myconfig-kanata-tray.service"
grep -Fxq 'Wants=myconfig-kanata.service' \
    "$REPO_ROOT/dotfiles/kanata-kde/.config/systemd/user/myconfig-kanata-tray.service" \
    || myconfig_fail "Kanata tray does not start the engine"
grep -Fxq 'After=myconfig-handy.service' \
    "$REPO_ROOT/dotfiles/kanata/.config/systemd/user/myconfig-kanata.service" \
    || myconfig_fail "Kanata engine does not wait for Handy input devices"
grep -Fxq 'RestartMode=direct' \
    "$REPO_ROOT/dotfiles/kanata/.config/systemd/user/myconfig-kanata.service" \
    || myconfig_fail "Kanata failures can still tear down the tray before restart"
if grep -Fq 'myconfig-kanata' \
    "$REPO_ROOT/dotfiles/handy/.config/systemd/user/myconfig-handy.service"; then
    myconfig_fail "Handy retained the reversed Kanata startup ordering"
fi
grep -Fxq 'ExecStopPost=-/usr/bin/systemctl --user stop myconfig-kanata.service' \
    "$REPO_ROOT/dotfiles/kanata-kde/.config/systemd/user/myconfig-kanata-tray.service" \
    || myconfig_fail "stopping the Kanata tray does not stop the engine"
(
    kanata_test_root="$TEST_HOME/kanata-module"
    kanata_test_home="$kanata_test_root/home"
    kanata_test_log="$kanata_test_root/module.log"
    mkdir -p \
        "$kanata_test_home/.config/kanata" \
        "$kanata_test_home/.config/systemd/user" \
        "$kanata_test_home/.config/systemd/user/default.target.wants" \
        "$kanata_test_home/.local/bin"
    cp "$REPO_ROOT/dotfiles/kanata/.config/kanata/config.kbd" \
        "$kanata_test_home/.config/kanata/config.kbd"
    cp "$REPO_ROOT/dotfiles/kanata/.config/systemd/user/myconfig-kanata.service" \
        "$kanata_test_home/.config/systemd/user/myconfig-kanata.service"
    cp "$REPO_ROOT/dotfiles/kanata-kde/.local/bin/myconfig-kanata-tray" \
        "$kanata_test_home/.local/bin/myconfig-kanata-tray"
    cp "$REPO_ROOT/dotfiles/kanata-kde/.config/systemd/user/myconfig-kanata-tray.service" \
        "$kanata_test_home/.config/systemd/user/myconfig-kanata-tray.service"
    ln -s ../myconfig-kanata.service \
        "$kanata_test_home/.config/systemd/user/default.target.wants/myconfig-kanata.service"
    chmod +x "$kanata_test_home/.local/bin/myconfig-kanata-tray"
    : >"$kanata_test_log"

    install_package_ids() {
        printf 'packages:%s\n' "$*" >>"$kanata_test_log"
    }
    kanata() {
        printf 'kanata:%s\n' "$*" >>"$kanata_test_log"
    }
    sudo() {
        printf 'sudo:%s\n' "$*" >>"$kanata_test_log"
        if [ "${1:-}" = tee ]; then
            while IFS= read -r line; do
                printf 'tee-input:%s\n' "$line" >>"$kanata_test_log"
            done
        fi
    }
    udevadm() {
        return 0
    }
    systemctl() {
        printf 'systemctl:%s\n' "$*" >>"$kanata_test_log"
        if [ "$*" = '--user --quiet is-active graphical-session.target' ]; then
            return 1
        fi
    }
    user_has_active_group() {
        return 1
    }

    MYCONFIG_PROFILE=cachyos
    HOME="$kanata_test_home" module_kanata
    grep -Fxq 'packages:kanata' "$kanata_test_log" \
        || myconfig_fail "Kanata module did not install Kanata"
    grep -Fxq "kanata:--check --cfg $kanata_test_home/.config/kanata/config.kbd" "$kanata_test_log" \
        || myconfig_fail "Kanata module did not validate its stowed configuration"
    grep -Fxq 'sudo:usermod -aG input,uinput '"$(id -un)" "$kanata_test_log" \
        || myconfig_fail "Kanata module did not grant both approved input groups"
    grep -Fxq 'sudo:modprobe uinput' "$kanata_test_log" \
        || myconfig_fail "Kanata module did not load uinput"
    grep -Fxq 'sudo:tee /etc/modules-load.d/myconfig-kanata.conf' "$kanata_test_log" \
        || myconfig_fail "Kanata module did not use its named modules-load file"
    grep -Fxq 'sudo:tee /etc/udev/rules.d/99-myconfig-kanata.rules' "$kanata_test_log" \
        || myconfig_fail "Kanata module did not use its named udev file"
    grep -Fxq 'tee-input:KERNEL=="uinput", MODE="0660", GROUP="uinput", OPTIONS+="static_node=uinput"' "$kanata_test_log" \
        || myconfig_fail "Kanata module did not write the uinput permission rule"
    grep -Fxq 'tee-input:SUBSYSTEM=="input", KERNEL=="event*", MODE="0660", GROUP="input"' "$kanata_test_log" \
        || myconfig_fail "Kanata module did not write the keyboard input rule"
    grep -Fxq 'systemctl:--user enable myconfig-kanata.service' "$kanata_test_log" \
        || myconfig_fail "Kanata module did not enable its user service"
    if grep -Fq 'systemctl:--user restart myconfig-kanata.service' "$kanata_test_log"; then
        myconfig_fail "Kanata module started before new group membership became active"
    fi

    python_command="$(command -v python)"
    python() {
        printf 'python:%s\n' "$*" >>"$kanata_test_log"
        "$python_command" "$@"
    }
    kwriteconfig6() {
        printf 'kwriteconfig6:%s\n' "$*" >>"$kanata_test_log"
    }
    module_kde_plasma_validate() {
        return 0
    }

    HOME="$kanata_test_home" module_kanata_kde
    grep -Fxq 'packages:python pyside6' "$kanata_test_log" \
        || myconfig_fail "Kanata KDE module did not install its Python dependencies"
    grep -Fxq 'kwriteconfig6:--file kglobalshortcutsrc --group kwin --key Overview Meta+W,Meta+W,Toggle Overview' "$kanata_test_log" \
        || myconfig_fail "Kanata KDE module did not configure the Overview shortcut"
    grep -Fxq 'systemctl:--user enable myconfig-kanata-tray.service' "$kanata_test_log" \
        || myconfig_fail "Kanata KDE module did not enable its tray service"
    [ ! -e "$kanata_test_home/.config/systemd/user/default.target.wants/myconfig-kanata.service" ] \
        || myconfig_fail "Kanata KDE module left the engine independently enabled"
    [ -f "$kanata_test_home/.config/systemd/user/myconfig-kanata.service" ] \
        || myconfig_fail "Kanata KDE module removed the stowed engine service"
    grep -Fxq 'systemctl:--user stop myconfig-kanata.service' "$kanata_test_log" \
        || myconfig_fail "Kanata KDE module left the engine running without its tray"

    MYCONFIG_PROFILE=arch-wsl
    if HOME="$kanata_test_home" module_kanata_kde >/dev/null 2>&1; then
        myconfig_fail "Kanata KDE module accepted a non-CachyOS profile"
    fi
    if grep -Fqi kanata "$REPO_ROOT/linux/modules/kde-plasma.sh"; then
        myconfig_fail "KDE Plasma module depends on Kanata"
    fi
)

handy_test_root="$TEST_HOME/handy"
handy_test_home="$handy_test_root/home"
handy_test_log="$handy_test_root/module.log"
mkdir -p \
    "$handy_test_home/.local/bin" \
    "$handy_test_home/.config/systemd/user"
cp "$REPO_ROOT/dotfiles/handy/.local/bin/myconfig-handy-configure" \
    "$handy_test_home/.local/bin/myconfig-handy-configure"
cp "$REPO_ROOT/dotfiles/handy/.config/systemd/user/myconfig-handy.service" \
    "$handy_test_home/.config/systemd/user/myconfig-handy.service"
chmod +x "$handy_test_home/.local/bin/myconfig-handy-configure"

HOME="$handy_test_home" \
    XDG_CONFIG_HOME="$handy_test_home/.config" \
    "$handy_test_home/.local/bin/myconfig-handy-configure"
handy_settings="$handy_test_home/.config/com.pais.handy/settings_store.json"
[ "$(jq -r '.settings.keyboard_implementation' "$handy_settings")" = handy_keys ] \
    || myconfig_fail "Handy settings did not select the direct keyboard backend"
[ "$(jq -r '.settings.push_to_talk' "$handy_settings")" = true ] \
    || myconfig_fail "Handy settings did not enable push-to-talk"
[ "$(jq -r '.settings.bindings.transcribe.current_binding' "$handy_settings")" = ctrl+space ] \
    || myconfig_fail "Handy settings did not configure Ctrl+Space"
[ "$(stat -c %a "$handy_settings")" = 600 ] \
    || myconfig_fail "Handy settings are not private"

jq '.settings.selected_model = "keep-model" | .settings.unrelated = {"keep": true}' \
    "$handy_settings" >"$handy_settings.updated"
mv "$handy_settings.updated" "$handy_settings"
HOME="$handy_test_home" \
    XDG_CONFIG_HOME="$handy_test_home/.config" \
    "$handy_test_home/.local/bin/myconfig-handy-configure"
[ "$(jq -r '.settings.selected_model' "$handy_settings")" = keep-model ] \
    || myconfig_fail "Handy settings updater erased the selected model"
[ "$(jq -r '.settings.unrelated.keep' "$handy_settings")" = true ] \
    || myconfig_fail "Handy settings updater erased unrelated settings"

handy_unit_test_root="$TEST_HOME/handy-unit"
mkdir -p "$handy_unit_test_root"
sed \
    -e 's|^ExecStartPre=.*|ExecStartPre=/bin/true|' \
    -e 's|^ExecStart=.*|ExecStart=/bin/true|' \
    "$REPO_ROOT/dotfiles/handy/.config/systemd/user/myconfig-handy.service" \
    >"$handy_unit_test_root/myconfig-handy.service"
systemd-analyze verify "$handy_unit_test_root/myconfig-handy.service"

(
    : >"$handy_test_log"
    install_package_ids() {
        printf 'packages:%s\n' "$*" >>"$handy_test_log"
    }
    handy() {
        return 0
    }
    sudo() {
        printf 'sudo:%s\n' "$*" >>"$handy_test_log"
        if [ "${1:-}" = tee ]; then
            while IFS= read -r line; do
                printf 'tee-input:%s\n' "$line" >>"$handy_test_log"
            done
        fi
    }
    udevadm() {
        return 0
    }
    systemctl() {
        printf 'systemctl:%s\n' "$*" >>"$handy_test_log"
        return 0
    }
    user_has_active_group() {
        return 1
    }

    MYCONFIG_PROFILE=cachyos
    export XDG_CONFIG_HOME="$handy_test_home/.config"
    HOME="$handy_test_home" module_handy
    grep -Fxq 'packages:handy' "$handy_test_log" \
        || myconfig_fail "Handy module did not install Handy"
    grep -Fxq 'sudo:tee /etc/modules-load.d/myconfig-handy.conf' "$handy_test_log" \
        || myconfig_fail "Handy module did not use its named modules-load file"
    grep -Fxq 'sudo:tee /etc/udev/rules.d/99-myconfig-handy.rules' "$handy_test_log" \
        || myconfig_fail "Handy module did not use its named udev file"
    grep -Fxq 'sudo:usermod -aG input,uinput '"$(id -un)" "$handy_test_log" \
        || myconfig_fail "Handy module did not grant both required input groups"
    grep -Fxq 'systemctl:--user enable myconfig-handy.service' "$handy_test_log" \
        || myconfig_fail "Handy module did not enable its user service"
    grep -Fxq 'systemctl:--user stop myconfig-handy.service' "$handy_test_log" \
        || myconfig_fail "Handy module ran before keyboard group membership became active"

    MYCONFIG_PROFILE=arch-wsl
    if HOME="$handy_test_home" module_handy >/dev/null 2>&1; then
        myconfig_fail "Handy module accepted a non-CachyOS profile"
    fi
)

MYCONFIG_PROFILE=cachyos
docker_test_log="$TEST_HOME/docker-module.log"
: >"$docker_test_log"
(
    install_package_ids() {
        printf 'packages:%s\n' "$*" >>"$docker_test_log"
    }
    sudo() {
        printf 'sudo:%s\n' "$*" >>"$docker_test_log"
    }
    systemctl() {
        printf 'systemctl:%s\n' "$*" >>"$docker_test_log"
        [ "$*" != '--user is-active docker.service' ] || printf 'active\n'
    }
    docker() {
        printf 'docker:%s\n' "$*" >>"$docker_test_log"
        case "$1 $2" in
            'context inspect') return 1 ;;
            'info --format') printf '[name=seccomp,profile=builtin name=rootless name=cgroupns]\n' ;;
        esac
    }
    id() {
        [ "$*" != -u ] || printf '1000\n'
    }
    user_is_in_group() {
        return 1
    }

    module_docker
)
grep -Fxq 'packages:docker docker_buildx docker_compose docker_rootless_extras slirp4netns' "$docker_test_log" \
    || myconfig_fail "Docker module did not install the rootless Docker packages"
grep -Fxq 'sudo:systemctl disable --now docker.service docker.socket' "$docker_test_log" \
    || myconfig_fail "Docker module did not disable the system Docker daemon"
grep -Fxq 'systemctl:--user enable --now docker.service' "$docker_test_log" \
    || myconfig_fail "Docker module did not enable and start the rootless daemon"
grep -Fxq 'docker:context create rootless --description Rootless Docker daemon --docker host=unix:///run/user/1000/docker.sock' "$docker_test_log" \
    || myconfig_fail "Docker module did not create the rootless context"
grep -Fxq 'docker:context use rootless' "$docker_test_log" \
    || myconfig_fail "Docker module did not select the rootless context"

: >"$docker_test_log"
(
    install_package_ids() { :; }
    sudo() { :; }
    systemctl() {
        [ "$*" != '--user is-active docker.service' ] || printf 'active\n'
    }
    docker() {
        [ "$1" != info ] || printf '[name=seccomp,profile=builtin]\n'
    }
    user_is_in_group() {
        return 1
    }
    module_docker
) >/dev/null 2>&1 && myconfig_fail "Docker module accepted a daemon that is not rootless"

: >"$docker_test_log"
(
    install_package_ids() {
        myconfig_fail "Docker module installed packages for a docker-group member"
    }
    user_is_in_group() {
        return 0
    }
    module_docker
) >/dev/null 2>&1 && myconfig_fail "Docker module accepted a docker-group member"
[ ! -s "$docker_test_log" ] \
    || myconfig_fail "Docker module changed system state after its docker-group refusal"

MYCONFIG_PROFILE=arch-wsl
if module_docker >/dev/null 2>&1; then
    myconfig_fail "Docker module accepted a non-CachyOS profile"
fi

MYCONFIG_PROFILE=cachyos
ghostty_actions="$({
    install_package_ids() {
        printf 'install:%s\n' "$*"
    }
    remove_package_ids() {
        printf 'remove:%s\n' "$*"
    }
    module_ghostty
})"
[ "$ghostty_actions" = $'[myconfig][cachyos] Installing Ghostty\ninstall:ghostty' ] \
    || myconfig_fail "Ghostty module did not install Ghostty"

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

source "$REPO_ROOT/linux/modules/tailscale.sh"
tailscale_test_log="$TEST_HOME/tailscale-module.log"
: >"$tailscale_test_log"
(
    install_package_ids() { :; }
    sudo() {
        printf 'sudo:%s\n' "$*" >>"$tailscale_test_log"
    }
    systemctl() {
        printf 'active\n'
    }
    tailscale() {
        printf '{"OperatorUser":"%s"}\n' "$(id -un)"
    }
    module_tailscale
)
grep -Fxq "sudo:tailscale set --operator=$(id -un)" "$tailscale_test_log" \
    || myconfig_fail "Tailscale module did not make the installing user the operator"

source "$REPO_ROOT/linux/modules/t3code.sh"
t3_test_home="$TEST_HOME/t3code-home"
t3_test_bin="$TEST_HOME/t3code-bin"
t3_test_log="$TEST_HOME/t3code.log"
mkdir -p "$t3_test_home/.local/bin" "$t3_test_home/.t3" "$t3_test_bin"
ln -s "$REPO_ROOT/dotfiles/t3code/.local/bin/t3ctl" "$t3_test_home/.local/bin/t3ctl"
printf 'OPENAI_API_KEY=kept\nT3CODE_PORT=4000\n' >"$t3_test_home/.t3/service.env"
: >"$t3_test_log"
cat >"$t3_test_bin/npm" <<'EOF'
#!/usr/bin/env bash
printf 'npm:%s\n' "$*" >>"$T3_TEST_LOG"
mkdir -p "$HOME/.local/bin"
printf '#!/usr/bin/env bash\nprintf "t3:%%s\\n" "$*" >>"$T3_TEST_LOG"\n' >"$HOME/.local/bin/t3"
chmod +x "$HOME/.local/bin/t3"
EOF
cat >"$t3_test_bin/systemctl" <<'EOF'
#!/usr/bin/env bash
printf 'systemctl:%s\n' "$*" >>"$T3_TEST_LOG"
case "$*" in
    *is-active*) [ -z "${T3_TEST_INACTIVE:-}" ] ;;
esac
EOF
cat >"$t3_test_bin/loginctl" <<'EOF'
#!/usr/bin/env bash
case "$1" in
    show-user) printf 'no\n' ;;
    *) printf 'loginctl:%s\n' "$*" >>"$T3_TEST_LOG" ;;
esac
EOF
# The server writes its runtime file once it is ready.
mkdir -p "$t3_test_home/.t3/userdata"
: >"$t3_test_home/.t3/userdata/server-runtime.json"
printf '#!/usr/bin/env bash\n' >"$t3_test_bin/curl"
printf '#!/usr/bin/env bash\nprintf "sudo:%%s\\n" "$*" >>"$T3_TEST_LOG"\n' >"$t3_test_bin/sudo"
chmod +x "$t3_test_bin"/*
(
    # Earlier checks leave shell functions with these names behind.
    unset -f npm systemctl loginctl sudo curl
    export HOME="$t3_test_home" PATH="$t3_test_bin:/usr/bin:/bin" T3_TEST_LOG="$t3_test_log"
    MYCONFIG_PROFILE=cachyos
    module_t3code
)
grep -Fxq "npm:install -g --prefix $t3_test_home/.local t3" "$t3_test_log" \
    || myconfig_fail "T3 Code module did not install t3 from npm into ~/.local"
grep -Fxq "loginctl:enable-linger $(id -un)" "$t3_test_log" \
    || myconfig_fail "T3 Code module did not keep the service running at boot"
grep -Fxq 't3:service install' "$t3_test_log" \
    || myconfig_fail "T3 Code module did not install the T3 Code service"
grep -Fxq 'systemctl:--user restart t3code.service' "$t3_test_log" \
    || myconfig_fail "T3 Code module did not restart the service to apply service.env"
[ "$(sort "$t3_test_home/.t3/service.env")" = "$(printf 'OPENAI_API_KEY=kept\nT3CODE_HOST=127.0.0.1\nT3CODE_PORT=3773')" ] \
    || myconfig_fail "T3 Code service.env does not bind 127.0.0.1:3773 while keeping other settings"
if (
    # Earlier checks leave shell functions with these names behind.
    unset -f npm systemctl loginctl sudo curl
    export HOME="$t3_test_home" PATH="$t3_test_bin:/usr/bin:/bin" T3_TEST_LOG="$t3_test_log" T3_TEST_INACTIVE=1
    MYCONFIG_PROFILE=cachyos
    module_t3code
) >/dev/null 2>&1; then
    myconfig_fail "T3 Code module succeeded without an active service"
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
for profile_source in "$cachyos_profile" "$arch_wsl_profile"; do
    [[ "$profile_source" == *module_ssh_server* ]] \
        || myconfig_fail "a profile does not offer to turn on the OpenSSH server"
done
[[ "$cachyos_profile" == *module_axidev_osk* ]] \
    || myconfig_fail "CachyOS profile does not include Axidev OSK"
[[ "$cachyos_profile" == *module_emacs* ]] \
    || myconfig_fail "CachyOS profile does not include Emacs"
[[ "$cachyos_profile" != *module_neovim* ]] \
    || myconfig_fail "CachyOS profile still includes Neovim"
[[ "$cachyos_profile" != *module_tmux* ]] \
    || myconfig_fail "CachyOS profile still includes tmux"
[[ "$cachyos_profile" == *module_ghostty* ]] \
    || myconfig_fail "CachyOS profile does not include Ghostty"
[[ "$cachyos_profile" == *module_kde_plasma* ]] \
    || myconfig_fail "CachyOS profile does not include KDE Plasma configuration"
[[ "$cachyos_profile" == *module_refind* ]] \
    || myconfig_fail "CachyOS profile does not include the rEFInd theme"
[[ "$cachyos_profile" == *module_kanata* ]] \
    || myconfig_fail "CachyOS profile does not include Kanata"
[[ "$cachyos_profile" == *module_kanata_kde* ]] \
    || myconfig_fail "CachyOS profile does not include the Kanata KDE tray"
[[ "$cachyos_profile" == *module_handy* ]] \
    || myconfig_fail "CachyOS profile does not include Handy"
for profile_source in "$cachyos_profile" "$arch_wsl_profile"; do
    [[ "$profile_source" == *module_t3code* ]] \
        || myconfig_fail "a Linux workstation profile does not include the T3 Code server"
done
[[ "$ubuntu_profile" != *module_t3code* ]] \
    || myconfig_fail "Ubuntu Server profile includes the T3 Code server"
for t3_profile in cachyos arch-wsl; do
    MYCONFIG_PROFILE="$t3_profile" dotfile_packages_for_profile | grep -Fxq t3code \
        || myconfig_fail "$t3_profile does not deploy the t3code package"
done
for app_module in module_flatpak module_appimage; do
    [[ "$cachyos_profile" == *"$app_module"* ]] \
        || myconfig_fail "CachyOS profile does not include $app_module"
    [[ "$arch_wsl_profile" != *"$app_module"* ]] \
        || myconfig_fail "Arch WSL profile includes $app_module"
done
[[ "$arch_wsl_profile" != *module_axidev_osk* ]] \
    || myconfig_fail "Arch WSL profile includes Axidev OSK"
[[ "$arch_wsl_profile" != *module_ghostty* ]] \
    || myconfig_fail "Arch WSL profile includes Ghostty"
[[ "$arch_wsl_profile" != *module_tmux* ]] \
    || myconfig_fail "Arch WSL profile still includes tmux"
[[ "$arch_wsl_profile" != *module_emacs* ]] \
    || myconfig_fail "Arch WSL profile includes Emacs"
[[ "$arch_wsl_profile" != *module_neovim* ]] \
    || myconfig_fail "Arch WSL profile still includes Neovim"
[[ "$arch_wsl_profile" != *module_kde_plasma* ]] \
    || myconfig_fail "Arch WSL profile includes KDE Plasma configuration"
[[ "$arch_wsl_profile" != *module_refind* ]] \
    || myconfig_fail "Arch WSL profile includes the rEFInd configuration"
[[ "$arch_wsl_profile" != *module_kanata* ]] \
    || myconfig_fail "Arch WSL profile includes Kanata"
[[ "$arch_wsl_profile" != *module_kanata_kde* ]] \
    || myconfig_fail "Arch WSL profile includes the Kanata KDE tray"
[[ "$arch_wsl_profile" != *module_handy* ]] \
    || myconfig_fail "Arch WSL profile includes Handy"
[[ "$ubuntu_profile" != *module_axidev_osk* ]] \
    || myconfig_fail "Ubuntu Server profile includes Axidev OSK"
[[ "$ubuntu_profile" != *module_ghostty* ]] \
    || myconfig_fail "Ubuntu Server profile includes Ghostty"
[[ "$ubuntu_profile" != *module_kde_plasma* ]] \
    || myconfig_fail "Ubuntu Server profile includes KDE Plasma configuration"
[[ "$ubuntu_profile" != *module_refind* ]] \
    || myconfig_fail "Ubuntu Server profile includes the rEFInd configuration"
[[ "$ubuntu_profile" != *module_kanata* ]] \
    || myconfig_fail "Ubuntu Server profile includes Kanata"
[[ "$ubuntu_profile" != *module_kanata_kde* ]] \
    || myconfig_fail "Ubuntu Server profile includes the Kanata KDE tray"
[[ "$ubuntu_profile" != *module_handy* ]] \
    || myconfig_fail "Ubuntu Server profile includes Handy"

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
(
    HOME="$TEST_HOME/agent-package-requests"
    MYCONFIG_PROFILE=arch-wsl
    mkdir -p "$HOME/.bun/install/global/node_modules/.bin"
    printf '#!/bin/sh\nexit 0\n' >"$HOME/.bun/install/global/node_modules/.bin/playwright"
    chmod +x "$HOME/.bun/install/global/node_modules/.bin/playwright"
    install_package_ids() { printf '%s\n' "$@" >>"$HOME/requests"; }
    bun() {
        printf '%s\n' "$*" >>"$HOME/bun-requests"
        mkdir -p "$HOME/.bun/bin" && printf '#!/bin/sh\n' >"$HOME/.bun/bin/pi" && chmod +x "$HOME/.bun/bin/pi"
    }
    curl() {
        printf '%s\n' "$*" >>"$HOME/downloads"
        printf '%s\n' 'mkdir -p "$HOME/.local/bin" && printf "#!/bin/sh\n" >"$HOME/.local/bin/claude" && chmod +x "$HOME/.local/bin/claude"'
    }
    module_agents_packages
    module_agents_packages
    [ "$(grep -c 'https://claude.ai/install.sh' "$HOME/downloads")" = 1 ] \
        || myconfig_fail "Agent module did not install Claude Code exactly once"
    [ -x "$HOME/.local/bin/claude" ] \
        || myconfig_fail "Agent module did not leave Claude Code in ~/.local/bin"
    grep -Fq '@mariozechner/pi-coding-agent@latest' "$HOME/bun-requests" \
        || myconfig_fail "Agent module does not install Pi"
    if grep -Exq 'opencode|fx_agent' "$HOME/requests"; then
        myconfig_fail "Agent module still installs OpenCode or FX"
    fi
    grep -Fxq lsof "$HOME/requests" && grep -Fxq wsl_ssh_agent "$HOME/requests" \
        || myconfig_fail "Agent module lost its remaining platform dependencies"
)
agents_packages_source="$(declare -f module_agents_packages)"
[[ "$agents_packages_source" == *'packages+=(ydotool)'* ]] \
    || myconfig_fail "CachyOS agent module does not request ydotool"
agents_configure_source="$(declare -f module_agents_configure)"
[[ "$agents_configure_source" == *'systemctl --user enable ydotool.service'* ]] \
    || myconfig_fail "CachyOS agent module does not enable ydotoold"
[[ -d "$REPO_ROOT/dotfiles/ai/.config/opencode" ]] \
    || myconfig_fail "AI dotfile package does not own the OpenCode configuration"
[[ ! -e "$REPO_ROOT/dotfiles/opencode" ]] \
    || myconfig_fail "OpenCode remains a separate dotfile package"
agents_home="$TEST_HOME/agents-home"
mkdir -p "$agents_home/.agents/skills/demo" "$agents_home/.claude/skills" "$agents_home/.config/opencode"
printf '%s\n' '# Test global instructions' >"$agents_home/.agents/AGENTS.md"
ln -s "../../.agents/skills/stale" "$agents_home/.claude/skills/stale"
HOME="$agents_home"
link_agent_config
[[ "$(readlink "$HOME/.config/opencode/AGENTS.md")" == "$HOME/.agents/AGENTS.md" ]] \
    || myconfig_fail "OpenCode AGENTS bridge does not use the live absolute target"
[[ "$(readlink "$HOME/.fx/AGENTS.md")" == "$HOME/.agents/AGENTS.md" ]] \
    || myconfig_fail "fx AGENTS bridge does not use the live absolute target"
[[ "$(readlink -f "$HOME/.pi/agent/AGENTS.md")" == "$(readlink -f "$HOME/.agents/AGENTS.md")" ]] \
    || myconfig_fail "Pi does not receive the shared global instructions"
cat >"$HOME/.fx/mcp.json" <<'EOF'
{
  "mcp": {
    "existing-server": {"type": "http", "url": "https://example.test/mcp"}
  }
}
EOF
BUN_INSTALL="$HOME/.bun"
configure_fx_playwright_mcp
jq -e --arg command "$HOME/.bun/bin/playwright-mcp" '
    .mcp["existing-server"].url == "https://example.test/mcp"
    and .mcp.playwright.command == [$command, "--headless"]
    and .mcp.playwright.type == "stdio"
    and .mcp.playwright.enabled == true
' "$HOME/.fx/mcp.json" >/dev/null \
    || myconfig_fail "fx MCP configuration did not preserve existing servers and add Playwright"
[[ "$(stat -c %a "$HOME/.fx/mcp.json")" = 600 ]] \
    || myconfig_fail "fx MCP configuration is not private"
[[ -L "$HOME/.claude/skills/demo" ]] \
    || myconfig_fail "Claude skill bridge was not created"
[[ ! -L "$HOME/.claude/skills/stale" ]] \
    || myconfig_fail "stale shared Claude skill bridge was not removed"
inventory_home="$TEST_HOME/inventory-home"
mkdir -p "$inventory_home"
HOME="$inventory_home"
MYCONFIG_PROFILE=cachyos
write_environment_inventory
grep -Fxq -- '- **Terminal tools**: Yazi, ripgrep, jq, and btop.' "$HOME/environment.md" \
    || myconfig_fail "CachyOS inventory still advertises removed terminal tools"
grep -Fq 'http://HOSTNAME.local:18080' "$HOME/environment.md" \
    || myconfig_fail "CachyOS inventory omitted the Emacs workbench"
grep -Fq 'Axidev OSK with desktop and login-screen startup' "$HOME/environment.md" \
    || myconfig_fail "CachyOS inventory omitted Axidev OSK"
grep -Fq 'Glassy Black & Pink theme, panels and application dock for KDE Plasma 6.7 through 6.x' "$HOME/environment.md" \
    || myconfig_fail "CachyOS inventory omitted KDE Plasma configuration"
grep -Fq 'Kanata keyboard remapping with a KDE tray profile selector' "$HOME/environment.md" \
    || myconfig_fail "CachyOS inventory omitted Kanata"
grep -Fq 'Handy offline push-to-talk dictation on Ctrl+Space' "$HOME/environment.md" \
    || myconfig_fail "CachyOS inventory omitted Handy"
grep -Fq 'ydotool with a persistent user service' "$HOME/environment.md" \
    || myconfig_fail "CachyOS inventory omitted ydotool"

printf 'Curated inventory\n' >"$HOME/environment.md"
write_environment_inventory
[ "$(cat "$HOME/environment.md")" = 'Curated inventory' ] \
    || myconfig_fail "Inventory generation overwrote an existing environment.md"

rm "$HOME/environment.md"
MYCONFIG_PROFILE=arch-wsl
write_environment_inventory
grep -Fxq -- '- **Terminal tools**: Yazi, ripgrep, jq, and btop.' "$HOME/environment.md" \
    || myconfig_fail "Arch WSL inventory still advertises removed terminal tools"
grep -Fq 'This profile does not install an editor.' "$HOME/environment.md" \
    || myconfig_fail "Arch WSL inventory still advertises an installed editor"
if grep -Fq 'Axidev OSK' "$HOME/environment.md"; then
    myconfig_fail "Arch WSL inventory included Axidev OSK"
fi
if grep -Fq 'KDE Plasma' "$HOME/environment.md"; then
    myconfig_fail "Arch WSL inventory included KDE Plasma configuration"
fi
if grep -Fq 'Kanata' "$HOME/environment.md"; then
    myconfig_fail "Arch WSL inventory included Kanata"
fi
if grep -Fq 'Handy' "$HOME/environment.md"; then
    myconfig_fail "Arch WSL inventory included Handy"
fi
if grep -Fq 'ydotool' "$HOME/environment.md"; then
    myconfig_fail "Arch WSL inventory included ydotool"
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

auth_output="$(MYCONFIG_TTY_PATH="$TEST_HOME/no-terminal" offer_action "Test is off." "Turn it on?" "test start" false)"
[[ "$auth_output" == *"Test is off. Run: test start"* ]] \
    || myconfig_fail "noninteractive step instructions were not printed"

ssh_test_log="$TEST_HOME/ssh-server.log"
ssh_output="$(
    source "$REPO_ROOT/linux/modules/ssh.sh"
    systemctl() {
        printf 'systemctl:%s\n' "$*" >>"$ssh_test_log"
        return 3
    }
    sudo() { printf 'sudo:%s\n' "$*" >>"$ssh_test_log"; }
    MYCONFIG_TTY_PATH="$TEST_HOME/no-terminal" module_ssh_server
)"
[[ "$ssh_output" == *"The OpenSSH server is off. Run: sudo systemctl enable --now sshd.service"* ]] \
    || myconfig_fail "stopped OpenSSH server without a terminal did not print how to turn it on"
if grep -Fq 'sudo:' "$ssh_test_log"; then
    myconfig_fail "stopped OpenSSH server was changed without being asked"
fi

ssh_output="$(
    source "$REPO_ROOT/linux/modules/ssh.sh"
    sudo() { return 1; }
    ssh_start_server
    printf 'install continued\n'
)"
[[ "$ssh_output" == *"WARNING: the OpenSSH server did not start"* ]] \
    || myconfig_fail "failed OpenSSH start did not warn"
[[ "$ssh_output" == *"install continued"* ]] \
    || myconfig_fail "failed OpenSSH start stopped the install"

: >"$ssh_test_log"
(
    source "$REPO_ROOT/linux/modules/ssh.sh"
    systemctl() { return 0; }
    sudo() { printf 'sudo:%s\n' "$*" >>"$ssh_test_log"; }
    offer_action() { myconfig_fail "running OpenSSH server asked before restarting"; }
    module_ssh_server >/dev/null
)
grep -Fxq 'sudo:systemctl restart sshd.service' "$ssh_test_log" \
    || myconfig_fail "running OpenSSH server did not restart with its new configuration"

printf 'Linux installer tests passed.\n'
