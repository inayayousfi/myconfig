#!/usr/bin/env bash
# Regenerate the black-pink rEFInd theme PNG assets.
#
# Usage: generate-theme-assets.sh <output-theme-dir>
#
# The output directory is created if missing. ImageMagick (`magick` or the
# legacy `convert` binary) is required.

set -euo pipefail

if [[ $# -ne 1 ]]; then
    echo "usage: $0 <output-theme-dir>" >&2
    exit 64
fi

theme_dir="$1"

if command -v magick >/dev/null 2>&1; then
    image_tool="magick"
elif command -v convert >/dev/null 2>&1; then
    image_tool="convert"
else
    echo "ImageMagick (magick or convert) is required." >&2
    exit 1
fi

mkdir -p "$theme_dir"

"$image_tool" -size 1920x1080 xc:'#000000' \
    -fill '#ff4ead' -draw 'rectangle 0,1076 1920,1080' \
    "$theme_dir/banner.png"

"$image_tool" -size 144x144 xc:none \
    -fill 'rgba(255,78,173,0.16)' -draw 'roundrectangle 2,2 142,142 12,12' \
    -stroke '#ff4ead' -strokewidth 3 -fill none -draw 'roundrectangle 2,2 142,142 12,12' \
    "$theme_dir/selection_big.png"

"$image_tool" -size 64x64 xc:none \
    -fill 'rgba(255,78,173,0.16)' -draw 'roundrectangle 1,1 63,63 6,6' \
    -stroke '#ff4ead' -strokewidth 2 -fill none -draw 'roundrectangle 1,1 63,63 6,6' \
    "$theme_dir/selection_small.png"
