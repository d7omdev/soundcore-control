#!/usr/bin/env bash
set -euo pipefail

binary_path="$HOME/.local/bin/soundcore-control"
desktop_path="$HOME/.local/share/applications/soundcore-control.desktop"
icon_path="$HOME/.local/share/icons/hicolor/512x512/apps/soundcore-control.png"
watch_service_path="$HOME/.config/systemd/user/soundcore-control-watch.service"

# Pre-rename (Liberty Control) install paths, removed on upgrade so users don't end up
# with two tray icons/binaries after pulling the renamed app.
legacy_binary_path="$HOME/.local/bin/liberty-control"
legacy_desktop_path="$HOME/.local/share/applications/liberty-control.desktop"
legacy_icon_path="$HOME/.local/share/icons/hicolor/512x512/apps/liberty-control.png"
legacy_watch_service_path="$HOME/.config/systemd/user/liberty-control-watch.service"

remove_legacy_install() {
	if command -v systemctl >/dev/null 2>&1 && [ -f "$legacy_watch_service_path" ]; then
		systemctl --user disable --now liberty-control-watch.service >/dev/null 2>&1 || true
	fi
	pkill -f "^$legacy_binary_path( --tray-only)?\$" >/dev/null 2>&1 || true
	rm -f "$legacy_watch_service_path" "$legacy_binary_path" "$legacy_desktop_path" "$legacy_icon_path"
}

refresh_caches() {
	if command -v gtk-update-icon-cache >/dev/null 2>&1; then
		gtk-update-icon-cache -f -t "$HOME/.local/share/icons/hicolor" >/dev/null 2>&1 || true
	fi
	if command -v update-desktop-database >/dev/null 2>&1; then
		update-desktop-database "$HOME/.local/share/applications" >/dev/null 2>&1 || true
	fi
}

install_app() {
	cargo build --release

	remove_legacy_install

	install -Dm755 target/release/soundcore-control "$binary_path"
	install -Dm644 assets/launcher-logo.png "$icon_path"
	sed "s|@EXEC@|$binary_path|g" packaging/soundcore-control.desktop >"$desktop_path"
	chmod 644 "$desktop_path"
	rm -f "$HOME/.local/share/icons/hicolor/scalable/apps/soundcore-control.svg"

	refresh_caches

	if command -v systemctl >/dev/null 2>&1; then
		sed "s|@EXEC@|$binary_path|g" packaging/soundcore-control-watch.service >"$watch_service_path"
		chmod 644 "$watch_service_path"
		systemctl --user daemon-reload
		systemctl --user enable --now soundcore-control-watch.service
		printf 'Installed Soundcore Control and enabled the Bluetooth connection watcher (systemd --user).\n'
		printf 'Soundcore Control will open in the tray automatically when a supported device connects.\n'
	else
		printf 'Installed Soundcore Control. Open it from your app launcher or run: soundcore-control\n'
		printf 'systemctl not found: skipped installing the auto-open-on-connect watcher.\n'
	fi
}

uninstall_app() {
	if command -v systemctl >/dev/null 2>&1 && [ -f "$watch_service_path" ]; then
		systemctl --user disable --now soundcore-control-watch.service >/dev/null 2>&1 || true
		systemctl --user daemon-reload
	fi

	pkill -f "^$binary_path( --tray-only)?\$" >/dev/null 2>&1 || true

	rm -f "$watch_service_path" "$binary_path" "$desktop_path" "$icon_path"

	refresh_caches

	printf 'Uninstalled Soundcore Control and removed the Bluetooth connection watcher.\n'
}

case "${1:-install}" in
install)
	install_app
	;;
uninstall)
	uninstall_app
	;;
*)
	printf 'Usage: %s [install|uninstall]\n' "$0" >&2
	exit 1
	;;
esac
