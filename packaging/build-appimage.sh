#!/usr/bin/env bash
set -euo pipefail

# Builds Soundcore Control as a portable Linux AppImage.
#
# Usage: packaging/build-appimage.sh
#
# Requires `linuxdeploy` (and the `linuxdeploy-plugin-appimage` plugin it
# shells out to) on PATH. Download both from
# https://github.com/linuxdeploy/linuxdeploy/releases and
# https://github.com/linuxdeploy/linuxdeploy-plugin-appimage/releases,
# `chmod +x`, and put them on PATH before running this script.
#
# This AppImage is GUI-only: it does not install the systemd --user watcher
# that auto-launches the tray on Bluetooth connect (see install.sh for that).
# An AppImage is a portable, unpacked-nowhere binary, so there's no stable
# path for a systemd unit to point at.

cd "$(dirname "$0")/.."

cargo build --release

appdir="target/appimage/AppDir"
rm -rf "$appdir"
mkdir -p \
	"$appdir/usr/bin" \
	"$appdir/usr/share/applications" \
	"$appdir/usr/share/icons/hicolor/512x512/apps"

install -Dm755 target/release/soundcore-control "$appdir/usr/bin/soundcore-control"
install -Dm644 assets/launcher-logo.png \
	"$appdir/usr/share/icons/hicolor/512x512/apps/soundcore-control.png"
sed "s|@EXEC@|soundcore-control|g" packaging/soundcore-control.desktop \
	>"$appdir/usr/share/applications/soundcore-control.desktop"

version="$(git describe --tags --always 2>/dev/null || true)"
if [ -z "$version" ]; then
	version="$(grep '^version' Cargo.toml | head -1 | cut -d '"' -f2)"
fi

# linuxdeploy's bundled AppImage runtime needs FUSE to mount itself when run
# normally; CI runners (and many containers) don't have FUSE available, so
# fall back to extract-and-run, which works everywhere.
export APPIMAGE_EXTRACT_AND_RUN=1

VERSION="$version" linuxdeploy \
	--appdir "$appdir" \
	--desktop-file "$appdir/usr/share/applications/soundcore-control.desktop" \
	--icon-file "$appdir/usr/share/icons/hicolor/512x512/apps/soundcore-control.png" \
	--output appimage

target_name="Soundcore-Control-${version}-x86_64.AppImage"
generated="$(find . -maxdepth 1 -name '*.AppImage' ! -name "$target_name" -printf '%f\n' | head -1)"
if [ -n "$generated" ]; then
	mv "$generated" "$target_name"
fi

printf 'Built %s\n' "$target_name"
