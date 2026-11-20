#!/bin/sh
# Renders the icons in src-tauri/icons from the SVG sources in this folder.
# Needs macOS `sips`, ImageMagick (`magick`) and the Tauri CLI. Run from the repo root:
#   sh design/render-icons.sh
set -eu

icons=src-tauri/icons
work=$(mktemp -d)

# App icon set (icns, ico, pngs).
sips -s format png design/icon.svg --out "$work/icon.png" >/dev/null
(cd src-tauri && cargo tauri icon "$work/icon.png" -o icons >/dev/null)

# Menu-bar icon: the Oxen mark, 22pt @2x, as a template image (only alpha matters).
sips -s format png design/tray.svg --out "$icons/tray.png" >/dev/null

# Menu-bar icon when Oxen is switched off: the same mark with a slash through it.
# A wide stroke is erased first so the slash has clear space on both sides.
slash='stroke-linecap round line 7,39 37,5'
magick "$icons/tray.png" \
  \( -size 44x44 xc:none -stroke black -strokewidth 9 -draw "$slash" \) -compose DstOut -composite \
  \( -size 44x44 xc:none -stroke black -strokewidth 3.4 -draw "$slash" \) -compose Over -composite \
  "$icons/tray-off.png"

echo "Rendered icons into $icons (mobile icon folders from the Tauri CLI can be deleted)."
