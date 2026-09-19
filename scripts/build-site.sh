#!/usr/bin/env bash
# Assemble the public website (site/) into dist-site/ for GitHub Pages.
# Images and fonts are copied from the app so the site never drifts from it.
# Run from the workspace root.
set -euo pipefail

out="dist-site"
rm -rf "$out"
mkdir -p "$out/screenshots" "$out/fonts"

cp site/*.html site/*.css "$out/"
cp src-tauri/icons/128x128@2x.png "$out/icon.png"
cp docs/screenshots/01-mail-inbox.png "$out/screenshots/mail-inbox.png"
cp public/fonts/InstrumentSans-latin.woff2 "$out/fonts/"
cp public/fonts/licenses/OFL-InstrumentSans.txt "$out/fonts/"
# Search Console ownership file(s), if any (google*.html).
find site -maxdepth 1 -name 'google*.html' -exec cp {} "$out/" \;
touch "$out/.nojekyll"

echo "OK: site assembled in $out/"
