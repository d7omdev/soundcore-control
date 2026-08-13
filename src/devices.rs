use std::{str::FromStr, sync::Arc, sync::LazyLock};

use openscq30_lib::{DeviceModel, connection::ConnectionDescriptor};
use serde::Deserialize;

/// A supported Soundcore model: its `openscq30-lib` identity, how it names itself over
/// Bluetooth, and how the UI should present it.
#[derive(Debug, Clone, PartialEq)]
pub struct DeviceProfile {
    pub model: DeviceModel,
    /// Matched case-insensitively against the BlueZ-advertised device name.
    pub bluetooth_name: String,
    pub display_name: String,
    /// PNG bytes for the in-app illustration, or `None` to fall back to the generic
    /// vector-drawn placeholder.
    pub icon: Option<Arc<[u8]>>,
    /// Whether this model exposes separate `ManualNoiseCanceling`/`ManualTransparency`
    /// numeric-range settings (as the Liberty 4 Pro does), which is what the ambient-level
    /// startup repair in `device::initialize_ambient_level` writes to. Devices without
    /// these settings must not run that repair: `openscq30-lib`'s `set_setting_values`
    /// panics (`unwrap()` on `None`) when asked to write a `SettingId` the device's
    /// settings manager doesn't register, rather than returning an error. Defaults to
    /// `false` (skip the repair) for any unverified model.
    pub supports_manual_ambient_ranges: bool,
}

#[derive(Debug, Deserialize)]
struct RawCatalog {
    device: Vec<RawDeviceProfile>,
}

#[derive(Debug, Deserialize)]
struct RawDeviceProfile {
    model: String,
    bluetooth_name: String,
    display_name: String,
    icon: Option<String>,
    supports_manual_ambient_ranges: bool,
}

/// Device icon PNGs, named in `assets/devices.toml`'s `icon` field.
#[derive(rust_embed::RustEmbed)]
#[folder = "assets/icons/"]
struct Icons;

/// Every Soundcore model this app knows how to drive, loaded from `assets/devices.toml`.
/// `bluetooth_name` for every entry besides the Liberty 4 Pro and R60i NC is an unverified
/// guess following the `"soundcore <Product>"` convention and needs confirming against
/// real hardware.
pub static DEVICE_PROFILES: LazyLock<Vec<DeviceProfile>> = LazyLock::new(load_device_profiles);

fn load_device_profiles() -> Vec<DeviceProfile> {
    let catalog: RawCatalog = toml::from_str(include_str!("../assets/devices.toml"))
        .expect("assets/devices.toml is bundled with the app and must parse");

    catalog
        .device
        .into_iter()
        .map(|raw| {
            let model = DeviceModel::from_str(&raw.model).unwrap_or_else(|_| {
                panic!(
                    "assets/devices.toml: '{}' is not a known openscq30-lib DeviceModel",
                    raw.model
                )
            });
            let icon = raw.icon.map(|name| {
                let file = Icons::get(&name).unwrap_or_else(|| {
                    panic!("assets/devices.toml: icon '{name}' not found in assets/icons/")
                });
                Arc::from(file.data.into_owned())
            });
            DeviceProfile {
                model,
                bluetooth_name: raw.bluetooth_name,
                display_name: raw.display_name,
                icon,
                supports_manual_ambient_ranges: raw.supports_manual_ambient_ranges,
            }
        })
        .collect()
}

/// Picks the descriptor that matches `profile` out of a batch returned by
/// `OpenSCQ30Session::list_devices`: by MAC address when one is configured, otherwise by
/// the profile's expected Bluetooth name.
pub fn find_matching_descriptor(
    descriptors: &[ConnectionDescriptor],
    profile: &DeviceProfile,
    configured_mac: Option<macaddr::MacAddr6>,
) -> Option<ConnectionDescriptor> {
    if let Some(configured_mac) = configured_mac {
        return descriptors
            .iter()
            .find(|descriptor| descriptor.mac_address == configured_mac)
            .cloned();
    }
    descriptors
        .iter()
        .find(|descriptor| {
            descriptor
                .name
                .eq_ignore_ascii_case(&profile.bluetooth_name)
        })
        .cloned()
}

/// Whether `name` (as advertised over Bluetooth) matches any known device profile.
pub fn matches_known_profile(name: &str) -> bool {
    DEVICE_PROFILES
        .iter()
        .any(|profile| name.eq_ignore_ascii_case(&profile.bluetooth_name))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn descriptor(name: &str, mac: &str) -> ConnectionDescriptor {
        ConnectionDescriptor {
            name: name.to_owned(),
            mac_address: mac.parse().unwrap(),
        }
    }

    #[test]
    fn matches_by_name_case_insensitively() {
        let profile = &DEVICE_PROFILES[0];
        let descriptors = vec![descriptor("SOUNDCORE LIBERTY 4 PRO", "AA:BB:CC:DD:EE:FF")];

        let found = find_matching_descriptor(&descriptors, profile, None);

        assert_eq!(found, Some(descriptors[0].clone()));
    }

    #[test]
    fn ignores_name_when_mac_is_configured() {
        let profile = &DEVICE_PROFILES[0];
        let configured_mac = "11:22:33:44:55:66".parse().unwrap();
        let descriptors = vec![
            descriptor("soundcore Liberty 4 Pro", "AA:BB:CC:DD:EE:FF"),
            descriptor("some other name", "11:22:33:44:55:66"),
        ];

        let found = find_matching_descriptor(&descriptors, profile, Some(configured_mac));

        assert_eq!(found, Some(descriptors[1].clone()));
    }

    #[test]
    fn returns_none_when_nothing_matches() {
        let profile = &DEVICE_PROFILES[0];
        let descriptors = vec![descriptor("unrelated device", "AA:BB:CC:DD:EE:FF")];

        assert_eq!(find_matching_descriptor(&descriptors, profile, None), None);
    }

    #[test]
    fn recognizes_every_registered_profile_name() {
        for profile in DEVICE_PROFILES.iter() {
            assert!(matches_known_profile(&profile.bluetooth_name));
            assert!(matches_known_profile(
                &profile.bluetooth_name.to_uppercase()
            ));
        }
        assert!(!matches_known_profile("totally unrelated device"));
    }

    #[test]
    fn loads_icons_for_entries_that_declare_one() {
        let liberty = DEVICE_PROFILES
            .iter()
            .find(|profile| profile.display_name == "Liberty 4 Pro")
            .unwrap();
        assert!(liberty.icon.is_some());

        let p20i = DEVICE_PROFILES
            .iter()
            .find(|profile| profile.display_name == "P20i")
            .unwrap();
        assert!(p20i.icon.is_none());
    }
}
