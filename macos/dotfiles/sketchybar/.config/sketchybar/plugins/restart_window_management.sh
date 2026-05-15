#!/usr/bin/env bash

set -euo pipefail

# Restart Yabai (window management)
yabai --restart-service 2>/dev/null || yabai --start-service 2>/dev/null || true

# Reload Sketchybar
sketchybar --reload
