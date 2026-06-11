#!/usr/bin/env bash
# Sync the standalone browser game into the personal website's RKTBTL page,
# which loads web/game.html in an overlay iframe. Run after ./scripts/build-wasm.sh
# whenever the game (game.html / game.js / wasm) changes.
set -euo pipefail
cd "$(dirname "$0")/.."

SRC="web"
DEST="${RKTBTL_WEBSITE_DIR:-$HOME/Documents/website/rktbtl}"

mkdir -p "$DEST/pkg"
cp "$SRC/game.html"               "$DEST/game.html"
cp "$SRC/game.js"                 "$DEST/game.js"
cp "$SRC/pkg/rocket_wasm.js"      "$DEST/pkg/rocket_wasm.js"
cp "$SRC/pkg/rocket_wasm_bg.wasm" "$DEST/pkg/rocket_wasm_bg.wasm"

# Cache-bust: stamp every `?v=dev` load token with a hash of the actual bundle,
# so the browser refetches game.js / the wasm glue / the wasm exactly when one of
# them changed, and keeps caching when nothing did. No manual version bumping.
STAMP=$(cat "$DEST/game.html" "$DEST/game.js" "$DEST/pkg/rocket_wasm.js" \
            "$DEST/pkg/rocket_wasm_bg.wasm" | shasum | cut -c1-10)
sed -i '' "s/?v=dev/?v=$STAMP/g" "$DEST/game.html" "$DEST/game.js"
echo "✓ synced game into $DEST (cache token v=$STAMP)"
