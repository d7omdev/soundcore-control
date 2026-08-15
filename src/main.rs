mod app;
mod ui;

use std::{process::Command, thread, time::Duration};

use app::SoundcoreApp;
use eframe::egui;
use soundcore_control::{
    device::{DeviceEvent, DeviceWorker},
    domain::DeviceCommand,
    tray::{TrayAction, TrayController, TrayState},
};

fn main() -> eframe::Result {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "soundcore_control=info".into()),
        )
        .init();

    // Selects the system locale for openscq30-lib's translated setting labels (falls back
    // to English for unsupported locales). Must run before any device connection, since
    // that's what populates the labels in DeviceSnapshot::daily_controls/button_controls.
    let requested_languages = i18n_embed::DesktopLanguageRequester::requested_languages();
    openscq30_lib::i18n::init(&requested_languages);

    if std::env::args().any(|argument| argument == "--tray-only") {
        run_tray_only();
        return Ok(());
    }

    if std::env::args().any(|argument| argument == "--watch") {
        run_watch();
        return Ok(());
    }

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Soundcore Control")
            .with_inner_size([920.0, 720.0])
            .with_min_inner_size([420.0, 560.0]),
        ..Default::default()
    };

    eframe::run_native(
        "Soundcore Control",
        options,
        Box::new(|context| Ok(Box::new(SoundcoreApp::new(context)))),
    )
}

fn run_tray_only() {
    let tray = TrayController::spawn_background();
    tray.update(TrayState::searching());
    // Register the replacement tray item before reconnecting, so closing the UI
    // never leaves the desktop without a Soundcore Control tray icon.
    thread::sleep(Duration::from_millis(500));
    let worker = DeviceWorker::spawn();
    let mut open_window = false;
    let mut device_name = String::new();

    'service: loop {
        while let Some(event) = worker.try_recv() {
            match event {
                DeviceEvent::Searching => tray.update(TrayState::searching()),
                DeviceEvent::Connecting { name, .. } => {
                    device_name = name;
                    tray.update(TrayState::connecting(&device_name));
                }
                DeviceEvent::Connected {
                    name,
                    snapshot,
                    icon: _,
                    supports_manual_ambient_ranges: _,
                } => {
                    device_name = name;
                    tray.update(TrayState::connected(&device_name, &snapshot));
                    soundcore_control::notify::buds_connected(&device_name, &snapshot);
                }
                DeviceEvent::Snapshot(snapshot) => {
                    tray.update(TrayState::connected(&device_name, &snapshot));
                }
                DeviceEvent::Error(error) => {
                    tracing::error!(%error, "tray Bluetooth error");
                    tray.update(TrayState::failed());
                }
                DeviceEvent::Disconnected => tray.update(TrayState::disconnected()),
            }
        }

        while let Some(action) = tray.try_recv() {
            match action {
                TrayAction::ShowWindow => {
                    open_window = true;
                    break 'service;
                }
                TrayAction::Disconnect => {
                    let _ = worker.send(DeviceCommand::Disconnect);
                }
                TrayAction::SetListeningMode(mode) => {
                    let _ = worker.send(DeviceCommand::SetListeningMode(mode));
                }
                TrayAction::Quit => break 'service,
            }
        }
        thread::sleep(Duration::from_millis(100));
    }

    drop(worker);
    drop(tray);
    if open_window {
        thread::sleep(Duration::from_millis(350));
        if let Ok(executable) = std::env::current_exe()
            && let Err(error) = Command::new(executable).spawn()
        {
            tracing::error!(%error, "could not reopen Soundcore Control");
        }
    }
}

fn run_watch() {
    let runtime = match tokio::runtime::Runtime::new() {
        Ok(runtime) => runtime,
        Err(error) => {
            tracing::error!(%error, "could not start Bluetooth watcher runtime");
            return;
        }
    };
    runtime.block_on(soundcore_control::watch::run());
}

fn launch_tray_only() {
    let Ok(executable) = std::env::current_exe() else {
        return;
    };
    if let Err(error) = Command::new(executable).arg("--tray-only").spawn() {
        tracing::error!(%error, "could not keep Soundcore Control in the tray");
    }
}
