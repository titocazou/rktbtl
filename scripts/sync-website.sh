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
echo "✓ synced game into $DEST"
