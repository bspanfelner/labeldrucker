#!/bin/sh
# Build a portable Linux release (glibc >= 2.30) into dist/.
# Needs: cargo-zigbuild (cargo install cargo-zigbuild) and zig (pip install ziglang).
set -e
cd "$(dirname "$0")/.."
version=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
target=x86_64-unknown-linux-gnu
cargo zigbuild --release -p katasymbol-app --target "$target.2.30"
name="katasymbol-$version-linux-x86_64"
out="dist/$name"
rm -rf "$out" && mkdir -p "$out"
cp "target/$target/release/katasymbol-app" "$out/"
cp packaging/katasymbol.desktop packaging/install-linux.sh "$out/"
cp packaging/LIESMICH-linux.txt "$out/LIESMICH.txt"
cp assets/icon-256.png "$out/katasymbol.png"
chmod +x "$out/katasymbol-app" "$out/install-linux.sh"
tar -C dist -czf "dist/$name.tar.gz" "$name"
echo "dist/$name.tar.gz"
