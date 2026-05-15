#!/bin/sh
set -eu

LOG="/tmp/sketchybar_click_debug.log"
{
  echo "---- $(date '+%F %T') ----"
  echo "SWITCH_SPACE invoked"
  echo "ARGV: $*"
  echo "NAME=${NAME:-} SID=${SID:-} SENDER=${SENDER:-} BUTTON=${BUTTON:-} MODIFIER=${MODIFIER:-}"
  echo "PWD=$(pwd)"
  echo "PATH=$PATH"
} >>"$LOG"

LOG="/tmp/sketchybar_switch_space.log"
CURRENT_FILE="/tmp/sketchybar_current_space"

target="${1:-}"

echo "---- $(date '+%F %T') ----" >>"$LOG"
echo "target=$target name=${NAME:-} sender=${SENDER:-} sid=${SID:-} selected=${SELECTED:-}" >>"$LOG"

# Validate target integer >= 1
case "$target" in
  ''|*[!0-9]*) echo "bad target" >>"$LOG"; exit 2 ;;
esac
if [ "$target" -le 0 ]; then
  echo "target <= 0" >>"$LOG"
  exit 2
fi

if [ ! -f "$CURRENT_FILE" ]; then
  echo "current file missing: $CURRENT_FILE" >>"$LOG"
  exit 4
fi

current_raw="$(cat "$CURRENT_FILE" 2>/dev/null || true)"
# Support formats: "<index>" or "<index>:<display>". Extract numeric index for
# validation and calculations while preserving the raw value in logs.
if [ -z "$current_raw" ]; then
  echo "bad current: $current_raw" >>"$LOG"
  exit 4
fi
current="${current_raw%%:*}"
case "$current" in
  ''|*[!0-9]*) echo "bad current: $current_raw" >>"$LOG"; exit 4 ;;
esac

echo "current_raw=$current_raw current=$current target=$target" >>"$LOG"

if [ "$current" -eq "$target" ]; then
  echo "already on target" >>"$LOG"
  exit 0
fi

delta=$((target - current))

# left arrow=123, right arrow=124
if [ "$delta" -gt 0 ]; then
  keycode=124
  steps="$delta"
else
  keycode=123
  steps=$(( -delta ))
fi

echo "steps=$steps keycode=$keycode" >>"$LOG"

# Send Ctrl+Arrow steps times; log any error (permissions show up here)
osascript <<EOF >>"$LOG" 2>&1
tell application "System Events"
  repeat $steps times
    key code $keycode using control down
    delay 0.06
  end repeat
end tell
EOF

echo "done" >>"$LOG"