use std::{
    path::PathBuf,
    sync::{Arc, mpsc},
    thread,
};

use anyhow::{Context, Result, anyhow};
use macaddr::MacAddr6;
use openscq30_lib::{
    OpenSCQ30Session,
    connection::ConnectionDescriptor,
    device::OpenSCQ30Device,
    settings::{Setting, SettingId},
    storage::PairedDevice,
};
use tokio::sync::mpsc as tokio_mpsc;

use crate::devices::{DEVICE_PROFILES, DeviceProfile, find_matching_descriptor};
use crate::domain::{DeviceCommand, DeviceSnapshot, setting_changes, snapshot_from_settings};

#[derive(Clone, Debug, PartialEq)]
pub enum DeviceEvent {
    Searching,
    Connecting {
        name: String,
        icon: Option<&'static [u8]>,
        supports_manual_ambient_ranges: bool,
    },
    Connected {
        name: String,
        snapshot: DeviceSnapshot,
        icon: Option<&'static [u8]>,
        supports_manual_ambient_ranges: bool,
    },
    Snapshot(DeviceSnapshot),
    Error(String),
    Disconnected,
}

pub struct DeviceWorker {
    command_sender: tokio_mpsc::UnboundedSender<DeviceCommand>,
    event_receiver: mpsc::Receiver<DeviceEvent>,
}

impl DeviceWorker {
    #[must_use]
    pub fn spawn() -> Self {
        let (command_sender, command_receiver) = tokio_mpsc::unbounded_channel();
        let (event_sender, event_receiver) = mpsc::channel();

        let thread_events = event_sender.clone();
        if let Err(error) = thread::Builder::new()
            .name("liberty-device-worker".into())
            .spawn(move || {
                let runtime = match tokio::runtime::Runtime::new() {
                    Ok(runtime) => runtime,
                    Err(error) => {
                        let _ = thread_events.send(DeviceEvent::Error(format!(
                            "Could not start Bluetooth worker: {error}"
                        )));
                        return;
                    }
                };
                // `openscq30-lib` panics rather than returning an error for some
                // unsupported per-model setting writes (see DeviceProfile's
                // `supports_manual_ambient_ranges` doc comment). Catch that so an
                // unexpected panic surfaces as a normal error instead of silently
                // killing this thread and leaving the UI stuck.
                let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    runtime.block_on(run_device(command_receiver, &thread_events))
                }));
                match outcome {
                    Ok(Ok(())) => {}
                    Ok(Err(error)) => {
                        let _ = thread_events.send(DeviceEvent::Error(format!("{error:#}")));
                    }
                    Err(panic) => {
                        let message = panic_message(&panic);
                        tracing::error!(message, "Bluetooth worker panicked");
                        let _ = thread_events.send(DeviceEvent::Error(format!(
                            "The Bluetooth worker crashed unexpectedly: {message}"
                        )));
                    }
                }
            })
        {
            let _ = event_sender.send(DeviceEvent::Error(format!(
                "Could not create Bluetooth worker thread: {error}"
            )));
        }

        Self {
            command_sender,
            event_receiver,
        }
    }

    pub fn send(&self, command: DeviceCommand) -> Result<()> {
        self.command_sender
            .send(command)
            .map_err(|_| anyhow!("the Bluetooth worker is not running"))
    }

    pub fn try_recv(&self) -> Option<DeviceEvent> {
        self.event_receiver.try_recv().ok()
    }
}

fn panic_message(payload: &(dyn std::any::Any + Send)) -> String {
    if let Some(message) = payload.downcast_ref::<&str>() {
        (*message).to_owned()
    } else if let Some(message) = payload.downcast_ref::<String>() {
        message.clone()
    } else {
        "unknown panic".to_owned()
    }
}

async fn run_device(
    mut commands: tokio_mpsc::UnboundedReceiver<DeviceCommand>,
    events: &mpsc::Sender<DeviceEvent>,
) -> Result<()> {
    events.send(DeviceEvent::Searching).ok();
    let session = OpenSCQ30Session::new(database_path()?)
        .await
        .context("could not initialize app storage")?;
    let (descriptor, profile) = find_target_device(&session).await?;
    events
        .send(DeviceEvent::Connecting {
            name: profile.display_name.to_owned(),
            icon: profile.icon,
            supports_manual_ambient_ranges: profile.supports_manual_ambient_ranges,
        })
        .ok();

    session
        .pair(PairedDevice {
            mac_address: descriptor.mac_address,
            model: profile.model,
            is_demo: false,
        })
        .await
        .with_context(|| format!("could not register the {}", profile.display_name))?;

    let device = session
        .connect(descriptor.mac_address)
        .await
        .context("could not open the Soundcore RFCOMM service")?;
    initialize_ambient_level(device.as_ref(), events, profile).await;
    let snapshot = read_snapshot(device.as_ref());
    events
        .send(DeviceEvent::Connected {
            name: profile.display_name.to_owned(),
            snapshot,
            icon: profile.icon,
            supports_manual_ambient_ranges: profile.supports_manual_ambient_ranges,
        })
        .ok();

    event_loop(device, descriptor.mac_address, &mut commands, events).await
}

/// Tries each known device profile in turn, querying BlueZ for devices of that model until
/// one matches (by configured MAC address, or by the profile's expected Bluetooth name).
async fn find_target_device(
    session: &OpenSCQ30Session,
) -> Result<(ConnectionDescriptor, &'static DeviceProfile)> {
    let configured_mac = configured_mac_address()?;

    for profile in DEVICE_PROFILES {
        let descriptors = session
            .list_devices(profile.model)
            .await
            .context("could not query connected Bluetooth devices")?;
        if let Some(descriptor) = find_matching_descriptor(&descriptors, profile, configured_mac) {
            return Ok((descriptor, profile));
        }
    }

    if let Some(mac_address) = configured_mac {
        return Err(anyhow!("configured device {mac_address} is not connected"));
    }
    let supported = DEVICE_PROFILES
        .iter()
        .map(|profile| profile.display_name)
        .collect::<Vec<_>>()
        .join(", ");
    Err(anyhow!(
        "No supported earbuds were found. Connect one of the following in Bluetooth settings, \
         then reopen the app: {supported}"
    ))
}

async fn event_loop(
    device: Arc<dyn OpenSCQ30Device + Send + Sync>,
    mac_address: MacAddr6,
    commands: &mut tokio_mpsc::UnboundedReceiver<DeviceCommand>,
    events: &mpsc::Sender<DeviceEvent>,
) -> Result<()> {
    let mut changes = device.watch_for_changes();
    let mut connection = device.connection_status();

    loop {
        tokio::select! {
            command = commands.recv() => {
                let Some(command) = command else { return Ok(()); };
                if matches!(command, DeviceCommand::Disconnect) {
                    match disconnect_bluetooth(mac_address).await {
                        Ok(()) => {
                            events.send(DeviceEvent::Disconnected).ok();
                            return Ok(());
                        }
                        Err(error) => {
                            events.send(DeviceEvent::Error(format!("Could not disconnect the earbuds: {error:#}"))).ok();
                        }
                    }
                    continue;
                }
                for change in setting_changes(command) {
                    if let Err(error) = device.set_setting_values(vec![change]).await {
                        events.send(DeviceEvent::Error(format!("Could not update the earbuds: {error}"))).ok();
                        break;
                    }
                }
            }
            changed = changes.changed() => {
                if changed.is_err() {
                    events.send(DeviceEvent::Disconnected).ok();
                    return Ok(());
                }
                events.send(DeviceEvent::Snapshot(read_snapshot(device.as_ref()))).ok();
            }
            changed = connection.changed() => {
                if changed.is_err() || matches!(*connection.borrow(), openscq30_lib::connection::ConnectionStatus::Disconnected) {
                    events.send(DeviceEvent::Disconnected).ok();
                    return Ok(());
                }
            }
        }
    }
}

async fn initialize_ambient_level(
    device: &dyn OpenSCQ30Device,
    events: &mpsc::Sender<DeviceEvent>,
    profile: &DeviceProfile,
) {
    if !profile.supports_manual_ambient_ranges {
        return;
    }
    let mode = match device.setting(&SettingId::AmbientSoundMode) {
        Some(Setting::Select { value, .. }) => value,
        _ => return,
    };
    let manual_noise_canceling = device
        .setting(&SettingId::ManualNoiseCanceling)
        .and_then(setting_range_value);
    let manual_transparency = device
        .setting(&SettingId::ManualTransparency)
        .and_then(setting_range_value);

    let level = match mode.as_ref() {
        "NoiseCanceling" if manual_noise_canceling.is_none() => {
            Some((manual_transparency.unwrap_or(5) + 5).clamp(6, 10) as u8)
        }
        "Transparency" if manual_transparency.is_none() => {
            Some((6 - manual_noise_canceling.unwrap_or(5)).clamp(1, 5) as u8)
        }
        _ => None,
    };
    let Some(level) = level else { return };

    tracing::info!(level, mode = %mode, "initializing missing ambient level");
    for change in setting_changes(DeviceCommand::SetAmbientLevel(level)) {
        if let Err(error) = device.set_setting_values(vec![change]).await {
            events
                .send(DeviceEvent::Error(format!(
                    "Could not initialize the ambient level: {error}"
                )))
                .ok();
            return;
        }
    }
}

fn setting_range_value(setting: Setting) -> Option<i32> {
    match setting {
        Setting::I32Range { value, .. } => Some(value),
        _ => None,
    }
}

async fn disconnect_bluetooth(mac_address: MacAddr6) -> Result<()> {
    let session = bluer::Session::new()
        .await
        .context("could not connect to BlueZ")?;
    let adapter = session
        .default_adapter()
        .await
        .context("could not find the Bluetooth adapter")?;
    let device = adapter
        .device(mac_address.into())
        .context("could not access the earbuds in BlueZ")?;
    device
        .disconnect()
        .await
        .context("BlueZ could not disconnect the earbuds")
}

fn read_snapshot(device: &dyn OpenSCQ30Device) -> DeviceSnapshot {
    for setting_id in [
        SettingId::BatteryLevelLeft,
        SettingId::BatteryLevelRight,
        SettingId::CaseBatteryLevel,
        SettingId::AmbientSoundMode,
        SettingId::ManualNoiseCanceling,
        SettingId::ManualTransparency,
        SettingId::PresetEqualizerProfile,
        SettingId::CustomEqualizerProfile,
    ] {
        tracing::debug!(?setting_id, setting = ?device.setting(&setting_id), "device setting");
    }
    snapshot_from_settings(|setting_id: SettingId| device.setting(&setting_id))
}

fn database_path() -> Result<PathBuf> {
    let directory = dirs::data_local_dir()
        .ok_or_else(|| anyhow!("could not locate the local data directory"))?
        .join("liberty-control");
    std::fs::create_dir_all(&directory)
        .with_context(|| format!("could not create {}", directory.display()))?;
    Ok(directory.join("devices.sqlite3"))
}

pub(crate) fn configured_mac_address() -> Result<Option<MacAddr6>> {
    std::env::var("LIBERTY_CONTROL_MAC")
        .ok()
        .map(|value| {
            value
                .parse()
                .with_context(|| format!("invalid LIBERTY_CONTROL_MAC value: {value}"))
        })
        .transpose()
}
