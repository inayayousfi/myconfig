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
[ "$(resolve_package cachy_update)" = official:cachy-update ] \
    || myconfig_fail "Cachy Update package did not resolve"
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

(
    HOME="$TEST_HOME/tmux-module-home"
    MYCONFIG_PROFILE=cachyos
    mkdir -p "$HOME/.config/tmux/tmux-atelier"
    printf 'stale\n' >"$HOME/.config/tmux/tmux-atelier/stale"

    package_log="$TEST_HOME/tmux-packages.log"
    removal_log="$TEST_HOME/tmux-removals.log"
    installer_log="$TEST_HOME/tmux-installer.log"
    install_package_ids() {
        printf '%s\n' "$@" >>"$package_log"
    }
    remove_package_ids() {
        printf '%s\n' "$@" >>"$removal_log"
    }
    export installer_log
    curl() {
        printf '%s\n' "$*" >"$installer_log"
        printf '%s\n' \
            '#!/usr/bin/env bash' \
            'set -eu' \
            'test "$1" = --install-dir' \
            'destination=$2' \
            'rm -rf "$destination"' \
            'mkdir -p "$destination/bin"' \
            'printf "#!/usr/bin/env bash\\n" >"$destination/tmux-atelier.tmux"' \
            'printf "#!/usr/bin/env bash\\n" >"$destination/bin/tmux-atelier"' \
            'chmod +x "$destination/bin/tmux-atelier"'
    }

    source "$REPO_ROOT/linux/modules/tmux.sh"
    module_tmux

    [ ! -e "$TMUX_ATELIER_DIR/stale" ] \
        || myconfig_fail "tmux module preserved stale managed files"
    grep -Fxq tmux "$package_log" \
        || myconfig_fail "tmux module did not install tmux"
    grep -Fxq wl_clipboard "$package_log" \
        || myconfig_fail "CachyOS tmux module did not install wl-clipboard"
    grep -Fxq herdr "$removal_log" \
        || myconfig_fail "tmux module did not retire the legacy Herdr package"
    grep -Fq 'https://raw.githubusercontent.com/inayayousfi/tmux-atelier/main/install.sh' "$installer_log" \
        || myconfig_fail "tmux module did not fetch the official installer"

    : >"$package_log"
    MYCONFIG_PROFILE=arch-wsl
    module_tmux
    [ "$(cat "$package_log")" = tmux ] \
        || myconfig_fail "Arch WSL tmux module installed unexpected packages"

    printf 'working\n' >"$TMUX_ATELIER_DIR/working"
    curl() {
        return 1
    }
    if module_tmux >/dev/null 2>&1; then
        myconfig_fail "tmux module accepted a failed release installation"
    fi
    [ "$(cat "$TMUX_ATELIER_DIR/working")" = working ] \
        || myconfig_fail "failed tmux-atelier refresh removed the working installation"
)

clipboard="$REPO_ROOT/dotfiles/tmux/.local/bin/myconfig-tmux-clipboard"
clipboard_bin="$TEST_HOME/clipboard-bin"
clipboard_log="$TEST_HOME/clipboard.log"
mkdir -p "$clipboard_bin"
cat >"$clipboard_bin/wl-copy" <<'EOF'
#!/usr/bin/env bash
cat >"$CLIPBOARD_LOG"
EOF
cat >"$clipboard_bin/wl-paste" <<'EOF'
#!/usr/bin/env bash
printf 'wayland clipboard'
EOF
cat >"$clipboard_bin/clip.exe" <<'EOF'
#!/usr/bin/env bash
cat >"$CLIPBOARD_LOG"
EOF
cat >"$clipboard_bin/powershell.exe" <<'EOF'
#!/usr/bin/env bash
printf 'windows clipboard'
EOF
chmod +x "$clipboard_bin"/*

export CLIPBOARD_LOG="$clipboard_log"
printf 'wayland copy' | WAYLAND_DISPLAY=wayland-0 PATH="$clipboard_bin:$PATH" "$clipboard" copy
[ "$(cat "$clipboard_log")" = 'wayland copy' ] \
    || myconfig_fail "Wayland clipboard copy did not receive tmux selection"
[ "$(WAYLAND_DISPLAY=wayland-0 PATH="$clipboard_bin:$PATH" "$clipboard" paste)" = 'wayland clipboard' ] \
    || myconfig_fail "Wayland clipboard paste did not return host content"

printf 'windows copy' | WSL_DISTRO_NAME=Arch PATH="$clipboard_bin:$PATH" "$clipboard" copy
[ "$(cat "$clipboard_log")" = 'windows copy' ] \
    || myconfig_fail "WSL clipboard copy did not receive tmux selection"
[ "$(WSL_DISTRO_NAME=Arch PATH="$clipboard_bin:$PATH" "$clipboard" paste)" = 'windows clipboard' ] \
    || myconfig_fail "WSL clipboard paste did not return host content"

(
    tmux_home="$TEST_HOME/tmux-config-home"
    tmux_socket="myconfig-test-$$"
    mkdir -p "$tmux_home/.config/tmux/tmux-atelier"
    cat >"$tmux_home/.config/tmux/tmux-atelier/tmux-atelier.tmux" <<'EOF'
#!/usr/bin/env bash
tab_style="$(tmux show-options -gqv @atelier_tab_style)"
active_style="$(tmux show-options -gqv @atelier_tab_active_style)"
tmux set-option -g 'status-format[0]' \
    "#[align=left]#{W:#[range=window|#{window_index} $tab_style] #I #W #[norange default]│,#[range=window|#{window_index} $active_style] #I #W #[norange default]│}"
EOF
    chmod +x "$tmux_home/.config/tmux/tmux-atelier/tmux-atelier.tmux"
    trap 'tmux -L "$tmux_socket" kill-server >/dev/null 2>&1 || true' EXIT

    HOME="$tmux_home" tmux -L "$tmux_socket" \
        -f "$REPO_ROOT/dotfiles/tmux/.config/tmux/tmux.conf" \
        new-session -d -s validation
    [ "$(tmux -L "$tmux_socket" show-options -gv @atelier_workspace_active_style)" = \
        'fg=#000000#,bg=#ff4ead#,bold' ] \
        || myconfig_fail "tmux config did not apply the active Black & Pink style"
    expanded_tabs="$(tmux -L "$tmux_socket" display-message -p '#{E:status-format[0]}')"
    [[ "$expanded_tabs" == *'#[range=window|0 fg=#000000,bg=#ff4ead,bold]'* ]] \
        || myconfig_fail "tmux config produced a malformed expanded tab format"
    tmux -L "$tmux_socket" list-keys -T copy-mode-vi \
        | grep -Fq 'myconfig-tmux-clipboard copy' \
        || myconfig_fail "tmux config did not bind host clipboard copy"
    tmux -L "$tmux_socket" list-keys -T prefix \
        | grep -Fq 'myconfig-tmux-clipboard' \
        || myconfig_fail "tmux config did not bind host clipboard paste"
)

source "$REPO_ROOT/linux/modules/axidev-osk.sh"
source "$REPO_ROOT/linux/modules/ghostty.sh"
source "$REPO_ROOT/linux/modules/cursor-theme.sh"
source "$REPO_ROOT/linux/modules/refind.sh"
source "$REPO_ROOT/linux/modules/kde-plasma.sh"
source "$REPO_ROOT/linux/modules/kanata.sh"
source "$REPO_ROOT/linux/modules/kanata-kde.sh"
source "$REPO_ROOT/linux/modules/handy.sh"

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
grep -Fq '/etc/libinput/plugins/90-myconfig-pointer-sensitivity.lua' \
    "$REPO_ROOT/linux/modules/kde-plasma.sh" \
    || myconfig_fail "KDE Plasma module does not install the libinput plugin"
grep -Fq 'panel.lengthMode = "fit";' \
    "$REPO_ROOT/dotfiles/kde-plasma/.local/share/myconfig/kde-plasma/layout.js" \
    || myconfig_fail "KDE Plasma dock does not fit its content"
grep -Fq 'tasks.writeConfig("fill", false);' \
    "$REPO_ROOT/dotfiles/kde-plasma/.local/share/myconfig/kde-plasma/layout.js" \
    || myconfig_fail "KDE Plasma task manager still fills the dock"
node "$REPO_ROOT/test/test-kde-plasma-panels.js" \
    "$REPO_ROOT/dotfiles/kde-plasma/.local/share/kwin/scripts/myconfig-plasma-panels/contents/code/main.js"
node "$REPO_ROOT/test/test-kde-plasma-layout.js" \
    "$REPO_ROOT/dotfiles/kde-plasma/.local/share/myconfig/kde-plasma/layout.js"
node -e 'JSON.parse(require("node:fs").readFileSync(process.argv[1], "utf8"))' \
    "$REPO_ROOT/dotfiles/kde-plasma/.local/share/kwin/scripts/myconfig-plasma-panels/metadata.json"
plasma_plasmoid_root="$REPO_ROOT/dotfiles/kde-plasma/.local/share/plasma/plasmoids"
for widget in overview session power; do
    node -e 'const metadata = JSON.parse(require("node:fs").readFileSync(process.argv[1], "utf8")); if (metadata.KPlugin.Id !== process.argv[2]) process.exit(1)' \
        "$plasma_plasmoid_root/myconfig.$widget/metadata.json" "myconfig.$widget"
    [ -f "$plasma_plasmoid_root/myconfig.$widget/contents/ui/main.qml" ] \
        || myconfig_fail "MyConfig $widget Plasma widget has no QML entry point"
done
node -e '
const fs = require("node:fs");
const root = process.argv[1];
const overview = fs.readFileSync(`${root}/myconfig.overview/contents/ui/main.qml`, "utf8");
const session = fs.readFileSync(`${root}/myconfig.session/contents/ui/main.qml`, "utf8");
const power = fs.readFileSync(`${root}/myconfig.power/contents/ui/main.qml`, "utf8");
if (!overview.includes(`text: qsTr("Overview")`) || !overview.includes("font.pointSize: 14") || !overview.includes("Layout.minimumWidth: implicitWidth") || !overview.includes("Layout.fillHeight: true") || !overview.includes("invokeShortcut Overview") || !overview.includes("CanFillArea")) process.exit(1);
const ordered = (source, values) => values.every((value, index) => source.indexOf(value) >= 0 && (index === 0 || source.indexOf(values[index - 1]) < source.indexOf(value)));
if (!ordered(session, [`qsTr("Lock")`, `qsTr("Log Out")`, `qsTr("Switch User")`])) process.exit(1);
if (!session.includes("enabled: session.canSwitchUser")) process.exit(1);
if (!ordered(power, [`qsTr("Restart")`, `qsTr("Shut Down")`, `qsTr("Sleep")`, `qsTr("Hibernate")`])) process.exit(1);
if (session.includes("ToolTip") || power.includes("ToolTip")) process.exit(1);
if (!session.includes("popupType: QQC2.Popup.Window") || !power.includes("popupType: QQC2.Popup.Window")) process.exit(1);
' "$plasma_plasmoid_root"
plasma_theme_root="$REPO_ROOT/dotfiles/kde-plasma/.local/share/plasma/desktoptheme/blacknpink"
plasma_global_theme_root="$REPO_ROOT/dotfiles/kde-plasma/.local/share/plasma/look-and-feel/org.myconfig.blacknpink.desktop"
cursor_theme_root="$REPO_ROOT/linux/assets/cursors/blacknpink-crosshair"
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
grep -Fxq 'Inherits=breeze_cursors' "$cursor_theme_root/index.theme" \
    || myconfig_fail "Black & Pink Crosshair cursor theme does not inherit Breeze"
for cursor in default pointer progress text wait size_hor size_ver; do
    [ -s "$cursor_theme_root/cursors/$cursor" ] \
        || myconfig_fail "Black & Pink Crosshair cursor theme is missing $cursor"
done
for cursor in pointer grab grabbing move dnd-move dnd-copy text; do
    [ "$(readlink "$cursor_theme_root/cursors/$cursor")" = crosshair ] \
        || myconfig_fail "Black & Pink Crosshair cursor theme does not use Precision Select for $cursor"
done
node -e 'const metadata = JSON.parse(require("node:fs").readFileSync(process.argv[1], "utf8")); if (metadata.KPlugin.Id !== "blacknpink") process.exit(1)' \
    "$plasma_theme_root/metadata.json"
grep -Fxq 'FallbackTheme=default' "$plasma_theme_root/plasmarc" \
    || myconfig_fail "Black & Pink Plasma theme does not inherit Breeze"
node -e 'const metadata = JSON.parse(require("node:fs").readFileSync(process.argv[1], "utf8")); if (metadata.KPackageStructure !== "Plasma/LookAndFeel" || metadata.KPlugin.Id !== "org.myconfig.blacknpink.desktop") process.exit(1)' \
    "$plasma_global_theme_root/metadata.json"
grep -Fxq 'ColorScheme=BlackPink' "$plasma_global_theme_root/contents/defaults" \
    || myconfig_fail "Black & Pink global theme does not select its color scheme"
grep -Fxq 'name=blacknpink' "$plasma_global_theme_root/contents/defaults" \
    || myconfig_fail "Black & Pink global theme does not select its Plasma theme"
grep -Fxq 'cursorTheme=blacknpink-crosshair' "$plasma_global_theme_root/contents/defaults" \
    || myconfig_fail "Black & Pink global theme does not select its cursor theme"
grep -Fq 'id="thick-hint-right-margin" x="95" y="-56" width="4" height="4"' \
    "$plasma_theme_root/widgets/panel-background.svg" \
    || myconfig_fail "Black & Pink Plasma theme does not set a 4-pixel floating-panel trailing margin"
grep -Fq 'id="thick-hint-left-margin" x="95" y="-20" width="4" height="8"' \
    "$plasma_theme_root/widgets/panel-background.svg" \
    || myconfig_fail "Black & Pink Plasma theme changed the Breeze floating-panel leading margin"
grep -Fq 'id="hint-top-margin" x="20" y="10" width="4" height="4"' \
    "$plasma_theme_root/widgets/panel-background.svg" \
    || myconfig_fail "Black & Pink Plasma theme changed the top panel's vertical content margin"
grep -Fq 'id="hint-left-margin" x="0" y="30" width=".00000001" height="4"' \
    "$plasma_theme_root/widgets/panel-background.svg" \
    || myconfig_fail "Black & Pink Plasma theme does not extend the Overview control to the screen left"
resvg "$plasma_theme_root/widgets/panel-background.svg" \
    "$TEST_HOME/panel-background.png"
systemd-analyze verify \
    "$REPO_ROOT/dotfiles/kde-plasma/.config/systemd/user/myconfig-kde-plasma-layout.service"

MYCONFIG_PROFILE=arch-wsl
if module_kde_plasma >/dev/null 2>&1; then
    myconfig_fail "KDE Plasma module accepted a non-CachyOS profile"
fi

plasma_layout_test_root="$TEST_HOME/kde-plasma-layout"
plasma_layout_home="$plasma_layout_test_root/home"
plasma_layout_bin="$plasma_layout_test_root/bin"
mkdir -p \
    "$plasma_layout_home/.config" \
    "$plasma_layout_home/.local/bin" \
    "$plasma_layout_home/.local/share/myconfig/kde-plasma" \
    "$plasma_layout_bin"
cp "$REPO_ROOT/dotfiles/kde-plasma/.local/bin/myconfig-kde-plasma-layout" \
    "$plasma_layout_home/.local/bin/myconfig-kde-plasma-layout"
cp "$REPO_ROOT/dotfiles/kde-plasma/.local/share/myconfig/kde-plasma/layout.js" \
    "$plasma_layout_home/.local/share/myconfig/kde-plasma/layout.js"
printf 'default panel configuration\n' \
    >"$plasma_layout_home/.config/plasma-org.kde.plasma.desktop-appletsrc"

cat >"$plasma_layout_bin/qdbus6" <<'EOF'
#!/usr/bin/env bash
if [ "$#" -eq 2 ]; then
    exit "${MYCONFIG_PLASMA_DBUS_STATUS:-0}"
fi
printf '%s\n' "${MYCONFIG_PLASMA_EVALUATE_OUTPUT:-MYCONFIG_STATUS=ok:screens=2}"
EOF
chmod +x "$plasma_layout_bin/qdbus6"

HOME="$plasma_layout_home" \
    PATH="$plasma_layout_bin:/usr/bin:/bin" \
    "$plasma_layout_home/.local/bin/myconfig-kde-plasma-layout"
[ "$(<"$plasma_layout_home/.local/state/myconfig/kde-plasma-layout-version")" = 4 ] \
    || myconfig_fail "KDE Plasma layout did not record its version"
shopt -s nullglob
plasma_backups=("$plasma_layout_home/.config/plasma-org.kde.plasma.desktop-appletsrc.backup."*)
[ "${#plasma_backups[@]}" -eq 1 ] \
    || myconfig_fail "KDE Plasma layout did not back up its initial panel configuration once"
[ "$(<"${plasma_backups[0]}")" = 'default panel configuration' ] \
    || myconfig_fail "KDE Plasma panel backup changed its contents"

HOME="$plasma_layout_home" \
    PATH="$plasma_layout_bin:/usr/bin:/bin" \
    "$plasma_layout_home/.local/bin/myconfig-kde-plasma-layout"
plasma_backups=("$plasma_layout_home/.config/plasma-org.kde.plasma.desktop-appletsrc.backup."*)
[ "${#plasma_backups[@]}" -eq 1 ] \
    || myconfig_fail "KDE Plasma layout repeated its first-run backup"

if HOME="$plasma_layout_home" \
    PATH="$plasma_layout_bin:/usr/bin:/bin" \
    MYCONFIG_PLASMA_EVALUATE_OUTPUT='MYCONFIG_STATUS=missing:org.kde.plasma.icontasks' \
    "$plasma_layout_home/.local/bin/myconfig-kde-plasma-layout" >/dev/null 2>&1; then
    myconfig_fail "KDE Plasma layout accepted a missing required widget"
fi

layout_status=0
HOME="$plasma_layout_home" \
    PATH="$plasma_layout_bin:/usr/bin:/bin" \
    MYCONFIG_PLASMA_DBUS_STATUS=1 \
    "$plasma_layout_home/.local/bin/myconfig-kde-plasma-layout" || layout_status=$?
[ "$layout_status" -eq 75 ] \
    || myconfig_fail "KDE Plasma layout did not defer when Plasma was inactive"

mkdir -p \
    "$plasma_layout_home/.config/autostart" \
    "$plasma_layout_home/.config/systemd/user" \
    "$plasma_layout_home/.local/share/color-schemes" \
    "$plasma_layout_home/.local/share/plasma/desktoptheme/blacknpink/widgets" \
    "$plasma_layout_home/.local/share/plasma/look-and-feel" \
    "$plasma_layout_home/.local/share/plasma/plasmoids" \
    "$plasma_layout_home/.local/share/kwin/scripts/myconfig-plasma-panels/contents/code"
cp "$REPO_ROOT/dotfiles/kde-plasma/.config/autostart/myconfig-kde-plasma-layout.desktop" \
    "$plasma_layout_home/.config/autostart/myconfig-kde-plasma-layout.desktop"
cp "$REPO_ROOT/dotfiles/kde-plasma/.local/share/color-schemes/BlackPink.colors" \
    "$plasma_layout_home/.local/share/color-schemes/BlackPink.colors"
cp "$plasma_theme_root/metadata.json" \
    "$plasma_layout_home/.local/share/plasma/desktoptheme/blacknpink/metadata.json"
cp "$plasma_theme_root/plasmarc" \
    "$plasma_layout_home/.local/share/plasma/desktoptheme/blacknpink/plasmarc"
cp "$plasma_theme_root/widgets/panel-background.svg" \
    "$plasma_layout_home/.local/share/plasma/desktoptheme/blacknpink/widgets/panel-background.svg"
cp -a "$plasma_global_theme_root" \
    "$plasma_layout_home/.local/share/plasma/look-and-feel/org.myconfig.blacknpink.desktop"
for widget in overview session power; do
    cp -a "$plasma_plasmoid_root/myconfig.$widget" \
        "$plasma_layout_home/.local/share/plasma/plasmoids/myconfig.$widget"
done
cp "$REPO_ROOT/dotfiles/kde-plasma/.config/systemd/user/myconfig-kde-plasma-layout.service" \
    "$plasma_layout_home/.config/systemd/user/myconfig-kde-plasma-layout.service"
cp "$REPO_ROOT/dotfiles/kde-plasma/.local/share/kwin/scripts/myconfig-plasma-panels/metadata.json" \
    "$plasma_layout_home/.local/share/kwin/scripts/myconfig-plasma-panels/metadata.json"
cp "$REPO_ROOT/dotfiles/kde-plasma/.local/share/kwin/scripts/myconfig-plasma-panels/contents/code/main.js" \
    "$plasma_layout_home/.local/share/kwin/scripts/myconfig-plasma-panels/contents/code/main.js"

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
fc-match() {
    printf 'IosevkaNerdFont-Regular.ttf: Iosevka Nerd Font\n'
}
desktop-file-validate() {
    /usr/bin/desktop-file-validate "$@"
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
HOME="$plasma_layout_home" \
    PATH="$plasma_layout_bin:/usr/bin:/bin" \
    module_cursor_theme
HOME="$plasma_layout_home" \
    PATH="$plasma_layout_bin:/usr/bin:/bin" \
    module_kde_plasma
grep -Fxq 'packages:iosevka_font desktop_file_utils libinput' "$plasma_module_log" \
    || myconfig_fail "KDE Plasma module omitted its appearance dependencies"
grep -Fxq "sudo:install -Dm644 $pointer_plugin /etc/libinput/plugins/90-myconfig-pointer-sensitivity.lua" \
    "$plasma_module_log" \
    || myconfig_fail "KDE Plasma module did not install the libinput plugin"
grep -Fxq 'global-theme:--apply org.myconfig.blacknpink.desktop' "$plasma_module_log" \
    || myconfig_fail "KDE Plasma module did not apply its global theme headlessly"
grep -Fxq 'cursors:--size 32 blacknpink-crosshair' "$plasma_module_log" \
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
grep -Fxq 'config:--file kcminputrc --group Mouse --key cursorSize 32' "$plasma_module_log" \
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
grep -Fxq 'config:--file kwinrc --group Plugins --key myconfig-plasma-panelsEnabled true' "$plasma_module_log" \
    || myconfig_fail "KDE Plasma module did not enable MyConfig Plasma Panels"
grep -Fxq 'systemctl:--user daemon-reload' "$plasma_module_log" \
    || myconfig_fail "KDE Plasma module did not reload user services"
grep -Fxq 'qdbus:org.kde.KWin /KWin org.kde.KWin.reconfigure' "$plasma_module_log" \
    || myconfig_fail "KDE Plasma module did not reload active KWin configuration"
unset -f sudo

desktop-file-validate \
    "$REPO_ROOT/dotfiles/kde-plasma/.config/autostart/myconfig-kde-plasma-layout.desktop"

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
    || myconfig_fail "Ghostty module did not install Ghostty without removing other packages"

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
[[ "$cachyos_profile" == *module_ghostty* ]] \
    || myconfig_fail "CachyOS profile does not include Ghostty"
[[ "$cachyos_profile" == *module_kde_plasma* ]] \
    || myconfig_fail "CachyOS profile does not include KDE Plasma configuration"
[[ "$cachyos_profile" == *module_cursor_theme* ]] \
    || myconfig_fail "CachyOS profile does not include the cursor theme"
[[ "$cachyos_profile" == *module_refind* ]] \
    || myconfig_fail "CachyOS profile does not include the rEFInd theme"
[[ "$cachyos_profile" == *module_kanata* ]] \
    || myconfig_fail "CachyOS profile does not include Kanata"
[[ "$cachyos_profile" == *module_kanata_kde* ]] \
    || myconfig_fail "CachyOS profile does not include the Kanata KDE tray"
[[ "$cachyos_profile" == *module_handy* ]] \
    || myconfig_fail "CachyOS profile does not include Handy"
[[ "$arch_wsl_profile" != *module_axidev_osk* ]] \
    || myconfig_fail "Arch WSL profile includes Axidev OSK"
[[ "$arch_wsl_profile" != *module_ghostty* ]] \
    || myconfig_fail "Arch WSL profile includes Ghostty"
[[ "$arch_wsl_profile" != *module_kde_plasma* ]] \
    || myconfig_fail "Arch WSL profile includes KDE Plasma configuration"
[[ "$arch_wsl_profile" != *module_cursor_theme* ]] \
    || myconfig_fail "Arch WSL profile includes the desktop cursor theme"
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
[[ "$ubuntu_profile" != *module_cursor_theme* ]] \
    || myconfig_fail "Ubuntu Server profile includes the desktop cursor theme"
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
agents_packages_source="$(declare -f module_agents_packages)"
[[ "$agents_packages_source" == *'packages+=(ydotool)'* ]] \
    || myconfig_fail "CachyOS agent module does not request ydotool"
agents_configure_source="$(declare -f module_agents_configure)"
[[ "$agents_configure_source" == *'systemctl --user enable ydotool.service'* ]] \
    || myconfig_fail "CachyOS agent module does not enable ydotoold"
inventory_home="$TEST_HOME/inventory-home"
mkdir -p "$inventory_home"
HOME="$inventory_home"
MYCONFIG_PROFILE=cachyos
write_environment_inventory
grep -Fq 'Ghostty, with Kitty, Alacritty, WezTerm, and Konsole removed' "$HOME/environment.md" \
    || myconfig_fail "CachyOS inventory omitted the Ghostty terminal policy"
grep -Fq 'Axidev OSK with desktop and login-screen startup' "$HOME/environment.md" \
    || myconfig_fail "CachyOS inventory omitted Axidev OSK"
grep -Fq 'Black & Pink panels and application dock for KDE Plasma 6.7 through 6.x' "$HOME/environment.md" \
    || myconfig_fail "CachyOS inventory omitted KDE Plasma configuration"
grep -Fq 'Kanata keyboard remapping with a KDE tray profile selector' "$HOME/environment.md" \
    || myconfig_fail "CachyOS inventory omitted Kanata"
grep -Fq 'Handy offline push-to-talk dictation on Ctrl+Space' "$HOME/environment.md" \
    || myconfig_fail "CachyOS inventory omitted Handy"
grep -Fq 'ydotool with a persistent user service' "$HOME/environment.md" \
    || myconfig_fail "CachyOS inventory omitted ydotool"

MYCONFIG_PROFILE=arch-wsl
write_environment_inventory
if grep -Fq 'Ghostty' "$HOME/environment.md"; then
    myconfig_fail "Arch WSL inventory included Ghostty"
fi
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

source "$REPO_ROOT/linux/modules/authentication.sh"
auth_output="$(offer_authentication Test "test login" false)"
[[ "$auth_output" == *"Test is not authenticated. Run: test login"* ]] \
    || myconfig_fail "noninteractive authentication instructions were not printed"

printf 'Linux installer tests passed.\n'
