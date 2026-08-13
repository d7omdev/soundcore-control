use std::borrow::Cow;

use openscq30_i18n::Translate;
use openscq30_lib::settings::{Setting, SettingId, Value};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ListeningMode {
    NoiseCanceling,
    Transparency,
    #[default]
    Normal,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SelectOption {
    pub value: String,
    pub label: String,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct EqualizerState {
    pub frequencies_hz: Vec<u16>,
    pub gains: Vec<i16>,
    pub min: i16,
    pub max: i16,
    pub fraction_digits: i16,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ControlValue {
    Toggle(bool),
    Select {
        value: Option<String>,
        options: Vec<SelectOption>,
        optional: bool,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct ControlSetting {
    pub id: SettingId,
    pub label: String,
    pub value: ControlValue,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct DeviceSnapshot {
    pub battery_left: Option<u8>,
    pub battery_right: Option<u8>,
    pub battery_case: Option<u8>,
    pub listening_mode: ListeningMode,
    pub ambient_level: Option<u8>,
    pub mode_options: Vec<SelectOption>,
    pub selected_preset: Option<String>,
    pub preset_options: Vec<SelectOption>,
    pub equalizer: Option<EqualizerState>,
    pub daily_controls: Vec<ControlSetting>,
    pub earbud_controls: Vec<ControlSetting>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum DeviceCommand {
    Disconnect,
    SetListeningMode(ListeningMode),
    SetAmbientLevel(u8),
    SetPreset(String),
    SetEqualizer(Vec<i16>),
    SetToggle(SettingId, bool),
    SetSelect {
        id: SettingId,
        value: Option<String>,
        optional: bool,
    },
}

pub fn setting_changes(command: DeviceCommand) -> Vec<(SettingId, Value)> {
    match command {
        DeviceCommand::Disconnect => Vec::new(),
        DeviceCommand::SetListeningMode(mode) => vec![(
            SettingId::AmbientSoundMode,
            Value::String(Cow::Borrowed(listening_mode_value(mode))),
        )],
        DeviceCommand::SetAmbientLevel(level) => ambient_level_changes(level),
        DeviceCommand::SetPreset(value) => vec![(
            SettingId::PresetEqualizerProfile,
            Value::String(Cow::Owned(value)),
        )],
        DeviceCommand::SetEqualizer(gains) => {
            vec![(SettingId::VolumeAdjustments, Value::I16Vec(gains))]
        }
        DeviceCommand::SetToggle(id, value) => vec![(id, Value::Bool(value))],
        DeviceCommand::SetSelect {
            id,
            value,
            optional,
        } => {
            let value = if optional {
                Value::OptionalString(value.map(Cow::Owned))
            } else {
                Value::String(Cow::Owned(value.unwrap_or_default()))
            };
            vec![(id, value)]
        }
    }
}

fn ambient_level_changes(level: u8) -> Vec<(SettingId, Value)> {
    let level = level.clamp(1, 10);
    if level <= 5 {
        vec![
            (
                SettingId::AmbientSoundMode,
                Value::String(Cow::Borrowed("Transparency")),
            ),
            (
                SettingId::ManualTransparency,
                Value::I32(i32::from(6 - level)),
            ),
        ]
    } else {
        vec![
            (
                SettingId::AmbientSoundMode,
                Value::String(Cow::Borrowed("NoiseCanceling")),
            ),
            (
                SettingId::ManualNoiseCanceling,
                Value::I32(i32::from(level - 5)),
            ),
        ]
    }
}

const DAILY_CONTROLS: &[SettingId] = &[
    SettingId::WindNoiseSuppression,
    SettingId::WearingDetection,
    SettingId::EasyChat,
    SettingId::EasyChatWaitTime,
    SettingId::AutoPowerOff,
    SettingId::Ldac,
    SettingId::SoundLeakCompensation,
];

const EARBUD_CONTROLS: &[SettingId] = &[
    SettingId::LeftSinglePress,
    SettingId::LeftDoublePress,
    SettingId::LeftTriplePress,
    SettingId::LeftLongPress,
    SettingId::LeftSlideUp,
    SettingId::LeftSlideDown,
    SettingId::RightSinglePress,
    SettingId::RightDoublePress,
    SettingId::RightTriplePress,
    SettingId::RightLongPress,
    SettingId::RightSlideUp,
    SettingId::RightSlideDown,
];

/// Builds controls with labels from `openscq30-lib`'s own translations (`SettingId::translate`),
/// so daily/earbud control names follow the system locale instead of being hardcoded English.
fn collect_controls(
    setting: &impl Fn(SettingId) -> Option<Setting>,
    definitions: &[SettingId],
) -> Vec<ControlSetting> {
    definitions
        .iter()
        .filter_map(|id| {
            control_value(setting(*id)?).map(|value| ControlSetting {
                id: *id,
                label: id.translate(),
                value,
            })
        })
        .collect()
}

fn control_value(setting: Setting) -> Option<ControlValue> {
    match setting {
        Setting::Toggle { value } => Some(ControlValue::Toggle(value)),
        Setting::Select { setting, value } => Some(ControlValue::Select {
            value: Some(value.into_owned()),
            options: select_options(setting),
            optional: false,
        }),
        Setting::OptionalSelect { setting, value } => Some(ControlValue::Select {
            value: value.map(Cow::into_owned),
            options: select_options(setting),
            optional: true,
        }),
        _ => None,
    }
}

pub fn snapshot_from_settings(setting: impl Fn(SettingId) -> Option<Setting>) -> DeviceSnapshot {
    let (listening_mode, mode_options) = setting(SettingId::AmbientSoundMode)
        .and_then(select_value_and_options)
        .map(|(value, options)| (parse_listening_mode(&value), options))
        .unwrap_or_default();

    let (selected_preset, preset_options) = setting(SettingId::PresetEqualizerProfile)
        .and_then(preset_value_and_options)
        .unwrap_or_default();

    DeviceSnapshot {
        battery_left: setting(SettingId::BatteryLevelLeft).and_then(battery_percent),
        battery_right: setting(SettingId::BatteryLevelRight).and_then(battery_percent),
        battery_case: setting(SettingId::CaseBatteryLevel).and_then(battery_percent),
        listening_mode,
        ambient_level: ambient_level(&setting, listening_mode),
        mode_options,
        selected_preset,
        preset_options,
        equalizer: setting(SettingId::VolumeAdjustments).and_then(equalizer_state),
        daily_controls: collect_controls(&setting, DAILY_CONTROLS),
        earbud_controls: collect_controls(&setting, EARBUD_CONTROLS),
    }
}

pub fn listening_mode_value(mode: ListeningMode) -> &'static str {
    match mode {
        ListeningMode::NoiseCanceling => "NoiseCanceling",
        ListeningMode::Transparency => "Transparency",
        ListeningMode::Normal => "Normal",
    }
}

pub fn parse_listening_mode(value: &str) -> ListeningMode {
    match value {
        "NoiseCanceling" => ListeningMode::NoiseCanceling,
        "Transparency" => ListeningMode::Transparency,
        "Normal" => ListeningMode::Normal,
        _ => ListeningMode::Normal,
    }
}

fn battery_percent(setting: Setting) -> Option<u8> {
    match setting {
        Setting::I32Range { value, .. } => u8::try_from(value.clamp(0, 100)).ok(),
        Setting::Information { value, .. } => fraction_as_percent(&value),
        _ => None,
    }
}

fn fraction_as_percent(value: &str) -> Option<u8> {
    let (current, maximum) = value.split_once('/')?;
    let current = current.parse::<u16>().ok()?;
    let maximum = maximum.parse::<u16>().ok()?;
    if maximum == 0 {
        return None;
    }
    let percent = (current.saturating_mul(100) + maximum / 2) / maximum;
    u8::try_from(percent.min(100)).ok()
}

fn ambient_level(
    setting: &impl Fn(SettingId) -> Option<Setting>,
    mode: ListeningMode,
) -> Option<u8> {
    let noise_canceling = setting(SettingId::ManualNoiseCanceling).and_then(range_value);
    let transparency = setting(SettingId::ManualTransparency).and_then(range_value);
    let noise_level = |strength: i32| u8::try_from((strength + 5).clamp(6, 10)).ok();
    let transparency_level = |strength: i32| u8::try_from((6 - strength).clamp(1, 5)).ok();

    match mode {
        ListeningMode::NoiseCanceling => noise_canceling
            .and_then(noise_level)
            .or_else(|| transparency.and_then(noise_level)),
        ListeningMode::Transparency => transparency
            .and_then(transparency_level)
            .or_else(|| noise_canceling.and_then(transparency_level)),
        ListeningMode::Normal => transparency
            .and_then(transparency_level)
            .or_else(|| noise_canceling.and_then(transparency_level)),
    }
}

fn range_value(setting: Setting) -> Option<i32> {
    match setting {
        Setting::I32Range { value, .. } => Some(value),
        _ => None,
    }
}

fn select_value_and_options(setting: Setting) -> Option<(String, Vec<SelectOption>)> {
    match setting {
        Setting::Select { setting, value } => Some((value.into_owned(), select_options(setting))),
        _ => None,
    }
}

fn preset_value_and_options(setting: Setting) -> Option<(Option<String>, Vec<SelectOption>)> {
    match setting {
        Setting::PresetEqualizerProfileSelect { select, value, .. } => Some((
            value.map(|value| value.into_owned()),
            select_options(select),
        )),
        _ => None,
    }
}

fn select_options(setting: openscq30_lib::settings::Select) -> Vec<SelectOption> {
    setting
        .options
        .into_iter()
        .enumerate()
        .map(|(index, value)| SelectOption {
            label: setting
                .localized_options
                .get(index)
                .cloned()
                .unwrap_or_else(|| value.to_string()),
            value: value.into_owned(),
        })
        .collect()
}

fn equalizer_state(setting: Setting) -> Option<EqualizerState> {
    match setting {
        Setting::Equalizer { setting, value, .. } => Some(EqualizerState {
            frequencies_hz: setting.band_hz.into_owned(),
            gains: value,
            min: setting.min,
            max: setting.max,
            fraction_digits: setting.fraction_digits,
        }),
        _ => None,
    }
}
