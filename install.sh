#!/usr/bin/env bash
set -euo pipefail

binary_path="$HOME/.local/bin/liberty-control"
desktop_path="$HOME/.local/share/applications/liberty-control.desktop"
icon_path="$HOME/.local/share/icons/hicolor/512x512/apps/liberty-control.png"
watch_service_path="$HOME/.config/systemd/user/liberty-control-watch.service"

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

	install -Dm755 target/release/liberty-control "$binary_path"
	install -Dm644 assets/launcher-logo.png "$icon_path"
	sed "s|@EXEC@|$binary_path|g" packaging/liberty-control.desktop >"$desktop_path"
	chmod 644 "$desktop_path"
	rm -f "$HOME/.local/share/icons/hicolor/scalable/apps/liberty-control.svg"

	refresh_caches

	if command -v systemctl >/dev/null 2>&1; then
		sed "s|@EXEC@|$binary_path|g" packaging/liberty-control-watch.service >"$watch_service_path"
		chmod 644 "$watch_service_path"
		systemctl --user daemon-reload
		systemctl --user enable --now liberty-control-watch.service
		printf 'Installed Liberty Control and enabled the Bluetooth connection watcher (systemd --user).\n'
		printf 'Liberty Control will open in the tray automatically when the Liberty 4 Pro connects.\n'
	else
		printf 'Installed Liberty Control. Open it from your app launcher or run: liberty-control\n'
		printf 'systemctl not found: skipped installing the auto-open-on-connect watcher.\n'
	fi
}

uninstall_app() {
	if command -v systemctl >/dev/null 2>&1 && [ -f "$watch_service_path" ]; then
		systemctl --user disable --now liberty-control-watch.service >/dev/null 2>&1 || true
		systemctl --user daemon-reload
	fi

	pkill -f "^$binary_path( --tray-only)?\$" >/dev/null 2>&1 || true

	rm -f "$watch_service_path" "$binary_path" "$desktop_path" "$icon_path"

	refresh_caches

	printf 'Uninstalled Liberty Control and removed the Bluetooth connection watcher.\n'
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
