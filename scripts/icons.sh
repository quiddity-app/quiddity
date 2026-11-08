#!/bin/sh
# Makes every app icon from the SVGs in apps/app/src-tauri/icons/source: the
# desktop and Windows icons, the iOS set, and Android's adaptive icon (its
# foreground, background and monochrome layers) and older launcher icons.
# Needs only what `mise run setup` installs.
set -eu
cd "$(dirname "$0")/.."

icons=apps/app/src-tauri/icons

# The layered manifest makes everything, including the adaptive layers.
pnpm app tauri icon src-tauri/icons/source/app-icon.json

# Tauri composes the older Android launcher icons (square and round) from those
# layers itself, and puts the Q's ring off to one side. Made from the whole
# master alone they are centred, so they replace those.
scratch=$(mktemp -d)
trap 'rm -rf "$scratch"' EXIT
pnpm app tauri icon src-tauri/icons/source/legacy-icon.json -o "$scratch"
for density in mdpi hdpi xhdpi xxhdpi xxxhdpi; do
  for name in ic_launcher ic_launcher_round; do
    cp "$scratch/android/mipmap-$density/$name.png" "$icons/android/mipmap-$density/$name.png"
  done
done

# Tauri writes the adaptive icon's XML without a final newline, which the
# repo's checks (.editorconfig) want.
xml="$icons/android/mipmap-anydpi-v26/ic_launcher.xml"
[ -z "$(tail -c1 "$xml")" ] || echo >> "$xml"
