<p align="center">
  <img src="assets/launcher-logo.png" alt="Soundcore Control icon" width="112">
</p>

<h1 align="center">Soundcore Control</h1>

<p align="center">
  A native Linux desktop controller for Soundcore earbuds and headphones.
</p>

<p align="center">
  <a href="https://github.com/d7omdev/soundcore-control/actions/workflows/ci.yml"><img alt="CI" src="https://github.com/d7omdev/soundcore-control/actions/workflows/ci.yml/badge.svg"></a>
  <img alt="Linux" src="https://img.shields.io/badge/platform-Linux-1793D1?style=flat-square">
  <img alt="Rust 1.85+" src="https://img.shields.io/badge/Rust-1.85%2B-DEA584?style=flat-square&logo=rust">
  <img alt="GPL-3.0-or-later" src="https://img.shields.io/badge/license-GPL--3.0--or--later-blue?style=flat-square">
</p>

<p align="center">
  <img src="docs/screenshots/ambient.png" alt="Soundcore Control ambient sound screen" width="520">
</p>

Soundcore Control brings the most useful Soundcore mobile-app controls to a responsive desktop interface. It talks directly to paired devices through BlueZ and [OpenSCQ30](https://github.com/Oppzippy/OpenSCQ30)'s Soundcore protocol implementation.

> [!IMPORTANT]
> Soundcore Control currently targets **Liberty 4 Pro** (verified), plus **R60i NC**, **P20i/P25i/R50i**, **Life Q30**, and **Space One Pro**. Support for the latter four is new and their Bluetooth device names are best-effort guesses. If your device isn't found, please open an issue with the exact name it advertises.

## Features

- Live left, right, and case battery levels
- Continuous ambient control on devices with manual noise-canceling/transparency ranges:
  - Levels **1–5** for transparency
  - A dedicated **Normal** mode in the center
  - Levels **6–10** for noise canceling
- Mode-only ambient picker (Normal/Transparency/Noise Canceling) on devices without a manual intensity range
- Automatic recovery when supported earbuds omit the active ambient strength during startup
- Soundcore equalizer presets and an eight-band custom equalizer
- Daily controls including wearing detection, Easy Chat, wind-noise reduction, auto power-off, LDAC, and sound leak compensation — shown per device based on what it actually supports
- Configurable left/right press, long-press, and slide gestures
- Persistent system tray menu with battery information, ambient-mode selection, Bluetooth disconnect, reopen, and quit actions
- Responsive, scrollable interface usable down to a 420 px window width
- Control labels follow your system locale via `openscq30-lib`'s translations (~12 languages)

## Screenshots

<table>
  <tr>
    <td align="center"><strong>Equalizer</strong></td>
    <td align="center"><strong>Daily and earbud controls</strong></td>
  </tr>
  <tr>
    <td><img src="docs/screenshots/equalizer.png" alt="Equalizer screen"></td>
    <td><img src="docs/screenshots/controls.png" alt="Daily and earbud controls screen"></td>
  </tr>
</table>

## Requirements

- Linux with BlueZ
- A supported device already paired and connected
- A desktop with Wayland or X11
- Rust **1.85 or newer** when building from source

Install the build and runtime dependencies on Arch Linux:

```bash
sudo pacman -S --needed \
  base-devel rustup pkgconf dbus bluez bluez-utils \
  libxkbcommon wayland mesa libx11 libxcursor libxi libxrandr
rustup default stable
```

Package names differ on other distributions.

## Try it without building

Download the latest `.AppImage` from the [Releases page](https://github.com/d7omdev/soundcore-control/releases), make it executable, and run it:

```bash
chmod +x Soundcore-Control-*.AppImage
./Soundcore-Control-*.AppImage
```

No Rust toolchain needed. This is the GUI only, it doesn't set up the background watcher that auto-launches the tray when your earbuds connect (see [Install](#install) below for that).

## Install

Clone the repository and run the user-level installer:

```bash
git clone https://github.com/d7omdev/soundcore-control.git
cd soundcore-control
./install.sh
```

The installer builds an optimized release and installs:

| Item | Path |
| --- | --- |
| Binary | `~/.local/bin/soundcore-control` |
| Desktop launcher | `~/.local/share/applications/soundcore-control.desktop` |
| Application icon | `~/.local/share/icons/hicolor/512x512/apps/soundcore-control.png` |

If you have an existing Liberty Control install, `install.sh` removes its binary, desktop launcher, icon, and systemd watcher unit automatically, and your paired-device database is migrated to the new location on first launch.

Open **Soundcore Control** from the desktop launcher, or run:

```bash
~/.local/bin/soundcore-control
```

## Run from source

```bash
cargo run --release
```

If more than one compatible device is visible, select one by MAC address:

```bash
SOUNDCORE_CONTROL_MAC=AA:BB:CC:DD:EE:FF cargo run --release
```

## System tray

While Soundcore Control is running, its logo appears in desktops that support the StatusNotifierItem protocol. Click the icon to view battery information, change ambient mode, disconnect the device, reopen the window, or quit the application.

Closing the main window hides it while the tray service and Bluetooth connection continue running. Use **Quit Soundcore Control** in the tray menu to exit completely. Tray presentation depends on the desktop shell or status bar.

## Troubleshooting

### Device is not found

1. Confirm the device is awake and connected in your desktop Bluetooth settings.
2. Run `bluetoothctl info <MAC>` and check that it reports `Connected: yes`.
3. Reopen Soundcore Control or press **Try again**.
4. If your device is one of the newly added models, its Bluetooth name may not match what we guessed — please open an issue with the exact advertised name.

### Tray icon is missing

Confirm that your desktop panel supports StatusNotifierItem/AppIndicator tray icons. Some minimal Wayland setups require enabling a tray module in the panel configuration.

### Ambient level is missing after connection

On devices with manual noise-canceling/transparency ranges, Soundcore Control repairs an occasionally missing active-mode strength during startup. If the device was connected after the app opened, press **Try again** to establish a fresh control session.

## Development

```bash
cargo fmt --all -- --check
cargo test --all-targets
cargo clippy --all-targets -- -D warnings
```

The app is split into focused modules:

- `src/device.rs` - BlueZ/OpenSCQ30 connection and command worker
- `src/devices.rs` - the supported-device registry, loaded from `assets/devices.toml`
- `src/domain.rs` - device-to-UI state mapping and command translation
- `src/tray.rs` - Linux StatusNotifierItem integration
- `src/app.rs` - the `eframe::App` state machine (connection/tray event handling)
- `src/ui/` - egui screen and widget code, split by feature (ambient, equalizer, controls, header, shared widgets, theme)
- `src/main.rs` - CLI entry point and run-mode dispatch

Adding a new device model means adding an entry to `assets/devices.toml` (and a PNG under `assets/icons/` if you have one), not editing Rust code.

Runtime state is stored in `${XDG_DATA_HOME:-~/.local/share}/soundcore-control/devices.sqlite3`. Soundcore Control has no telemetry and does not require an internet connection at runtime.

## Uninstall

```bash
rm -f ~/.local/bin/soundcore-control
rm -f ~/.local/share/applications/soundcore-control.desktop
rm -f ~/.local/share/icons/hicolor/512x512/apps/soundcore-control.png
```

## Credits

Bluetooth protocol support is provided by [`openscq30-lib`](https://github.com/Oppzippy/OpenSCQ30).

Soundcore is a trademark of its respective owner. Soundcore Control is an unofficial community project and is not affiliated with or endorsed by Soundcore or Anker Innovations.

## License

Licensed under the [GNU General Public License v3.0 or later](LICENSE).
