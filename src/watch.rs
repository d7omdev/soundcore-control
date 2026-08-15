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
    if find_own_processes(|second_argument| matches!(second_argument, None | Some("--tray-only")))
        .next()
        .is_some()
    {
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

/// Stops any running `--watch` background process for this executable, so quitting the app
/// (from the tray or the main window) doesn't leave the watcher alive to silently relaunch a
/// new instance, and reopen a Bluetooth connection, the next time the earbuds connect.
///
/// Tries `systemctl --user stop` first, since `install.sh` runs the watcher as a
/// `Restart=on-failure` systemd unit: killing that process directly (bypassing systemd's own
/// stop path) makes systemd treat the death as a crash and respawn it a few seconds later,
/// undoing the point of this function entirely. Falls back to signaling the process directly
/// for installs that aren't running the watcher under systemd (e.g. a manually started
/// `--watch` process during development).
pub fn stop_background_watcher() {
    if let Ok(status) = Command::new("systemctl")
        .args(["--user", "stop", "soundcore-control-watch.service"])
        .status()
        && status.success()
    {
        tracing::info!("stopped the background Bluetooth watcher via systemctl");
        return;
    }

    for pid in find_own_processes(|second_argument| second_argument == Some("--watch")) {
        // SAFETY: `pid` is a real, currently-running process id just read from `/proc`;
        // sending SIGTERM is a normal signal delivery, not a memory operation. The process
        // could in principle have exited and had its pid reused between the /proc scan and
        // this call; worst case that misdirects a SIGTERM at an unrelated process, not a
        // memory-safety issue.
        if unsafe { libc::kill(pid, libc::SIGTERM) } == 0 {
            tracing::info!(pid, "stopped the background Bluetooth watcher directly");
        }
    }
}

/// Scans `/proc` for other processes running this same executable whose second command-line
/// argument (the first is the binary path) satisfies `matches_args`, ignoring the calling
/// process itself. Shared by `launch_app` (checking for an existing GUI/tray instance) and
/// `stop_background_watcher` (finding the `--watch` instance to signal).
fn find_own_processes(matches_args: impl Fn(Option<&str>) -> bool) -> impl Iterator<Item = i32> {
    let own_pid = std::process::id();
    let exe_name = std::env::current_exe()
        .ok()
        .and_then(|path| path.file_name().map(OsString::from));
    let entries = std::fs::read_dir("/proc").into_iter().flatten();

    entries.flatten().filter_map(move |entry| {
        let pid = entry.file_name().to_str()?.parse::<i32>().ok()?;
        if pid as u32 == own_pid {
            return None;
        }
        let exe_name = exe_name.as_deref()?;
        let cmdline = std::fs::read(entry.path().join("cmdline")).ok()?;
        let mut arguments = cmdline
            .split(|&byte| byte == 0)
            .filter(|argument| !argument.is_empty())
            .map(|argument| String::from_utf8_lossy(argument).into_owned());
        let binary = arguments.next()?;
        if Path::new(&binary).file_name() != Some(exe_name) {
            return None;
        }
        matches_args(arguments.next().as_deref()).then_some(pid)
    })
}
