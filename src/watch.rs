use std::{ffi::OsString, path::Path, process::Command, time::Duration};

use anyhow::{Context, Result, anyhow};
use bluer::{Adapter, Address, Device, DeviceEvent, DeviceProperty, Session};
use futures::{StreamExt, pin_mut};

use crate::device::configured_mac_address;
use crate::devices::matches_known_profile;

/// Watches BlueZ for a supported earbud connecting and launches the tray app when it does.
///
/// Runs forever; reconnects to BlueZ and retries discovery on any error.
pub async fn run() {
    tracing::info!("watching for supported Soundcore Bluetooth connections");
    loop {
        if let Err(error) = watch_once().await {
            tracing::warn!(%error, "Bluetooth watcher stopped, retrying in 10s");
        }
        tokio::time::sleep(Duration::from_secs(10)).await;
    }
}

async fn watch_once() -> Result<()> {
    let session = Session::new().await.context("could not connect to BlueZ")?;
    let adapter = session
        .default_adapter()
        .await
        .context("could not find the Bluetooth adapter")?;
    let configured = configured_mac_address()?.map(Address::from);

    let target = find_target(&adapter, configured)
        .await?
        .ok_or_else(|| anyhow!("no supported earbuds are paired with this adapter yet"))?;
    let device = adapter.device(target)?;

    let mut was_connected = device.is_connected().await.unwrap_or(false);
    tracing::info!(
        connected = was_connected,
        "watching earbuds connection state"
    );

    let events = device
        .events()
        .await
        .context("could not subscribe to Bluetooth device events")?;
    pin_mut!(events);

    while let Some(event) = events.next().await {
        let DeviceEvent::PropertyChanged(DeviceProperty::Connected(connected)) = event else {
            continue;
        };
        if connected && !was_connected {
            tracing::info!("earbuds connected over Bluetooth");
            launch_app();
        }
        was_connected = connected;
    }

    Err(anyhow!("Bluetooth device event stream ended"))
}

async fn find_target(adapter: &Adapter, configured: Option<Address>) -> Result<Option<Address>> {
    for address in adapter
        .device_addresses()
        .await
        .context("could not list Bluetooth devices")?
    {
        if let Some(configured) = configured {
            if address == configured {
                return Ok(Some(address));
            }
            continue;
        }
        if matches_target_name(&adapter.device(address)?).await {
            return Ok(Some(address));
        }
    }
    Ok(None)
}

async fn matches_target_name(device: &Device) -> bool {
    device
        .name()
        .await
        .ok()
        .flatten()
        .is_some_and(|name| matches_known_profile(&name))
}

fn launch_app() {
    if app_already_running() {
        tracing::debug!("Soundcore Control is already running; not spawning another instance");
        return;
    }
    let Ok(executable) = std::env::current_exe() else {
        tracing::error!("could not determine the Soundcore Control executable path");
        return;
    };
    if let Err(error) = Command::new(executable).arg("--tray-only").spawn() {
        tracing::error!(%error, "could not launch Soundcore Control");
    }
}

/// Checks `/proc` for a running GUI or tray-only instance, ignoring this watcher process itself.
fn app_already_running() -> bool {
    let Some(exe_name) = std::env::current_exe()
        .ok()
        .and_then(|path| path.file_name().map(OsString::from))
    else {
        return false;
    };
    let Ok(entries) = std::fs::read_dir("/proc") else {
        return false;
    };

    for entry in entries.flatten() {
        let Ok(cmdline) = std::fs::read(entry.path().join("cmdline")) else {
            continue;
        };
        let mut arguments = cmdline
            .split(|&byte| byte == 0)
            .filter(|argument| !argument.is_empty())
            .map(|argument| String::from_utf8_lossy(argument).into_owned());
        let Some(binary) = arguments.next() else {
            continue;
        };
        if Path::new(&binary).file_name() != Some(exe_name.as_os_str()) {
            continue;
        }
        match arguments.next().as_deref() {
            None | Some("--tray-only") => return true,
            _ => {}
        }
    }
    false
}
