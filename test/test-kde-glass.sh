#!/usr/bin/env bash
set -euo pipefail

MYCONFIG_REPO_ROOT="$(git rev-parse --show-toplevel)"
source "$MYCONFIG_REPO_ROOT/linux/modules/kde-plasma.sh"
test_root="$(mktemp -d)"
trap 'rm -rf "$test_root"' EXIT
data_dir="$test_root/data"
mkdir "$data_dir"
log="$test_root/calls"
host_kwin='kwin 6.7.4-test'
package_version="$(bash -c 'source "$1"; printf "%s %s-%s" "$pkgname" "$pkgver" "$pkgrel"' _ "$MYCONFIG_REPO_ROOT/linux/assets/kde-glass/PKGBUILD")"
installed_package="$package_version"
effect_id=myconfig_glass_fixture
printf '%s\n' "$effect_id" > "$data_dir/effect-id"
printf '%s\n' "$host_kwin" > "$data_dir/kwin-version"

myconfig_fail() { printf '%s\n' "$*" >&2; return 1; }
myconfig_log() { printf '%s\n' "$*"; }
require_command() { command -v "$1" >/dev/null; }
# sudo is also mocked below; no package manager is invoked by this test.
# shellcheck disable=SC2032
pacman() {
    case "$*" in
        '-Q kwin') printf '%s\n' "$host_kwin" ;;
        '-Q myconfig-kde-glass') [[ -n "$installed_package" ]] && printf '%s\n' "$installed_package" ;;
        *) return 1 ;;
    esac
}
kwriteconfig6() { printf 'config:%s\n' "$*" >> "$log"; }
makepkg() {
    printf 'build:%s\n' "$*" >> "$log"
    cmp PKGBUILD "$MYCONFIG_REPO_ROOT/linux/assets/kde-glass/PKGBUILD"
    touch myconfig-kde-glass-test.pkg.tar.zst
}
sudo() {
    [[ "$1 $2" == 'pacman -U' ]]
    printf 'install:%s\n' "$*" >> "$log"
    installed_package="$package_version"
    printf '%s\n' "$host_kwin" > "$data_dir/kwin-version"
    printf '%s\n' "$effect_id" > "$data_dir/effect-id"
}

install_kde_plasma_glass "$data_dir"
grep -q '^build:' "$log" && exit 1
for expected in \
    '--group Effect-blurplus --key PhysicallyBasedRefraction true' \
    '--group Effect-blurplus --key IgnoreContentBlurRegion false' \
    '--group Effect-blurplus --key Saturation 1' \
    '--group Effect-blurplus --key FrostedIOR 1.50' \
    '--group Effect-blurplus --key FrostedRoughness 0.45' \
    '--group Effect-blurplus --key FrostedThickness 80' \
    '--group Effect-blurplus --key FrostedInteriorShadow 0.30' \
    '--group Effect-blurplus --key FrostedCurvature 1.25' \
    '--group Effect-blurplus --key RefractionExcludeOSD false' \
    '--group Plugins --key blurEnabled false' \
    '--group Plugins --key glassEnabled false' \
    '--group Plugins --key myconfig_glass_fixtureEnabled true'; do
    grep -Fq -- "$expected" "$log" || { printf 'Missing setting: %s\n' "$expected"; exit 1; }
done

installed_package=''
install_kde_plasma_glass "$data_dir"
grep -q '^build:--syncdeps --noconfirm' "$log"
grep -q '^install:pacman -U' "$log"
: > "$log"
host_kwin='kwin 6.7.5-test'
install_kde_plasma_glass "$data_dir"
grep -q '^build:' "$log"
[[ "$(<"$data_dir/kwin-version")" == "$host_kwin" ]]

session_active=true
loaded_effects=$'blur\nmyconfig_glass_old\nother_effect'
load_succeeds=true
shaders_valid=true
qdbus6() {
    printf 'dbus:%s\n' "$*" >> "$log"
    [[ "$2" != /KWin ]] || { "$session_active"; return; }
    case "$3" in
        *.loadedEffects) printf '%s\n' "$loaded_effects" ;;
        *.loadEffect) [[ "$4" == blur ]] && printf 'true\n' || printf '%s\n' "$load_succeeds" ;;
        *.debug) "$shaders_valid" && printf 'valid=1 shaders=1 draws=0\n' || printf 'valid=0 shaders=0\n' ;;
    esac
}
: > "$log"
activate_kde_plasma_glass "$data_dir"
grep -q 'loadEffect myconfig_glass_fixture' "$log"
grep -q 'unloadEffect blur' "$log"
grep -q 'unloadEffect myconfig_glass_old' "$log"
grep -q 'unloadEffect other_effect' "$log" && exit 1
unload_line="$(grep -n 'Effects.unloadEffect myconfig_glass_old$' "$log" | cut -d: -f1)"
load_line="$(grep -n 'Effects.loadEffect myconfig_glass_fixture$' "$log" | cut -d: -f1)"
[[ "$unload_line" -lt "$load_line" ]]
: > "$log"
loaded_effects=$'myconfig_glass_fixture\nother_effect'
activate_kde_plasma_glass "$data_dir"
grep -q 'reconfigureEffect myconfig_glass_fixture' "$log"
grep -q 'Effects.unloadEffect myconfig_glass_fixture' "$log"
grep -q 'Effects.loadEffect myconfig_glass_fixture' "$log"

for failure in load shader; do
    : > "$log"
    loaded_effects=$'blur\nmyconfig_glass_old\nother_effect'
    load_succeeds=true
    shaders_valid=true
    if [[ "$failure" == load ]]; then load_succeeds=false; else shaders_valid=false; fi
    if activate_kde_plasma_glass "$data_dir" 2>/dev/null; then exit 1; fi
    grep -q 'Effects.loadEffect blur' "$log"
    grep -q 'unloadEffect other_effect' "$log" && exit 1
    grep -q 'blurEnabled true' "$log"
done
: > "$log"
session_active=false
activate_kde_plasma_glass "$data_dir"
grep -q '/Effects' "$log" && exit 1
printf '%s\n' 'Glass install, rebuild, activation, failure, and offline tests passed.'
