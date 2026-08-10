use std::{
    path::PathBuf,
    sync::{Arc, mpsc},
    thread,
};

use anyhow::{Context, Result, anyhow};
use macaddr::MacAddr6;
use openscq30_lib::{
    DeviceModel, OpenSCQ30Session,
    device::OpenSCQ30Device,
    settings::{Setting, SettingId},
    storage::PairedDevice,
};
use tokio::sync::mpsc as tokio_mpsc;

use crate::domain::{DeviceCommand, DeviceSnapshot, setting_changes, snapshot_from_settings};

const DEVICE_NAME: &str = "soundcore Liberty 4 Pro";

#[derive(Clone, Debug, PartialEq)]
pub enum DeviceEvent {
    Searching,
    Connecting {
        name: String,
    },
    Connected {
        name: String,
        snapshot: DeviceSnapshot,
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
                if let Err(error) = runtime.block_on(run_device(command_receiver, &thread_events)) {
                    let _ = thread_events.send(DeviceEvent::Error(format!("{error:#}")));
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

async fn run_device(
    mut commands: tokio_mpsc::UnboundedReceiver<DeviceCommand>,
    events: &mpsc::Sender<DeviceEvent>,
) -> Result<()> {
    events.send(DeviceEvent::Searching).ok();
    let session = OpenSCQ30Session::new(database_path()?)
        .await
        .context("could not initialize app storage")?;
    let descriptor = find_liberty_4_pro(&session).await?;
    events
        .send(DeviceEvent::Connecting {
            name: descriptor.name.clone(),
        })
        .ok();

    session
        .pair(PairedDevice {
            mac_address: descriptor.mac_address,
            model: DeviceModel::SoundcoreA3954,
            is_demo: false,
        })
        .await
        .context("could not register the Liberty 4 Pro")?;

    let device = session
        .connect(descriptor.mac_address)
        .await
        .context("could not open the Soundcore RFCOMM service")?;
    initialize_ambient_level(device.as_ref(), events).await;
    let snapshot = read_snapshot(device.as_ref());
    events
        .send(DeviceEvent::Connected {
            name: descriptor.name,
            snapshot,
        })
        .ok();

    event_loop(device, descriptor.mac_address, &mut commands, events).await
}

async fn find_liberty_4_pro(
    session: &OpenSCQ30Session,
) -> Result<openscq30_lib::connection::ConnectionDescriptor> {
    let devices = session
        .list_devices(DeviceModel::SoundcoreA3954)
        .await
        .context("could not query connected Bluetooth devices")?;

    if let Some(mac_address) = configured_mac_address()? {
        return devices
            .into_iter()
            .find(|device| device.mac_address == mac_address)
            .ok_or_else(|| anyhow!("configured device {mac_address} is not connected"));
    }

    devices
        .into_iter()
        .find(|device| device.name.eq_ignore_ascii_case(DEVICE_NAME))
        .ok_or_else(|| {
            anyhow!(
                "Liberty 4 Pro was not found. Connect it in Bluetooth settings, then reopen the app"
            )
        })
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
) {
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
        .context("could not access the Liberty 4 Pro in BlueZ")?;
    device
        .disconnect()
        .await
        .context("BlueZ could not disconnect the Liberty 4 Pro")
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

fn configured_mac_address() -> Result<Option<MacAddr6>> {
    std::env::var("LIBERTY_CONTROL_MAC")
        .ok()
        .map(|value| {
            value
                .parse()
                .with_context(|| format!("invalid LIBERTY_CONTROL_MAC value: {value}"))
        })
        .transpose()
}
