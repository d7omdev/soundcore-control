#!/usr/bin/env bash
set -euo pipefail

cargo build --release

binary_path="$HOME/.local/bin/liberty-control"
desktop_path="$HOME/.local/share/applications/liberty-control.desktop"

install -Dm755 target/release/liberty-control "$binary_path"
install -Dm644 assets/launcher-logo.png \
	"$HOME/.local/share/icons/hicolor/512x512/apps/liberty-control.png"
sed "s|@EXEC@|$binary_path|g" packaging/liberty-control.desktop >"$desktop_path"
chmod 644 "$desktop_path"
rm -f "$HOME/.local/share/icons/hicolor/scalable/apps/liberty-control.svg"

if command -v gtk-update-icon-cache >/dev/null 2>&1; then
	gtk-update-icon-cache -f -t "$HOME/.local/share/icons/hicolor" >/dev/null 2>&1 || true
fi
if command -v update-desktop-database >/dev/null 2>&1; then
	update-desktop-database "$HOME/.local/share/applications" >/dev/null 2>&1 || true
fi

printf 'Installed Liberty Control. Open it from your app launcher or run: liberty-control\n'
