#!/bin/bash

# RAM usage plugin for sketchybar
# Uses stats_provider system_stats event

if [[ -n "${RAM_USED:-}" && -n "${RAM_TOTAL:-}" ]]; then
	sketchybar --set "$NAME" label="$RAM_USED/$RAM_TOTAL"
else
	sketchybar --set "$NAME" label="${RAM_USAGE:-}"
fi
