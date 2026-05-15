#!/bin/sh
set -eu

# Show Mission Control (default macOS shortcut: Control + Up Arrow)
osascript <<'EOF'
tell application "System Events"
  key code 126 using control down
end tell
EOF
