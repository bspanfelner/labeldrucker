#!/bin/sh
# Build the web app and zip it into dist/ (upload the contents to any HTTPS
# web host, e.g. GitHub Pages).
set -e
cd "$(dirname "$0")/.."
./web/build.sh
version=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
mkdir -p dist
rm -f "dist/katasymbol-$version-web.zip"
(cd web && zip -qr "../dist/katasymbol-$version-web.zip" index.html manifest.json icon.svg pkg)
echo "dist/katasymbol-$version-web.zip"
