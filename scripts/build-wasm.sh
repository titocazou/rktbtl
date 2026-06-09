#!/usr/bin/env bash
# Build the browser frontend: compile rocket-wasm to wasm32 and generate the JS
# bindings into web/pkg/. Requires `wasm-bindgen-cli` (cargo install wasm-bindgen-cli).
set -euo pipefail
cd "$(dirname "$0")/.."

cargo build -p rocket-wasm --release --target wasm32-unknown-unknown
wasm-bindgen target/wasm32-unknown-unknown/release/rocket_wasm.wasm \
  --target web --out-dir web/pkg --out-name rocket_wasm
echo "✓ web/pkg updated"
