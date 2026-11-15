#!/bin/sh
# Install Katasymbol for the current user (no root needed):
# program to ~/.local/bin, menu entry and icon to ~/.local/share.
set -e
here="$(cd "$(dirname "$0")" && pwd)"
bin="$HOME/.local/bin"; apps="$HOME/.local/share/applications"; icons="$HOME/.local/share/icons/hicolor/256x256/apps"
mkdir -p "$bin" "$apps" "$icons"
install -m 755 "$here/katasymbol-app" "$bin/katasymbol-app"
install -m 644 "$here/katasymbol.png" "$icons/katasymbol.png"
sed "s|^Exec=.*|Exec=$bin/katasymbol-app|" "$here/katasymbol.desktop" > "$apps/katasymbol.desktop"
command -v update-desktop-database >/dev/null && update-desktop-database "$apps" 2>/dev/null || true
echo "Installiert. Katasymbol steht jetzt im Anwendungsmenü (oder: $bin/katasymbol-app)."
