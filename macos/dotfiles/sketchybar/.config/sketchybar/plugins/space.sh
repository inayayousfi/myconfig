#!/bin/sh
set -eu

LOG="/tmp/sketchybar_switch_space.log"
echo "---- $(date '+%F %T') ----" >>"$LOG"
echo "PWD=$(pwd)" >>"$LOG"
echo "ARGS: $*" >>"$LOG"
echo "NAME=${NAME:-} SID=${SID:-} SENDER=${SENDER:-} SELECTED=${SELECTED:-}" >>"$LOG"
echo "PATH=$PATH" >>"$LOG"

# $SELECTED: true/false
# $SID: space id associated to this item (available for space components)
# $NAME: item name

CURRENT_FILE="/tmp/sketchybar_current_space"

# Try to re-fetch the current space (index) and which display it's on using yabai.
# Save as "<index>:<display>" so callers can know both the space and the screen.
CURRENT_INDEX=""
CURRENT_DISPLAY=""
if command -v yabai >/dev/null 2>&1; then
  if yabai_out=$(yabai -m query --spaces --space 2>/dev/null); then
    # Prefer python3, fallback to python. If neither exists, skip JSON parsing.
    if command -v python3 >/dev/null 2>&1; then
      parsed=$(printf '%s' "$yabai_out" | python3 -c 'import sys,json; j=json.load(sys.stdin); print(f"{j.get("index","")}:{j.get("display","")}")') || parsed=""
    elif command -v python >/dev/null 2>&1; then
      parsed=$(printf '%s' "$yabai_out" | python -c 'import sys,json; j=json.load(sys.stdin); print(f"{j.get("index","")}:{j.get("display","")}")') || parsed=""
    else
      parsed=""
    fi

    if [ -n "$parsed" ]; then
      CURRENT_INDEX=${parsed%%:*}
      CURRENT_DISPLAY=${parsed##*:}
      # Write the refreshed current space info (index:display)
      echo "${CURRENT_INDEX}:${CURRENT_DISPLAY}" >"$CURRENT_FILE"
    fi
  fi
fi

# Keep sketchybar visuals in sync
# Default: ensure background visibility follows the selected state
sketchybar --set "$NAME" background.drawing="$SELECTED"

# Hover color (desaturated Monokai green) and selected color (Monokai pink)
# Monokai green is 0xffa6e22e; this is a slightly grayed/desaturated variant (tidi-er gray)
HOVER_COLOR=0xff8fa354
SELECTED_COLOR=0xfff92672

hover_on() {
  sketchybar --set "$NAME" background.color=$HOVER_COLOR background.drawing=on
}

hover_off() {
  # Restore the selected color and drawing state
  sketchybar --set "$NAME" background.color=$SELECTED_COLOR background.drawing="$SELECTED"
}

case "${SENDER:-}" in
  "mouse.entered")
    hover_on
    ;;
  "mouse.exited" | "mouse.exited.global")
    hover_off
    ;;
  *)
    # When invoked for selection changes or initial run, ensure state is correct
    sketchybar --set "$NAME" background.drawing="$SELECTED"
    ;;
esac

if [ "${SELECTED:-false}" = "true" ] && [ -n "${SID:-}" ]; then
  # When this item becomes selected, write the selected SID and the display we
  # just determined (if available). This ensures any switch action reads a
  # fresh SID:DISPLAY pair instead of relying on stale state.
  if [ -n "$CURRENT_DISPLAY" ]; then
    echo "${SID}:${CURRENT_DISPLAY}" >"$CURRENT_FILE"
  else
    echo "$SID" >"$CURRENT_FILE"
  fi
fi