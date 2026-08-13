# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project

Soundcore Control (`soundcore-control`, formerly Liberty Control) is a native Linux desktop controller for Soundcore earbuds and headphones. It talks to paired devices over BlueZ via a pinned fork of `openscq30-lib`, and presents an `egui`/`eframe` GUI plus a system tray (`ksni`, StatusNotifierItem).

Supported models live in `src/devices.rs`'s `DEVICE_PROFILES` registry (currently Liberty 4 Pro, R60i NC, P20i/P25i/R50i, Life Q30, Space One Pro) — each entry maps a Bluetooth-advertised name to an `openscq30_lib::DeviceModel`, an optional in-app icon, and whether the device exposes manual noise-canceling/transparency ranges (`supports_manual_ambient_ranges`) vs. discrete modes only. Extending device support means adding an entry there, not touching connection logic.

## Commands

```bash
cargo run --release                 # run the GUI
cargo fmt --all -- --check          # formatting check (must pass, enforced by pre-commit hook and CI)
cargo clippy --all-targets -- -D warnings   # lint, warnings are errors
cargo test --all-targets            # run unit + integration tests
cargo test --test domain_test       # run a single integration test file (tests/domain_test.rs)
cargo test some_test_name           # run tests matching a name
```

`.githooks/pre-commit` runs `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`, and `cargo test` — the same checks CI runs (`.github/workflows/ci.yml`). Run these before committing; there is no separate lint/typecheck step to remember.

If more than one compatible device is visible, `SOUNDCORE_CONTROL_MAC=AA:BB:CC:DD:EE:FF` selects one (read via `device::configured_mac_address()`, used by both the device worker and the Bluetooth watcher).

Runtime state lives in `${XDG_DATA_HOME:-~/.local/share}/soundcore-control/devices.sqlite3` (OpenSCQ30's device database, not app-specific config). `device::database_path()` transparently migrates an old `liberty-control` data directory on first launch if found.

## Architecture

The binary has three run modes, selected by CLI flags checked early in `main()`:
- default: full GUI (`eframe`/`egui`)
- `--tray-only`: tray icon only, no window shown at startup (used when auto-launched on Bluetooth connect)
- `--watch`: background watcher process (`watch.rs`) that has no UI at all — it listens for any known device (per `devices::matches_known_profile`) connecting over BlueZ (via `bluer`) and spawns a fresh `--tray-only` instance, guarding against duplicate launches by scanning `/proc` for an already-running instance with the same executable name.

Module layout (`src/lib.rs` re-exports all of these):

- **`device.rs`** — owns the Bluetooth/OpenSCQ30 connection. `DeviceWorker::spawn()` starts a dedicated OS thread with its own Tokio runtime, decoupling async device I/O from the (sync) egui event loop. Communication is two one-way channels: `DeviceCommand`s go in via a `tokio::mpsc` sender, `DeviceEvent`s (Searching/Connecting/Connected/Snapshot/Error/Disconnected) come out via a std `mpsc` receiver that the UI polls with `try_recv()`. Also handles pairing, initial connection, the `event_loop` that races device changes/connection status/incoming commands via `tokio::select!`, and a workaround (`initialize_ambient_level`) for the earbuds sometimes omitting their active ambient strength on startup.
- **`domain.rs`** — pure translation layer between OpenSCQ30's generic `Setting`/`SettingId`/`Value` model and this app's UI-facing types (`DeviceSnapshot`, `ControlSetting`, `ListeningMode`, `EqualizerState`, etc.). `setting_changes()` maps a `DeviceCommand` to the OpenSCQ30 setting writes it requires; `snapshot_from_settings()` does the reverse (build a `DeviceSnapshot` from a settings lookup closure). This is the module to extend when adding a new control/setting. Has the most test coverage (`tests/domain_test.rs`, `tests/ambient_state_test.rs`).
- **`tray.rs`** — `ksni`-based StatusNotifierItem tray icon: battery display, ambient-mode submenu, disconnect/reopen/quit actions. Runs on its own thread, driven by `TrayState` diffs and emitting `TrayAction`s back to the main app over channels, following the same worker-thread-plus-channel pattern as `device.rs`.
- **`watch.rs`** — the `--watch` mode described above; independent of the GUI/device-worker machinery, talks to `bluer` directly.
- **`notify.rs`** — thin wrapper around `notify-rust` for desktop notifications (e.g. on auto-launch from the watcher).
- **`main.rs`** — CLI flag handling, the `eframe::App` implementation (`SoundcoreApp`) and all egui screen/widget code (equalizer, ambient slider or mode-only picker, daily/earbud controls, responsive layout down to 420px).

Cross-cutting pattern: both `device.rs` and `tray.rs` isolate blocking/async work on background threads and communicate with the main egui loop purely through channels — follow this pattern rather than calling into `bluer`/`openscq30-lib` directly from UI code. The device worker thread also wraps its work in `catch_unwind`, since `openscq30-lib` panics rather than erroring for some unsupported per-model setting writes.

`openscq30-lib` is pinned to a specific git revision in `Cargo.toml` (not a published crate version); check that pin before assuming upstream API behavior matches published docs. `list_devices`/`pair` are per-`DeviceModel`, so discovering an unknown-model device means trying each `DEVICE_PROFILES` entry in turn (see `device::find_target_device`).
