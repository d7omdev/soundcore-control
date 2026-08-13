use std::{borrow::Cow, collections::HashMap};

use openscq30_lib::settings::{Equalizer, Range, Select, Setting, SettingId, Value};
use pretty_assertions::assert_eq;
use soundcore_control::domain::{
    DeviceCommand, ListeningMode, listening_mode_value, setting_changes, snapshot_from_settings,
};

fn battery_value(current: u8, maximum: u8) -> Setting {
    Setting::Information {
        value: format!("{current}/{maximum}"),
        translated_value: format!("{}%", u16::from(current) * 100 / u16::from(maximum)),
    }
}

fn select(options: &[(&'static str, &str)], value: &'static str) -> Setting {
    Setting::Select {
        setting: Select {
            options: options.iter().map(|(raw, _)| Cow::Borrowed(*raw)).collect(),
            localized_options: options
                .iter()
                .map(|(_, label)| (*label).to_owned())
                .collect(),
        },
        value: Cow::Borrowed(value),
    }
}

#[test]
fn maps_protocol_values_for_each_listening_mode() {
    assert_eq!(
        listening_mode_value(ListeningMode::NoiseCanceling),
        "NoiseCanceling"
    );
    assert_eq!(
        listening_mode_value(ListeningMode::Transparency),
        "Transparency"
    );
    assert_eq!(listening_mode_value(ListeningMode::Normal), "Normal");
}

#[test]
fn maps_ui_commands_to_protocol_setting_changes() {
    assert_eq!(
        setting_changes(DeviceCommand::SetListeningMode(
            ListeningMode::NoiseCanceling,
        )),
        vec![(
            SettingId::AmbientSoundMode,
            Value::String(Cow::Borrowed("NoiseCanceling")),
        )]
    );
    assert_eq!(
        setting_changes(DeviceCommand::SetPreset("BassBooster".into())),
        vec![(
            SettingId::PresetEqualizerProfile,
            Value::String(Cow::Owned("BassBooster".into())),
        )]
    );
    assert_eq!(
        setting_changes(DeviceCommand::SetEqualizer(vec![0, 10, -10])),
        vec![(
            SettingId::VolumeAdjustments,
            Value::I16Vec(vec![0, 10, -10]),
        )]
    );
    assert_eq!(
        setting_changes(DeviceCommand::SetAmbientLevel(1)),
        vec![
            (
                SettingId::AmbientSoundMode,
                Value::String(Cow::Borrowed("Transparency")),
            ),
            (SettingId::ManualTransparency, Value::I32(5)),
        ]
    );
    assert_eq!(
        setting_changes(DeviceCommand::SetAmbientLevel(10)),
        vec![
            (
                SettingId::AmbientSoundMode,
                Value::String(Cow::Borrowed("NoiseCanceling")),
            ),
            (SettingId::ManualNoiseCanceling, Value::I32(5)),
        ]
    );
}

#[test]
fn extracts_core_controls_from_device_settings() {
    let mut settings = HashMap::from([
        (SettingId::BatteryLevelLeft, battery_value(90, 100)),
        (SettingId::BatteryLevelRight, battery_value(80, 100)),
        (SettingId::CaseBatteryLevel, battery_value(7, 10)),
        (
            SettingId::AmbientSoundMode,
            select(
                &[
                    ("NoiseCanceling", "Noise canceling"),
                    ("Transparency", "Transparency"),
                    ("Normal", "Normal"),
                ],
                "Transparency",
            ),
        ),
    ]);
    settings.insert(
        SettingId::ManualTransparency,
        Setting::I32Range {
            setting: Range {
                range: 1..=5,
                step: 1,
            },
            value: 5,
        },
    );
    settings.insert(
        SettingId::PresetEqualizerProfile,
        Setting::PresetEqualizerProfileSelect {
            equalizer: Equalizer {
                band_hz: Cow::Borrowed(&[100, 200, 400, 800, 1600, 3200, 6400, 12800]),
                fraction_digits: 1,
                min: -60,
                max: 60,
            },
            select: Select {
                options: vec![
                    Cow::Borrowed("SoundcoreSignature"),
                    Cow::Borrowed("BassBooster"),
                ],
                localized_options: vec!["Soundcore Signature".into(), "Bass Booster".into()],
            },
            presets: vec![vec![0; 8], vec![30; 8]],
            value: Some(Cow::Borrowed("BassBooster")),
        },
    );
    settings.insert(
        SettingId::VolumeAdjustments,
        Setting::Equalizer {
            setting: Equalizer {
                band_hz: Cow::Borrowed(&[100, 200, 400, 800, 1600, 3200, 6400, 12800]),
                fraction_digits: 1,
                min: -60,
                max: 60,
            },
            read_only: false,
            value: vec![0, 10, 20, 30, 20, 10, 0, -10],
        },
    );

    let snapshot = snapshot_from_settings(|id| settings.get(&id).cloned());

    assert_eq!(snapshot.battery_left, Some(90));
    assert_eq!(snapshot.battery_right, Some(80));
    assert_eq!(snapshot.battery_case, Some(70));
    assert_eq!(snapshot.listening_mode, ListeningMode::Transparency);
    assert_eq!(snapshot.ambient_level, Some(1));
    assert_eq!(snapshot.mode_options.len(), 3);
    assert_eq!(snapshot.selected_preset.as_deref(), Some("BassBooster"));
    assert_eq!(snapshot.preset_options[1].label, "Bass Booster");
    assert_eq!(snapshot.equalizer.as_ref().unwrap().gains[3], 30);
    assert_eq!(
        snapshot.equalizer.as_ref().unwrap().frequencies_hz[7],
        12800
    );
}

#[test]
fn extracts_daily_and_earbud_controls() {
    let settings = HashMap::from([
        (SettingId::EasyChat, Setting::Toggle { value: true }),
        (
            SettingId::EasyChatWaitTime,
            select(&[("5s", "5 seconds"), ("10s", "10 seconds")], "10s"),
        ),
        (
            SettingId::LeftSinglePress,
            Setting::OptionalSelect {
                setting: Select {
                    options: vec![Cow::Borrowed("PlayPause"), Cow::Borrowed("VoiceAssistant")],
                    localized_options: vec!["Play / Pause".into(), "Voice Assistant".into()],
                },
                value: Some(Cow::Borrowed("PlayPause")),
            },
        ),
    ]);

    let snapshot = snapshot_from_settings(|id| settings.get(&id).cloned());

    assert_eq!(snapshot.daily_controls.len(), 2);
    assert_eq!(snapshot.daily_controls[0].label, "Easy Chat");
    assert_eq!(snapshot.earbud_controls.len(), 1);
    assert_eq!(snapshot.earbud_controls[0].label, "Left Single Press");
}

#[test]
fn missing_settings_produce_safe_defaults() {
    let snapshot = snapshot_from_settings(|_| None);

    assert_eq!(snapshot, Default::default());
}
