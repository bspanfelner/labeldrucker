#!/bin/sh
# Build the wasm module into web/pkg. Needs: rustup target add wasm32-unknown-unknown,
# cargo install wasm-bindgen-cli --version <same as Cargo.lock>.
set -e
cd "$(dirname "$0")/.."
cargo build -p katasymbol-web --target wasm32-unknown-unknown --release
wasm-bindgen --target web --no-typescript --out-dir web/pkg \
  target/wasm32-unknown-unknown/release/katasymbol_web.wasm
echo "Serve with: python3 -m http.server -d web 8080  ->  http://localhost:8080"
