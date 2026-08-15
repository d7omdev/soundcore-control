//! Debug-only window (Ctrl+D, `debug_assertions` builds only) for simulating device
//! connections and states without real hardware. Simulated events are fed through
//! `SoundcoreApp::handle_device_event`, the same path real hardware events take, so this
//! exercises the real UI-update logic rather than a parallel copy of it.

use std::borrow::Cow;
use std::collections::HashMap;

use eframe::egui::{self, RichText};
use openscq30_lib::DeviceModel;
use openscq30_lib::settings::{Equalizer, Range, Select, Setting, SettingId};
use soundcore_control::device::DeviceEvent;
use soundcore_control::devices::DEVICE_PROFILES;
use soundcore_control::domain::{DeviceSnapshot, ListeningMode};

/// Whether `model` reports a single overall `SettingId::BatteryLevel` (over-ear headphones
/// with no case) rather than `BatteryLevelLeft`/`Right`/`CaseBatteryLevel` (earbuds with a
/// case). Mirrors the `single_battery`/`dual_battery` builder calls in the pinned
/// `openscq30-lib` revision's `lib/src/devices/soundcore/<model>.rs` files. Debug-only: this
/// is simulation metadata to make the debug window match real hardware shape, not something
/// the shipped UI needs, since the shipped UI reacts to whichever `SettingId`s a real
/// connection actually reports.
fn is_single_battery(model: DeviceModel) -> bool {
    matches!(
        model,
        DeviceModel::SoundcoreA3028 | DeviceModel::SoundcoreA3062
    )
}

use crate::app::SoundcoreApp;

pub(crate) struct DebugState {
    profile_index: usize,
    battery_level: u8,
    battery_left: u8,
    battery_right: u8,
    battery_case: u8,
    listening_mode: ListeningMode,
    ambient_level: u8,
    error_message: String,
}

impl Default for DebugState {
    fn default() -> Self {
        Self {
            profile_index: 0,
            battery_level: 80,
            battery_left: 90,
            battery_right: 85,
            battery_case: 70,
            listening_mode: ListeningMode::Normal,
            ambient_level: 5,
            error_message: "Simulated device error".into(),
        }
    }
}

pub(crate) fn debug_window(app: &mut SoundcoreApp, context: &egui::Context) {
    let viewport_id = egui::ViewportId::from_hash_of("soundcore-control-debug-window");
    context.show_viewport_immediate(
        viewport_id,
        egui::ViewportBuilder::default()
            .with_title("Debug: simulate device")
            .with_inner_size(egui::vec2(360.0, 560.0)),
        |context, _class| {
            if context.input(|input| input.viewport().close_requested()) {
                app.debug_window_open = false;
            }
            egui::CentralPanel::default().show(context, |ui| {
                ui.label(RichText::new("Ctrl+D (in the main window) to toggle this window").weak());
                ui.add_space(8.0);

                egui::ComboBox::from_label("Device profile")
                    .selected_text(
                        DEVICE_PROFILES[app.debug_state.profile_index]
                            .display_name
                            .clone(),
                    )
                    .show_ui(ui, |ui| {
                        for (index, profile) in DEVICE_PROFILES.iter().enumerate() {
                            ui.selectable_value(
                                &mut app.debug_state.profile_index,
                                index,
                                &profile.display_name,
                            );
                        }
                    });

                ui.add_space(10.0);
                ui.separator();
                ui.add_space(10.0);
                ui.label("Connection state");
                ui.horizontal_wrapped(|ui| {
                    if ui.button("Searching").clicked() {
                        app.handle_device_event(context, DeviceEvent::Searching);
                    }
                    if ui.button("Connecting").clicked() {
                        let profile = &DEVICE_PROFILES[app.debug_state.profile_index];
                        app.handle_device_event(
                            context,
                            DeviceEvent::Connecting {
                                name: profile.display_name.clone(),
                                icon: profile.icon.clone(),
                                supports_manual_ambient_ranges: profile
                                    .supports_manual_ambient_ranges,
                            },
                        );
                    }
                    if ui.button("Disconnected").clicked() {
                        app.handle_device_event(context, DeviceEvent::Disconnected);
                    }
                });

                let profile = &DEVICE_PROFILES[app.debug_state.profile_index];
                let single_battery = is_single_battery(profile.model);

                ui.add_space(10.0);
                ui.separator();
                ui.add_space(10.0);
                ui.label(if single_battery {
                    "Battery (single, no case, set by the picked device)"
                } else {
                    "Battery (left/right/case, set by the picked device)"
                });
                if single_battery {
                    ui.add(
                        egui::Slider::new(&mut app.debug_state.battery_level, 0..=100)
                            .text("Battery"),
                    );
                } else {
                    ui.add(
                        egui::Slider::new(&mut app.debug_state.battery_left, 0..=100).text("Left"),
                    );
                    ui.add(
                        egui::Slider::new(&mut app.debug_state.battery_right, 0..=100)
                            .text("Right"),
                    );
                    ui.add(
                        egui::Slider::new(&mut app.debug_state.battery_case, 0..=100).text("Case"),
                    );
                }

                ui.add_space(10.0);
                ui.label("Ambient");
                egui::ComboBox::from_label("Listening mode")
                    .selected_text(format!("{:?}", app.debug_state.listening_mode))
                    .show_ui(ui, |ui| {
                        for mode in [
                            ListeningMode::Normal,
                            ListeningMode::Transparency,
                            ListeningMode::NoiseCanceling,
                        ] {
                            ui.selectable_value(
                                &mut app.debug_state.listening_mode,
                                mode,
                                format!("{mode:?}"),
                            );
                        }
                    });
                ui.add(egui::Slider::new(&mut app.debug_state.ambient_level, 0..=10).text("Level"));

                ui.add_space(14.0);
                if ui.button("Connect with these settings").clicked() {
                    let snapshot = build_snapshot(
                        &app.debug_state,
                        profile.model,
                        single_battery,
                        profile.supports_manual_ambient_ranges,
                    );
                    app.handle_device_event(
                        context,
                        DeviceEvent::Connected {
                            name: profile.display_name.clone(),
                            snapshot,
                            icon: profile.icon.clone(),
                            supports_manual_ambient_ranges: profile.supports_manual_ambient_ranges,
                        },
                    );
                }

                ui.add_space(14.0);
                ui.separator();
                ui.add_space(10.0);
                ui.label("Error");
                ui.text_edit_singleline(&mut app.debug_state.error_message);
                if ui.button("Fire error").clicked() {
                    app.handle_device_event(
                        context,
                        DeviceEvent::Error(app.debug_state.error_message.clone()),
                    );
                }
            });
        },
    );
}

/// Which of the settings the shipped UI cares about (`domain::DAILY_CONTROLS`,
/// `domain::BUTTON_CONTROLS`, and whether ambient mode switching exists at all) a given real
/// model actually registers, per the pinned `openscq30-lib` revision's
/// `lib/src/devices/soundcore/<model>[.rs|/]` sources. Cross-checked against both the
/// per-model builder method calls (`builder.auto_power_off()`, `builder.ldac()`, etc., each
/// of which registers one specific `SettingId`) and literal `SettingId::X` references in
/// those files, since some settings are wired through shared helper modules that don't
/// re-spell the id locally. Debug-only: this is simulation metadata so the debug window
/// doesn't show settings a real device of this model wouldn't report; the shipped UI itself
/// doesn't need this, since it already only renders whatever a live connection reports.
struct ModelSupport {
    ambient_mode: bool,
    daily_ids: &'static [SettingId],
    button_ids: &'static [SettingId],
}

fn model_support(model: DeviceModel) -> ModelSupport {
    use SettingId::{
        AutoPowerOff, DoublePress, EasyChat, EasyChatWaitTime, Ldac, LeftDoublePress,
        LeftLongPress, LeftSinglePress, LeftSlideDown, LeftSlideUp, LeftTriplePress,
        RightDoublePress, RightLongPress, RightSinglePress, RightSlideDown, RightSlideUp,
        RightTriplePress, SinglePress, SoundLeakCompensation, WearingDetection,
        WindNoiseSuppression,
    };
    match model {
        // Liberty 4 Pro: full daily-control set and full 12-gesture button set.
        DeviceModel::SoundcoreA3954 => ModelSupport {
            ambient_mode: true,
            daily_ids: &[
                WindNoiseSuppression,
                WearingDetection,
                EasyChat,
                EasyChatWaitTime,
                AutoPowerOff,
                Ldac,
                SoundLeakCompensation,
            ],
            button_ids: &[
                LeftSinglePress,
                LeftDoublePress,
                LeftTriplePress,
                LeftLongPress,
                LeftSlideUp,
                LeftSlideDown,
                RightSinglePress,
                RightDoublePress,
                RightTriplePress,
                RightLongPress,
                RightSlideUp,
                RightSlideDown,
            ],
        },
        // R60i NC: no easy chat or sound-leak compensation, no slide gestures.
        DeviceModel::SoundcoreD1202C => ModelSupport {
            ambient_mode: true,
            daily_ids: &[WindNoiseSuppression, AutoPowerOff, Ldac],
            button_ids: &[
                LeftSinglePress,
                LeftDoublePress,
                LeftTriplePress,
                LeftLongPress,
                RightSinglePress,
                RightDoublePress,
                RightTriplePress,
                RightLongPress,
            ],
        },
        // P20i: no ANC, so no ambient mode switching at all, and no daily controls; button
        // gestures are single/double/long press only (no triple, no slides).
        DeviceModel::SoundcoreA3949 => ModelSupport {
            ambient_mode: false,
            daily_ids: &[],
            button_ids: &[
                LeftSinglePress,
                LeftDoublePress,
                LeftLongPress,
                RightSinglePress,
                RightDoublePress,
                RightLongPress,
            ],
        },
        // Life Q30: mode switching and auto-power-off only, no configurable button actions
        // upstream at all.
        DeviceModel::SoundcoreA3028 => ModelSupport {
            ambient_mode: true,
            daily_ids: &[AutoPowerOff],
            button_ids: &[],
        },
        // Space One Pro: single physical button per ear cup (single/double press only).
        DeviceModel::SoundcoreA3062 => ModelSupport {
            ambient_mode: true,
            daily_ids: &[WindNoiseSuppression, AutoPowerOff, Ldac],
            button_ids: &[SinglePress, DoublePress],
        },
        _ => ModelSupport {
            ambient_mode: true,
            daily_ids: &[],
            button_ids: &[],
        },
    }
}

fn daily_setting(id: SettingId) -> Setting {
    match id {
        SettingId::EasyChatWaitTime => Setting::Select {
            setting: Select {
                options: vec![Cow::Borrowed("5s"), Cow::Borrowed("10s")],
                localized_options: vec!["5 seconds".into(), "10 seconds".into()],
            },
            value: Cow::Borrowed("10s"),
        },
        SettingId::AutoPowerOff => Setting::OptionalSelect {
            setting: Select {
                options: vec![Cow::Borrowed("30min"), Cow::Borrowed("60min")],
                localized_options: vec!["30 minutes".into(), "60 minutes".into()],
            },
            value: Some(Cow::Borrowed("30min")),
        },
        // WindNoiseSuppression, WearingDetection, EasyChat, Ldac, SoundLeakCompensation are
        // all plain on/off toggles.
        _ => Setting::Toggle { value: true },
    }
}

fn press_action_select(value: &'static str) -> Setting {
    Setting::OptionalSelect {
        setting: Select {
            options: vec![Cow::Borrowed("PlayPause"), Cow::Borrowed("VoiceAssistant")],
            localized_options: vec!["Play / Pause".into(), "Voice Assistant".into()],
        },
        value: Some(Cow::Borrowed(value)),
    }
}

/// Builds a fake raw-settings map for the picked device shape, then runs it through the
/// real `domain::snapshot_from_settings`, the exact function a live connection's settings
/// feed goes through. This guarantees the debug window shows the same full set of daily and
/// button controls (with real translated labels) that a live device of this shape would,
/// rather than a hand-picked subset that can silently drift from what `domain.rs` reports.
fn build_snapshot(
    state: &DebugState,
    model: DeviceModel,
    single_battery: bool,
    supports_manual_ambient_ranges: bool,
) -> DeviceSnapshot {
    let mut settings: HashMap<SettingId, Setting> = HashMap::new();
    let support = model_support(model);

    if single_battery {
        settings.insert(
            SettingId::BatteryLevel,
            battery_fraction(state.battery_level),
        );
    } else {
        settings.insert(
            SettingId::BatteryLevelLeft,
            battery_fraction(state.battery_left),
        );
        settings.insert(
            SettingId::BatteryLevelRight,
            battery_fraction(state.battery_right),
        );
        settings.insert(
            SettingId::CaseBatteryLevel,
            battery_fraction(state.battery_case),
        );
    }
    for id in support.button_ids {
        settings.insert(*id, press_action_select("PlayPause"));
    }

    if support.ambient_mode {
        settings.insert(
            SettingId::AmbientSoundMode,
            Setting::Select {
                setting: Select {
                    options: vec![
                        Cow::Borrowed("Normal"),
                        Cow::Borrowed("Transparency"),
                        Cow::Borrowed("NoiseCanceling"),
                    ],
                    localized_options: vec![
                        "Normal".into(),
                        "Transparency".into(),
                        "Noise Canceling".into(),
                    ],
                },
                value: Cow::Borrowed(soundcore_control::domain::listening_mode_value(
                    state.listening_mode,
                )),
            },
        );
        if supports_manual_ambient_ranges {
            settings.insert(
                SettingId::ManualTransparency,
                Setting::I32Range {
                    setting: Range {
                        range: 1..=5,
                        step: 1,
                    },
                    value: i32::from(state.ambient_level.clamp(1, 5)),
                },
            );
            settings.insert(
                SettingId::ManualNoiseCanceling,
                Setting::I32Range {
                    setting: Range {
                        range: 1..=5,
                        step: 1,
                    },
                    value: i32::from(state.ambient_level.clamp(1, 5)),
                },
            );
        }
    }

    // Equalizer support is confirmed present for every registered model.
    settings.insert(
        SettingId::PresetEqualizerProfile,
        Setting::PresetEqualizerProfileSelect {
            equalizer: sample_equalizer(),
            select: Select {
                options: vec![
                    Cow::Borrowed("SoundcoreSignature"),
                    Cow::Borrowed("BassBooster"),
                ],
                localized_options: vec!["Soundcore Signature".into(), "Bass Booster".into()],
            },
            presets: vec![vec![0; 8], vec![30; 8]],
            value: Some(Cow::Borrowed("SoundcoreSignature")),
        },
    );
    settings.insert(
        SettingId::VolumeAdjustments,
        Setting::Equalizer {
            setting: sample_equalizer(),
            read_only: false,
            value: vec![0, 10, 20, 30, 20, 10, 0, -10],
        },
    );

    for id in support.daily_ids {
        settings.insert(*id, daily_setting(*id));
    }

    soundcore_control::domain::snapshot_from_settings(|id| settings.get(&id).cloned())
}

fn battery_fraction(level: u8) -> Setting {
    Setting::Information {
        value: format!("{level}/100"),
        translated_value: format!("{level}%"),
    }
}

fn sample_equalizer() -> Equalizer {
    Equalizer {
        band_hz: Cow::Borrowed(&[100, 200, 400, 800, 1600, 3200, 6400, 12800]),
        fraction_digits: 1,
        min: -60,
        max: 60,
    }
}
