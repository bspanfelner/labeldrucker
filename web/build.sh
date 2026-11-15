#!/bin/sh
# Build the web app into web/ (pkg/ holds the wasm). Needs:
#   rustup target add wasm32-unknown-unknown
#   cargo install wasm-bindgen-cli --version <same as wasm-bindgen in Cargo.lock>
set -e
cd "$(dirname "$0")/.."
cargo build -p katasymbol-webapp --target wasm32-unknown-unknown --profile web
wasm-bindgen --target web --no-typescript --out-dir web/pkg \
  target/wasm32-unknown-unknown/web/katasymbol_webapp.wasm
echo "Built web/. Serve it over HTTPS (or http://localhost), e.g.:"
echo "  python3 -m http.server -d web 8080"
