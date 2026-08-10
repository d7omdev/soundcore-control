use std::{borrow::Cow, collections::HashMap};

use liberty_control::domain::{ListeningMode, snapshot_from_settings};
use openscq30_lib::settings::{Range, Select, Setting, SettingId};

#[test]
fn keeps_hardware_level_available_when_starting_in_normal_mode() {
    let settings = HashMap::from([
        (
            SettingId::AmbientSoundMode,
            Setting::Select {
                setting: Select {
                    options: vec![
                        Cow::Borrowed("NoiseCanceling"),
                        Cow::Borrowed("Transparency"),
                        Cow::Borrowed("Normal"),
                    ],
                    localized_options: vec![
                        "Noise Canceling".into(),
                        "Transparency".into(),
                        "Normal".into(),
                    ],
                },
                value: Cow::Borrowed("Normal"),
            },
        ),
        (
            SettingId::ManualTransparency,
            Setting::I32Range {
                setting: Range {
                    range: 1..=5,
                    step: 1,
                },
                value: 3,
            },
        ),
    ]);

    let snapshot = snapshot_from_settings(|id| settings.get(&id).cloned());

    assert_eq!(snapshot.listening_mode, ListeningMode::Normal);
    assert_eq!(snapshot.ambient_level, Some(3));
}

#[test]
fn shows_hardware_level_when_mode_and_manual_range_are_temporarily_inconsistent() {
    let settings = HashMap::from([
        (
            SettingId::AmbientSoundMode,
            Setting::Select {
                setting: Select {
                    options: vec![
                        Cow::Borrowed("NoiseCanceling"),
                        Cow::Borrowed("Transparency"),
                        Cow::Borrowed("Normal"),
                    ],
                    localized_options: vec![
                        "Noise Canceling".into(),
                        "Transparency".into(),
                        "Normal".into(),
                    ],
                },
                value: Cow::Borrowed("NoiseCanceling"),
            },
        ),
        (
            SettingId::ManualTransparency,
            Setting::I32Range {
                setting: Range {
                    range: 1..=5,
                    step: 1,
                },
                value: 5,
            },
        ),
    ]);

    let snapshot = snapshot_from_settings(|id| settings.get(&id).cloned());

    assert_eq!(snapshot.listening_mode, ListeningMode::NoiseCanceling);
    assert_eq!(snapshot.ambient_level, Some(10));
}
