#!/bin/sh
# Build the web app into web/ (pkg/ holds the wasm). Needs:
#   rustup target add wasm32-unknown-unknown
#   cargo install wasm-bindgen-cli --version <same as wasm-bindgen in Cargo.lock>
set -e
cd "$(dirname "$0")/.."
# The build id (commit + time) shows in the app and names the files the page
# loads, so browsers never keep running an older build from their cache.
KATASYMBOL_BUILD="$(git rev-parse --short HEAD 2>/dev/null || echo dev)$(git diff --quiet HEAD 2>/dev/null || echo +) $(date -u +%Y-%m-%d\ %H:%M) UTC"
export KATASYMBOL_BUILD
cargo build -p katasymbol-webapp --target wasm32-unknown-unknown --profile web
wasm-bindgen --target web --no-typescript --out-dir web/pkg \
  target/wasm32-unknown-unknown/web/katasymbol_webapp.wasm
echo "$KATASYMBOL_BUILD" > web/pkg/build.txt
echo "Built web/ ($KATASYMBOL_BUILD). Serve it over HTTPS (or http://localhost), e.g.:"
echo "  python3 -m http.server -d web 8080"
