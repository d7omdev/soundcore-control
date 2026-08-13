use openscq30_lib::{DeviceModel, connection::ConnectionDescriptor};

/// A supported Soundcore model: its `openscq30-lib` identity, how it names itself over
/// Bluetooth, and how the UI should present it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeviceProfile {
    pub model: DeviceModel,
    /// Matched case-insensitively against the BlueZ-advertised device name.
    pub bluetooth_name: &'static str,
    pub display_name: &'static str,
    /// Embedded PNG bytes for the in-app illustration, or `None` to fall back to the
    /// generic vector-drawn placeholder.
    pub icon: Option<&'static [u8]>,
}

/// Every Soundcore model this app knows how to drive. `bluetooth_name` for every entry
/// besides the Liberty 4 Pro is an unverified guess following the `"soundcore <Product>"`
/// convention and needs confirming against real hardware.
pub const DEVICE_PROFILES: &[DeviceProfile] = &[
    DeviceProfile {
        model: DeviceModel::SoundcoreA3954,
        bluetooth_name: "soundcore Liberty 4 Pro",
        display_name: "Liberty 4 Pro",
        icon: Some(include_bytes!("../assets/liberty4pro.png")),
    },
    DeviceProfile {
        model: DeviceModel::SoundcoreD1202C,
        bluetooth_name: "soundcore R60i NC",
        display_name: "R60i NC",
        icon: Some(include_bytes!("../assets/r60inc.png")),
    },
    DeviceProfile {
        model: DeviceModel::SoundcoreA3949,
        bluetooth_name: "soundcore P20i",
        display_name: "P20i",
        icon: None,
    },
    DeviceProfile {
        model: DeviceModel::SoundcoreA3028,
        bluetooth_name: "soundcore Life Q30",
        display_name: "Life Q30",
        icon: None,
    },
    DeviceProfile {
        model: DeviceModel::SoundcoreA3062,
        bluetooth_name: "soundcore Space One Pro",
        display_name: "Space One Pro",
        icon: None,
    },
];

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
        .find(|descriptor| descriptor.name.eq_ignore_ascii_case(profile.bluetooth_name))
        .cloned()
}

/// Whether `name` (as advertised over Bluetooth) matches any known device profile.
pub fn matches_known_profile(name: &str) -> bool {
    DEVICE_PROFILES
        .iter()
        .any(|profile| name.eq_ignore_ascii_case(profile.bluetooth_name))
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
        for profile in DEVICE_PROFILES {
            assert!(matches_known_profile(profile.bluetooth_name));
            assert!(matches_known_profile(
                &profile.bluetooth_name.to_uppercase()
            ));
        }
        assert!(!matches_known_profile("totally unrelated device"));
    }
}
