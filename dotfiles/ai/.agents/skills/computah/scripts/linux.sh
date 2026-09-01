#!/usr/bin/env bash
set -euo pipefail

session=${XDG_SESSION_TYPE:-unknown}
desktop=${XDG_CURRENT_DESKTOP:-unknown}

has() { command -v "$1" >/dev/null 2>&1; }

capture_backend() {
  if [[ $session == wayland && $desktop == *KDE* ]] && has spectacle; then
    printf '%s' spectacle
  elif [[ $session == wayland ]] && has grim; then
    printf '%s' grim
  elif [[ $session == x11 ]] && has spectacle; then
    printf '%s' spectacle
  elif [[ $session == x11 ]] && has gnome-screenshot; then
    printf '%s' gnome-screenshot
  elif [[ $session == x11 ]] && has scrot; then
    printf '%s' scrot
  fi
}

keyboard_backend() {
  if [[ $session == wayland ]] && has wtype; then
    printf '%s' wtype
  elif [[ $session == x11 ]] && has xdotool; then
    printf '%s' xdotool
  fi
}

pointer_backend() {
  local displays
  displays=$(display_count)
  if has ydotool && (( displays <= 1 )); then
    printf '%s' ydotool
  elif [[ $session == x11 ]] && has xdotool; then
    printf '%s' xdotool
  fi
}

scroll_backend() {
  if [[ $session == x11 ]] && has xdotool; then
    printf '%s' xdotool
  fi
}

display_count() {
  local count=0
  if has kscreen-doctor; then
    count=$(kscreen-doctor -o 2>/dev/null | grep -c 'Output:' || true)
  elif [[ $session == x11 ]] && has xrandr; then
    count=$(xrandr --listmonitors 2>/dev/null | grep -c '^ [0-9]:' || true)
  fi
  printf '%s' "$count"
}

json_string() {
  local value=${1//\\/\\\\}
  value=${value//\"/\\\"}
  printf '"%s"' "$value"
}

backend_json() {
  if [[ -n $1 ]]; then json_string "$1"; else printf 'null'; fi
}

capabilities() {
  local capture keyboard pointer scroll crop displays
  crop=''
  capture=$(capture_backend)
  keyboard=$(keyboard_backend)
  pointer=$(pointer_backend)
  scroll=$(scroll_backend)
  displays=$(display_count)
  has magick && crop=magick
  printf '{"platform":"linux","session":%s,"desktop":%s,"displays":%s,"capture":%s,"crop":%s,"keyboard":%s,"pointer":%s,"scroll":%s}\n' \
    "$(json_string "$session")" "$(json_string "$desktop")" \
    "$displays" \
    "$(backend_json "$capture")" "$(backend_json "$crop")" \
    "$(backend_json "$keyboard")" "$(backend_json "$pointer")" \
    "$(backend_json "$scroll")"
}

absolute_output() {
  local output=$1
  output=$(realpath -m "$output")
  mkdir -p "$(dirname "$output")"
  printf '%s' "$output"
}

capture() {
  local backend output
  backend=$(capture_backend)
  [[ -n $backend ]] || { printf 'computah: no screenshot backend for %s/%s\n' "$session" "$desktop" >&2; exit 2; }
  output=$(absolute_output "${1:-${TMPDIR:-/tmp}/computah/shot.png}")
  case $backend in
    spectacle) spectacle --background --nonotify --fullscreen --output "$output" ;;
    grim) grim "$output" ;;
    gnome-screenshot) gnome-screenshot --file="$output" ;;
    scrot) scrot "$output" ;;
  esac
  [[ -s $output ]] || { printf 'computah: screenshot is empty\n' >&2; exit 2; }
  printf '%s\n' "$output"
}

crop() {
  [[ $# -eq 6 ]] || { printf 'computah: crop requires INPUT OUTPUT X Y WIDTH HEIGHT\n' >&2; exit 2; }
  has magick || { printf 'computah: crop requires ImageMagick\n' >&2; exit 2; }
  local input=$1 output=$2 x=$3 y=$4 width=$5 height=$6
  output=$(absolute_output "$output")
  magick "$input" -crop "${width}x${height}+${x}+${y}" +repage "$output"
  printf '%s\n' "$output"
}

wtype_key() {
  local chord=${1^^} part key
  local -a parts press release
  IFS=+ read -r -a parts <<< "$chord"
  key=${parts[-1]}
  unset 'parts[-1]'
  for part in "${parts[@]}"; do
    case $part in
      CTRL|CONTROL) part=ctrl ;;
      ALT) part=alt ;;
      SHIFT) part='shift' ;;
      META|WIN|SUPER) part=logo ;;
      *) printf 'computah: unknown modifier: %s\n' "$part" >&2; exit 2 ;;
    esac
    press+=(-M "$part")
    release=(-m "$part" "${release[@]}")
  done
  case $key in
    ENTER|RETURN) key=Return ;;
    ESC|ESCAPE) key=Escape ;;
    BACKSPACE) key=BackSpace ;;
    META|WIN|SUPER) key=Super_L ;;
    CTRL|CONTROL) key=Control_L ;;
    ALT) key=Alt_L ;;
    SHIFT) key=Shift_L ;;
    SPACE) key=space ;;
    LEFT|RIGHT|UP|DOWN|HOME|END|TAB|DELETE) key=${key,,}; key=${key^} ;;
  esac
  wtype "${press[@]}" -k "$key" "${release[@]}"
}

require_keyboard() {
  local backend
  backend=$(keyboard_backend)
  [[ -n $backend ]] || { printf 'computah: no keyboard backend for %s/%s\n' "$session" "$desktop" >&2; exit 2; }
  printf '%s' "$backend"
}

require_pointer() {
  local backend
  backend=$(pointer_backend)
  [[ -n $backend ]] || { printf 'computah: no pointer backend for %s/%s\n' "$session" "$desktop" >&2; exit 2; }
  printf '%s' "$backend"
}

command=${1:-}
[[ -n $command ]] || { printf 'computah: missing command\n' >&2; exit 2; }
shift

case $command in
  capabilities) capabilities ;;
  capture) capture "$@" ;;
  crop) crop "$@" ;;
  type)
    [[ $# -eq 1 ]] || { printf 'computah: type requires one quoted TEXT argument\n' >&2; exit 2; }
    backend=$(require_keyboard)
    if [[ $backend == wtype ]]; then wtype "$1"; else xdotool type --clearmodifiers --delay 0 -- "$1"; fi
    ;;
  key)
    [[ $# -eq 1 ]] || { printf 'computah: key requires CHORD\n' >&2; exit 2; }
    backend=$(require_keyboard)
    if [[ $backend == wtype ]]; then wtype_key "$1"; else xdotool key --clearmodifiers "$1"; fi
    ;;
  move)
    [[ $# -eq 2 ]] || { printf 'computah: move requires X Y\n' >&2; exit 2; }
    backend=$(require_pointer)
    if [[ $backend == ydotool ]]; then ydotool mousemove --absolute "$1" "$2"; else xdotool mousemove --sync "$1" "$2"; fi
    ;;
  click)
    [[ $# -ge 2 && $# -le 3 ]] || { printf 'computah: click requires X Y [BUTTON]\n' >&2; exit 2; }
    backend=$(require_pointer)
    button=${3:-left}
    if [[ $backend == ydotool ]]; then
      case $button in left) button=0xC0;; right) button=0xC1;; middle) button=0xC2;; *) printf 'computah: unknown button\n' >&2; exit 2;; esac
      ydotool mousemove --absolute "$1" "$2"
      ydotool click "$button"
    else
      case $button in left) button=1;; middle) button=2;; right) button=3;; *) printf 'computah: unknown button\n' >&2; exit 2;; esac
      xdotool mousemove --sync "$1" "$2" click "$button"
    fi
    ;;
  drag)
    [[ $# -ge 4 && $# -le 5 ]] || { printf 'computah: drag requires X1 Y1 X2 Y2 [BUTTON]\n' >&2; exit 2; }
    backend=$(require_pointer)
    button=${5:-left}
    if [[ $backend == ydotool ]]; then
      case $button in left) down=0x40; up=0x80;; right) down=0x41; up=0x81;; middle) down=0x42; up=0x82;; *) printf 'computah: unknown button\n' >&2; exit 2;; esac
      ydotool mousemove --absolute "$1" "$2"
      ydotool click "$down"
      ydotool mousemove --absolute "$3" "$4"
      ydotool click "$up"
    else
      case $button in left) button=1;; middle) button=2;; right) button=3;; *) printf 'computah: unknown button\n' >&2; exit 2;; esac
      xdotool mousemove --sync "$1" "$2" mousedown "$button" mousemove --sync "$3" "$4" mouseup "$button"
    fi
    ;;
  scroll)
    [[ $# -eq 1 && $1 =~ ^-?[0-9]+$ ]] || { printf 'computah: scroll requires integer NOTCHES\n' >&2; exit 2; }
    backend=$(scroll_backend)
    [[ -n $backend ]] || { printf 'computah: no scroll backend for %s/%s\n' "$session" "$desktop" >&2; exit 2; }
    notches=$1
    button=4; (( notches < 0 )) && { button=5; notches=$(( -notches )); }
    (( notches == 0 )) || xdotool click --repeat "$notches" "$button"
    ;;
  *) printf 'computah: unknown command: %s\n' "$command" >&2; exit 2 ;;
esac
