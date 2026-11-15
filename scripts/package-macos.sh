#!/bin/sh
# Build Katasymbol.app (Apple Silicon + Intel) into dist/ — run on a Mac.
# Needs only Rust (https://rustup.rs) and the Xcode command line tools
# (xcode-select --install); the script adds the Rust targets itself.
set -e
cd "$(dirname "$0")/.."
command -v cargo >/dev/null || { echo "Rust fehlt: curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh"; exit 1; }
version=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
rustup target add aarch64-apple-darwin x86_64-apple-darwin
export MACOSX_DEPLOYMENT_TARGET=11.0
for t in aarch64-apple-darwin x86_64-apple-darwin; do
    cargo build --release -p katasymbol-app --target "$t"
done

app=dist/Katasymbol.app
rm -rf "$app" && mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
lipo -create -output "$app/Contents/MacOS/katasymbol-app" \
    target/aarch64-apple-darwin/release/katasymbol-app target/x86_64-apple-darwin/release/katasymbol-app
sed "s/@VERSION@/$version/g" packaging/Info.plist > "$app/Contents/Info.plist"

# App icon from the 1024 px PNG.
iconset="$(mktemp -d)/katasymbol.iconset"
mkdir -p "$iconset"
for s in 16 32 128 256 512; do
    sips -z $s $s assets/icon-1024.png --out "$iconset/icon_${s}x${s}.png" >/dev/null
    sips -z $((s * 2)) $((s * 2)) assets/icon-1024.png --out "$iconset/icon_${s}x${s}@2x.png" >/dev/null
done
iconutil -c icns "$iconset" -o "$app/Contents/Resources/katasymbol.icns"

# Ad-hoc signature (no developer account needed; required on Apple Silicon).
codesign --force --deep --sign - "$app"

cp packaging/LIESMICH-macos.txt dist/
ditto -c -k --keepParent "$app" "dist/katasymbol-$version-macos.zip"
hdiutil create -volname Katasymbol -srcfolder "$app" -ov -format UDZO "dist/katasymbol-$version-macos.dmg" >/dev/null
echo "Fertig: $app, dist/katasymbol-$version-macos.zip, dist/katasymbol-$version-macos.dmg"
echo "Starten: open $app"
