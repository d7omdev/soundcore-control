//! Hardware probe for equalizer writes on an aliased model (e.g. K20i mapped to A3949).
//!
//! Connects to one device by MAC, reads the equalizer state, switches to a different preset,
//! waits for the earbuds to report back, prints what changed, then restores the original
//! preset. Close the GUI first: the earbuds accept a single RFCOMM connection.
//!
//! ```text
//! RUST_LOG=debug cargo run --release --example eq_probe -- AA:BB:CC:DD:EE:FF SoundcoreA3949 BassBooster
//! ```

use std::{borrow::Cow, time::Duration};

use anyhow::{Context, Result, anyhow};
use openscq30_lib::{
    DeviceModel, OpenSCQ30Session,
    device::OpenSCQ30Device,
    settings::{Setting, SettingId, Value},
    storage::PairedDevice,
};

fn describe(device: &dyn OpenSCQ30Device) -> String {
    let preset = match device.setting(&SettingId::PresetEqualizerProfile) {
        Some(Setting::PresetEqualizerProfileSelect { value, .. }) => value.map(Cow::into_owned),
        _ => None,
    };
    let bands = match device.setting(&SettingId::VolumeAdjustments) {
        Some(Setting::Equalizer { value, .. }) => Some(value),
        _ => None,
    };
    format!("preset={preset:?} bands={bands:?}")
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let mut args = std::env::args().skip(1);
    let mac = args
        .next()
        .context("usage: eq_probe <MAC> <DeviceModel> <preset>")?;
    let model: DeviceModel = args
        .next()
        .context("missing DeviceModel")?
        .parse()
        .map_err(|_| anyhow!("unknown DeviceModel"))?;
    let target = args.next().context("missing preset name")?;

    // Throwaway database so the probe never touches the app's paired-device data.
    let database = std::env::temp_dir().join("soundcore-eq-probe.sqlite3");
    let session = OpenSCQ30Session::new(database).await?;
    session
        .pair(PairedDevice {
            mac_address: mac.parse().context("invalid MAC")?,
            model,
            is_demo: false,
        })
        .await?;
    let device = session.connect(mac.parse()?).await?;

    for id in [
        SettingId::PresetEqualizerProfile,
        SettingId::CustomEqualizerProfile,
        SettingId::VolumeAdjustments,
    ] {
        println!("raw {id:?}: {:?}", device.setting(&id));
    }
    let before = describe(device.as_ref());
    println!("before : {before}");
    let original = match device.setting(&SettingId::PresetEqualizerProfile) {
        Some(Setting::PresetEqualizerProfileSelect {
            value: Some(value), ..
        }) => value.into_owned(),
        _ => {
            return Err(anyhow!(
                "device reports no current preset; refusing to write"
            ));
        }
    };
    if original == target {
        return Err(anyhow!("already on '{target}'; pick a different preset"));
    }

    let result = write_and_report(device.as_ref(), &target).await;
    // Always try to put the original preset back, even if the write above failed.
    let restore = device
        .set_setting_values(vec![(
            SettingId::PresetEqualizerProfile,
            Value::String(Cow::Owned(original.clone())),
        )])
        .await;
    tokio::time::sleep(Duration::from_secs(2)).await;
    println!(
        "restored: {} (restore result: {restore:?})",
        describe(device.as_ref())
    );
    result
}

async fn write_and_report(device: &dyn OpenSCQ30Device, target: &str) -> Result<()> {
    device
        .set_setting_values(vec![(
            SettingId::PresetEqualizerProfile,
            Value::String(Cow::Owned(target.to_owned())),
        )])
        .await
        .context("device rejected the preset write")?;
    tokio::time::sleep(Duration::from_secs(2)).await;
    println!("after  : {}", describe(device));
    Ok(())
}
