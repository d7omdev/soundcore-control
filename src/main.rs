use std::{collections::HashMap, process::Command, thread, time::Duration};

use eframe::egui::{
    self, Align, Color32, CornerRadius, FontId, Layout, Margin, RichText, Stroke, Vec2,
};
use soundcore_control::{
    device::{DeviceEvent, DeviceWorker},
    domain::{
        ControlSetting, ControlValue, DeviceCommand, DeviceSnapshot, ListeningMode,
        parse_listening_mode,
    },
    tray::{TrayAction, TrayController, TrayState},
};

const BG: Color32 = Color32::BLACK;
const HERO: Color32 = Color32::from_rgb(27, 28, 30);
const CARD: Color32 = Color32::from_rgb(27, 28, 30);
const CARD_ALT: Color32 = Color32::from_rgb(45, 45, 48);
const TEXT: Color32 = Color32::from_rgb(247, 247, 249);
const MUTED: Color32 = Color32::from_rgb(166, 166, 174);
const ACCENT: Color32 = Color32::from_rgb(25, 190, 235);
const EQ_ACCENT: Color32 = Color32::from_rgb(248, 174, 55);
const BATTERY: Color32 = Color32::from_rgb(61, 205, 85);
const WARNING: Color32 = Color32::from_rgb(255, 174, 105);

fn main() -> eframe::Result {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "soundcore_control=info".into()),
        )
        .init();

    // Selects the system locale for openscq30-lib's translated setting labels (falls back
    // to English for unsupported locales). Must run before any device connection, since
    // that's what populates the labels in DeviceSnapshot::daily_controls/earbud_controls.
    let requested_languages = i18n_embed::DesktopLanguageRequester::requested_languages();
    openscq30_lib::i18n::init(&requested_languages);

    if std::env::args().any(|argument| argument == "--tray-only") {
        run_tray_only();
        return Ok(());
    }

    if std::env::args().any(|argument| argument == "--watch") {
        run_watch();
        return Ok(());
    }

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Soundcore Control")
            .with_inner_size([920.0, 720.0])
            .with_min_inner_size([420.0, 560.0]),
        ..Default::default()
    };

    eframe::run_native(
        "Soundcore Control",
        options,
        Box::new(|context| Ok(Box::new(SoundcoreApp::new(context)))),
    )
}

fn run_tray_only() {
    let tray = TrayController::spawn_background();
    tray.update(TrayState::searching());
    // Register the replacement tray item before reconnecting, so closing the UI
    // never leaves the desktop without a Soundcore Control tray icon.
    thread::sleep(Duration::from_millis(500));
    let worker = DeviceWorker::spawn();
    let mut open_window = false;
    let mut device_name = String::new();

    'service: loop {
        while let Some(event) = worker.try_recv() {
            match event {
                DeviceEvent::Searching => tray.update(TrayState::searching()),
                DeviceEvent::Connecting { name, .. } => {
                    device_name = name;
                    tray.update(TrayState::connecting(&device_name));
                }
                DeviceEvent::Connected {
                    name,
                    snapshot,
                    icon: _,
                    supports_manual_ambient_ranges: _,
                } => {
                    device_name = name;
                    tray.update(TrayState::connected(&device_name, &snapshot));
                    soundcore_control::notify::buds_connected(&device_name, &snapshot);
                }
                DeviceEvent::Snapshot(snapshot) => {
                    tray.update(TrayState::connected(&device_name, &snapshot));
                }
                DeviceEvent::Error(error) => {
                    tracing::error!(%error, "tray Bluetooth error");
                    tray.update(TrayState::failed());
                }
                DeviceEvent::Disconnected => tray.update(TrayState::disconnected()),
            }
        }

        while let Some(action) = tray.try_recv() {
            match action {
                TrayAction::ShowWindow => {
                    open_window = true;
                    break 'service;
                }
                TrayAction::Disconnect => {
                    let _ = worker.send(DeviceCommand::Disconnect);
                }
                TrayAction::SetListeningMode(mode) => {
                    let _ = worker.send(DeviceCommand::SetListeningMode(mode));
                }
                TrayAction::Quit => break 'service,
            }
        }
        thread::sleep(Duration::from_millis(100));
    }

    drop(worker);
    drop(tray);
    if open_window {
        thread::sleep(Duration::from_millis(350));
        if let Ok(executable) = std::env::current_exe()
            && let Err(error) = Command::new(executable).spawn()
        {
            tracing::error!(%error, "could not reopen Soundcore Control");
        }
    }
}

fn run_watch() {
    let runtime = match tokio::runtime::Runtime::new() {
        Ok(runtime) => runtime,
        Err(error) => {
            tracing::error!(%error, "could not start Bluetooth watcher runtime");
            return;
        }
    };
    runtime.block_on(soundcore_control::watch::run());
}

fn launch_tray_only() {
    let Ok(executable) = std::env::current_exe() else {
        return;
    };
    if let Err(error) = Command::new(executable).arg("--tray-only").spawn() {
        tracing::error!(%error, "could not keep Soundcore Control in the tray");
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum ActiveTab {
    #[default]
    Ambient,
    Equalizer,
    Controls,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
enum ConnectionView {
    #[default]
    Searching,
    Connecting,
    Connected,
    Disconnected,
    Failed,
}

struct SoundcoreApp {
    worker: DeviceWorker,
    tray: TrayController,
    connection: ConnectionView,
    device_name: String,
    snapshot: DeviceSnapshot,
    message: Option<String>,
    active_tab: ActiveTab,
    supports_manual_ambient_ranges: bool,
    current_icon: Option<&'static [u8]>,
    icon_textures: HashMap<usize, egui::TextureHandle>,
    soundcore_texture: Option<egui::TextureHandle>,
    allow_exit: bool,
}

impl SoundcoreApp {
    fn new(context: &eframe::CreationContext<'_>) -> Self {
        configure_style(&context.egui_ctx);
        let tray = TrayController::spawn_for_ui(context.egui_ctx.clone());
        tray.update(TrayState::searching());
        Self {
            worker: DeviceWorker::spawn(),
            tray,
            connection: ConnectionView::Searching,
            device_name: "Searching…".into(),
            snapshot: DeviceSnapshot::default(),
            message: None,
            active_tab: ActiveTab::Ambient,
            supports_manual_ambient_ranges: true,
            current_icon: None,
            icon_textures: HashMap::new(),
            soundcore_texture: load_soundcore_texture(
                &context.egui_ctx,
                include_bytes!("../assets/soundcore.png"),
            ),
            allow_exit: false,
        }
    }

    /// Loads and caches a texture for `icon`'s bytes, keyed by their address (each embedded
    /// icon asset is a distinct `'static` byte slice), and makes it the active icon.
    fn set_current_icon(&mut self, context: &egui::Context, icon: Option<&'static [u8]>) {
        self.current_icon = icon;
        let Some(icon) = icon else { return };
        let key = icon.as_ptr() as usize;
        if self.icon_textures.contains_key(&key) {
            return;
        }
        if let Some(texture) = load_png_texture(context, "device-icon", icon) {
            self.icon_textures.insert(key, texture);
        }
    }

    fn poll_device(&mut self, context: &egui::Context) {
        let events = std::iter::from_fn(|| self.worker.try_recv()).collect::<Vec<_>>();
        for event in events {
            match event {
                DeviceEvent::Searching => {
                    self.connection = ConnectionView::Searching;
                    self.tray.update(TrayState::searching());
                }
                DeviceEvent::Connecting {
                    name,
                    icon,
                    supports_manual_ambient_ranges,
                } => {
                    self.connection = ConnectionView::Connecting;
                    self.device_name = name;
                    self.supports_manual_ambient_ranges = supports_manual_ambient_ranges;
                    self.set_current_icon(context, icon);
                    self.tray.update(TrayState::connecting(&self.device_name));
                }
                DeviceEvent::Connected {
                    name,
                    snapshot,
                    icon,
                    supports_manual_ambient_ranges,
                } => {
                    tracing::info!(
                        device = %name,
                        daily_controls = snapshot.daily_controls.len(),
                        earbud_controls = snapshot.earbud_controls.len(),
                        "connected to earbuds"
                    );
                    self.connection = ConnectionView::Connected;
                    self.device_name = name.clone();
                    self.supports_manual_ambient_ranges = supports_manual_ambient_ranges;
                    self.set_current_icon(context, icon);
                    self.tray
                        .update(TrayState::connected(&self.device_name, &snapshot));
                    soundcore_control::notify::buds_connected(&name, &snapshot);
                    self.snapshot = snapshot;
                    self.message = None;
                }
                DeviceEvent::Snapshot(snapshot) => {
                    self.tray
                        .update(TrayState::connected(&self.device_name, &snapshot));
                    self.snapshot = snapshot;
                    self.connection = ConnectionView::Connected;
                    self.message = None;
                }
                DeviceEvent::Error(error) => {
                    tracing::error!(%error, "device control error");
                    if self.connection != ConnectionView::Connected {
                        self.connection = ConnectionView::Failed;
                        self.tray.update(TrayState::failed());
                    }
                    self.message = Some(error);
                }
                DeviceEvent::Disconnected => {
                    tracing::warn!("earbuds disconnected");
                    self.connection = ConnectionView::Disconnected;
                    self.tray.update(TrayState::disconnected());
                    self.message = Some("The earbuds disconnected from Bluetooth.".into());
                }
            }
        }
    }

    fn poll_tray(&mut self, context: &egui::Context) {
        while let Some(action) = self.tray.try_recv() {
            match action {
                TrayAction::ShowWindow => {
                    context.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
                    context.send_viewport_cmd(egui::ViewportCommand::Focus);
                }
                TrayAction::Disconnect => {
                    self.message = Some("Disconnecting earbuds…".into());
                    self.send(DeviceCommand::Disconnect);
                }
                TrayAction::SetListeningMode(mode) => {
                    self.snapshot.listening_mode = mode;
                    self.send(DeviceCommand::SetListeningMode(mode));
                }
                TrayAction::Quit => {
                    self.allow_exit = true;
                    context.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            }
        }
    }

    fn send(&mut self, command: DeviceCommand) {
        if let Err(error) = self.worker.send(command) {
            self.message = Some(error.to_string());
        }
    }

    fn reconnect(&mut self) {
        self.worker = DeviceWorker::spawn();
        self.connection = ConnectionView::Searching;
        self.tray.update(TrayState::searching());
        self.message = None;
    }

    fn header(&mut self, ui: &mut egui::Ui) {
        egui::Frame::new()
            .fill(HERO)
            .corner_radius(CornerRadius::same(22))
            .inner_margin(Margin::same(24))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    if let Some(texture) = &self.soundcore_texture {
                        ui.add(
                            egui::Image::new(texture)
                                .fit_to_exact_size(Vec2::new(126.0, 24.0))
                                .maintain_aspect_ratio(true),
                        );
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if self.connection != ConnectionView::Connected
                            && ui.button("Try again").clicked()
                        {
                            self.reconnect();
                        }
                    });
                });
                ui.add_space(4.0);
                ui.vertical_centered(|ui| {
                    ui.heading(RichText::new(&self.device_name).size(27.0).color(TEXT));
                    ui.add_space(8.0);
                    let icon_texture = self
                        .current_icon
                        .and_then(|icon| self.icon_textures.get(&(icon.as_ptr() as usize)));
                    draw_buds(ui, icon_texture);
                    ui.add_space(14.0);
                    ui.columns(3, |columns| {
                        battery(&mut columns[0], "L", self.snapshot.battery_left);
                        battery(&mut columns[1], "R", self.snapshot.battery_right);
                        battery(&mut columns[2], "Case", self.snapshot.battery_case);
                    });
                    ui.add_space(8.0);
                    let (color, text) = match self.connection {
                        ConnectionView::Searching => (WARNING, "Looking for earbuds…"),
                        ConnectionView::Connecting => (WARNING, "Opening controls…"),
                        ConnectionView::Connected => (BATTERY, "Connected"),
                        ConnectionView::Disconnected => (WARNING, "Disconnected"),
                        ConnectionView::Failed => (WARNING, "Needs attention"),
                    };
                    ui.horizontal(|ui| {
                        status_dot(ui, color);
                        ui.label(RichText::new(text).color(MUTED));
                    });
                });
            });
    }

    fn tab_bar(&mut self, ui: &mut egui::Ui) {
        ui.columns(3, |columns| {
            for (column, (tab, icon, label)) in columns.iter_mut().zip([
                (ActiveTab::Ambient, TabIcon::Ambient, "Ambient"),
                (ActiveTab::Equalizer, TabIcon::Equalizer, "Equalizer"),
                (ActiveTab::Controls, TabIcon::Controls, "Controls"),
            ]) {
                let response = tab_button(column, icon, label, self.active_tab == tab);
                if response.clicked() {
                    self.active_tab = tab;
                }
            }
        });
    }

    fn sound_mode_card(&mut self, ui: &mut egui::Ui) {
        card(ui, |ui| {
            if self.supports_manual_ambient_ranges {
                self.ambient_slider_section(ui);
            } else {
                self.ambient_mode_picker_section(ui);
            }
        });
    }

    /// Continuous 1-10 ambient slider, for devices that expose manual noise-canceling and
    /// transparency ranges (see `DeviceProfile::supports_manual_ambient_ranges`).
    fn ambient_slider_section(&mut self, ui: &mut egui::Ui) {
        section_title(
            ui,
            "Ambient Sound",
            "Slide from transparency to noise canceling",
        );
        ui.add_space(18.0);
        match ambient_slider(
            ui,
            self.snapshot.ambient_level,
            self.snapshot.listening_mode,
            self.is_connected(),
        ) {
            Some(AmbientSelection::Level(level)) => {
                self.snapshot.ambient_level = Some(level);
                self.snapshot.listening_mode = if level <= 5 {
                    ListeningMode::Transparency
                } else {
                    ListeningMode::NoiseCanceling
                };
                self.send(DeviceCommand::SetAmbientLevel(level));
            }
            Some(AmbientSelection::Normal) => {
                self.snapshot.listening_mode = ListeningMode::Normal;
                self.snapshot.ambient_level = None;
                self.send(DeviceCommand::SetListeningMode(ListeningMode::Normal));
            }
            None => {}
        }
        ui.add_space(8.0);
        let (labels, _) =
            ui.allocate_exact_size(Vec2::new(ui.available_width(), 18.0), egui::Sense::hover());
        let font = FontId::proportional(12.0);
        ui.painter().text(
            labels.left_center(),
            egui::Align2::LEFT_CENTER,
            "Transparency",
            font.clone(),
            MUTED,
        );
        ui.painter().text(
            labels.center(),
            egui::Align2::CENTER_CENTER,
            "Normal",
            font.clone(),
            MUTED,
        );
        ui.painter().text(
            labels.right_center(),
            egui::Align2::RIGHT_CENTER,
            "Noise Canceling",
            font,
            MUTED,
        );
    }

    /// Mode-only picker for devices with discrete ambient modes but no manual intensity
    /// slider (e.g. the R60i NC only exposes Normal/Transparency/Noise Canceling as fixed
    /// modes, not a continuous 1-10 range).
    fn ambient_mode_picker_section(&mut self, ui: &mut egui::Ui) {
        section_title(ui, "Ambient Sound", "Choose a listening mode");
        ui.add_space(18.0);
        let connected = self.is_connected();
        let options = self.snapshot.mode_options.clone();
        let current = self.snapshot.listening_mode;
        let mut selected = None;
        ui.columns(options.len().max(1), |columns| {
            for (column, option) in columns.iter_mut().zip(&options) {
                let mode = parse_listening_mode(&option.value);
                if column
                    .add_enabled(
                        connected,
                        egui::Button::new(&option.label).selected(mode == current),
                    )
                    .clicked()
                {
                    selected = Some(mode);
                }
            }
        });
        if let Some(mode) = selected {
            self.snapshot.listening_mode = mode;
            self.send(DeviceCommand::SetListeningMode(mode));
        }
    }

    fn equalizer_card(&mut self, ui: &mut egui::Ui) {
        card(ui, |ui| {
            ui.visuals_mut().selection.bg_fill = EQ_ACCENT;
            section_title(ui, "Equalizer", "Tune the sound to you");
            ui.add_space(12.0);
            let current = self
                .snapshot
                .selected_preset
                .clone()
                .unwrap_or_else(|| "Custom".into());
            let options = self.snapshot.preset_options.clone();
            egui::ComboBox::from_id_salt("equalizer-preset")
                .selected_text(preset_label(&options, &current))
                .width(ui.available_width().min(260.0))
                .show_ui(ui, |ui| {
                    ui.set_min_width(240.0);
                    for option in options {
                        if ui
                            .selectable_label(option.value == current, &option.label)
                            .clicked()
                        {
                            self.snapshot.selected_preset = Some(option.value.clone());
                            self.send(DeviceCommand::SetPreset(option.value));
                        }
                    }
                });
            ui.add_space(18.0);

            let connected = self.is_connected();
            let mut changed_gains = None;
            if let Some(equalizer) = &mut self.snapshot.equalizer {
                let mut changed = false;
                let band_count = equalizer.gains.len().max(1) as f32;
                let band_width = (ui.available_width() / band_count).max(34.0);
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 0.0;
                    for (index, gain) in equalizer.gains.iter_mut().enumerate() {
                        ui.allocate_ui_with_layout(
                            Vec2::new(band_width, 190.0),
                            Layout::top_down(Align::Center),
                            |ui| {
                                ui.spacing_mut().slider_width = 120.0;
                                let db = *gain as f32
                                    / 10_f32.powi(i32::from(equalizer.fraction_digits));
                                ui.label(
                                    RichText::new(format!("{db:+.1}")).size(11.0).color(MUTED),
                                );
                                let response = ui.add_enabled(
                                    connected,
                                    egui::Slider::new(gain, equalizer.min..=equalizer.max)
                                        .vertical()
                                        .show_value(false),
                                );
                                changed |= response.changed();
                                let frequency =
                                    equalizer.frequencies_hz.get(index).copied().unwrap_or(0);
                                ui.label(
                                    RichText::new(format_frequency(frequency))
                                        .size(11.0)
                                        .color(MUTED),
                                );
                            },
                        );
                    }
                });
                if changed {
                    changed_gains = Some(equalizer.gains.clone());
                }
            } else {
                ui.label(
                    RichText::new("Equalizer data will appear after connection.").color(MUTED),
                );
                ui.add_space(72.0);
            }
            if let Some(gains) = changed_gains {
                self.snapshot.selected_preset = None;
                self.send(DeviceCommand::SetEqualizer(gains));
            }
        });
    }

    fn controls_card(&mut self, ui: &mut egui::Ui) {
        let daily_controls = self.snapshot.daily_controls.clone();
        let earbud_controls = self.snapshot.earbud_controls.clone();
        let connected = self.is_connected();
        let mut commands = Vec::new();

        card(ui, |ui| {
            section_title(ui, "Daily Controls", "Listening and convenience features");
            ui.add_space(12.0);
            if daily_controls.is_empty() {
                ui.label(RichText::new("Controls will appear after connection.").color(MUTED));
            }
            for control in &daily_controls {
                if let Some(command) = control_row(ui, control, connected) {
                    commands.push(command);
                }
            }
        });

        ui.add_space(14.0);
        card(ui, |ui| {
            section_title(ui, "Earbud Controls", "Choose actions for each gesture");
            ui.add_space(12.0);
            if earbud_controls.is_empty() {
                ui.label(
                    RichText::new("Gesture controls will appear after connection.").color(MUTED),
                );
            }
            for control in &earbud_controls {
                if let Some(command) = control_row(ui, control, connected) {
                    commands.push(command);
                }
            }
        });

        for command in commands {
            self.send(command);
        }
    }

    fn content(&mut self, ui: &mut egui::Ui) {
        self.header(ui);
        if let Some(message) = &self.message {
            ui.add_space(18.0);
            egui::Frame::new()
                .fill(Color32::from_rgb(54, 37, 28))
                .corner_radius(CornerRadius::same(12))
                .inner_margin(Margin::symmetric(16, 12))
                .show(ui, |ui| {
                    ui.label(RichText::new(message).color(Color32::from_rgb(255, 211, 174)));
                });
        }
        ui.add_space(20.0);
        self.tab_bar(ui);
        ui.add_space(18.0);
        match self.active_tab {
            ActiveTab::Ambient => self.sound_mode_card(ui),
            ActiveTab::Equalizer => self.equalizer_card(ui),
            ActiveTab::Controls => self.controls_card(ui),
        }
    }

    fn is_connected(&self) -> bool {
        self.connection == ConnectionView::Connected
    }
}

impl eframe::App for SoundcoreApp {
    fn update(&mut self, context: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll_tray(context);
        self.poll_device(context);
        context.request_repaint_after(Duration::from_millis(100));

        let close_requested = context.input(|input| input.viewport().close_requested());
        let escape_pressed = context.input(|input| input.key_pressed(egui::Key::Escape));
        if (close_requested || escape_pressed) && !self.allow_exit {
            launch_tray_only();
            self.allow_exit = true;
            if escape_pressed {
                context.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }

        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(BG).inner_margin(Margin::same(20)))
            .show(context, |ui| {
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        let content_width = ui.available_width().min(920.0);
                        ui.vertical_centered(|ui| {
                            ui.allocate_ui_with_layout(
                                Vec2::new(content_width, 0.0),
                                Layout::top_down(Align::Min),
                                |ui| self.content(ui),
                            );
                        });
                    });
            });
    }
}

fn control_row(
    ui: &mut egui::Ui,
    control: &ControlSetting,
    enabled: bool,
) -> Option<DeviceCommand> {
    let mut command = None;
    ui.horizontal(|ui| {
        ui.label(RichText::new(&control.label).color(TEXT));
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            match &control.value {
                ControlValue::Toggle(current) => {
                    let mut value = *current;
                    if ui
                        .add_enabled(
                            enabled,
                            egui::Button::new(if value { "On" } else { "Off" }).selected(value),
                        )
                        .clicked()
                    {
                        value = !value;
                        command = Some(DeviceCommand::SetToggle(control.id, value));
                    }
                }
                ControlValue::Select {
                    value,
                    options,
                    optional,
                } => {
                    let selected_label = value
                        .as_deref()
                        .and_then(|selected| {
                            options
                                .iter()
                                .find(|option| option.value == selected)
                                .map(|option| option.label.as_str())
                        })
                        .unwrap_or("Disabled");
                    egui::ComboBox::from_id_salt(format!("control-{:?}", control.id))
                        .selected_text(selected_label)
                        .width(ui.available_width().min(210.0))
                        .show_ui(ui, |ui| {
                            if *optional
                                && ui.selectable_label(value.is_none(), "Disabled").clicked()
                            {
                                command = Some(DeviceCommand::SetSelect {
                                    id: control.id,
                                    value: None,
                                    optional: true,
                                });
                            }
                            for option in options {
                                if ui
                                    .selectable_label(
                                        value.as_deref() == Some(&option.value),
                                        &option.label,
                                    )
                                    .clicked()
                                {
                                    command = Some(DeviceCommand::SetSelect {
                                        id: control.id,
                                        value: Some(option.value.clone()),
                                        optional: *optional,
                                    });
                                }
                            }
                        });
                }
            }
        });
    });
    ui.add_space(6.0);
    ui.separator();
    ui.add_space(6.0);
    command
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AmbientSelection {
    Level(u8),
    Normal,
}

fn ambient_slider(
    ui: &mut egui::Ui,
    current: Option<u8>,
    mode: ListeningMode,
    enabled: bool,
) -> Option<AmbientSelection> {
    let (rect, response) = ui.allocate_exact_size(
        Vec2::new(ui.available_width(), 64.0),
        if enabled {
            egui::Sense::click_and_drag()
        } else {
            egui::Sense::hover()
        },
    );
    let painter = ui.painter();
    let slot_width = rect.width() / 11.0;
    let normal_selected = mode == ListeningMode::Normal;
    let normal_active = normal_selected || current.is_some_and(|level| level >= 6);

    for slot in 0..11 {
        let level = match slot {
            0..=4 => Some(slot + 1),
            5 => None,
            _ => Some(slot),
        };
        let segment = egui::Rect::from_min_max(
            egui::pos2(rect.left() + slot as f32 * slot_width, rect.top()),
            egui::pos2(rect.left() + (slot + 1) as f32 * slot_width, rect.bottom()),
        );
        let rounding = if slot == 0 {
            CornerRadius {
                nw: 14,
                sw: 14,
                ..Default::default()
            }
        } else if slot == 10 {
            CornerRadius {
                ne: 14,
                se: 14,
                ..Default::default()
            }
        } else {
            CornerRadius::ZERO
        };
        let is_colored = level.is_some_and(|level| {
            if normal_selected {
                level <= 5
            } else {
                current.is_some_and(|selected| level <= usize::from(selected))
            }
        });
        let fill = match level {
            Some(level) if is_colored => ambient_gradient((level as f32 - 0.5) / 10.0),
            None if normal_active => ambient_gradient(0.5),
            _ => CARD_ALT,
        };
        painter.rect_filled(segment, rounding, fill);
        if slot > 0 {
            painter.line_segment(
                [segment.left_top(), segment.left_bottom()],
                Stroke::new(1.0_f32, Color32::from_white_alpha(52)),
            );
        }
        if let Some(level) = level {
            painter.text(
                segment.center(),
                egui::Align2::CENTER_CENTER,
                level.to_string(),
                FontId::proportional(16.0),
                if is_colored { Color32::WHITE } else { MUTED },
            );
        }
    }

    let normal_center = egui::pos2(rect.left() + slot_width * 5.5, rect.center().y);
    painter.circle_filled(normal_center, 19.0, HERO);
    painter.circle_filled(
        normal_center,
        16.0,
        if normal_active {
            ambient_gradient(0.5)
        } else {
            CARD_ALT
        },
    );
    painter.circle_stroke(
        normal_center,
        16.0,
        Stroke::new(
            1.5_f32,
            if normal_selected {
                Color32::WHITE
            } else if normal_active {
                Color32::from_white_alpha(180)
            } else {
                Color32::from_rgb(115, 139, 153)
            },
        ),
    );
    draw_normal_icon(
        painter,
        normal_center,
        if normal_active { Color32::WHITE } else { TEXT },
    );

    if enabled && (response.clicked() || response.dragged()) {
        let pointer = response.interact_pointer_pos()?;
        let slot = (((pointer.x - rect.left()) / slot_width).floor() as i32).clamp(0, 10);
        if slot == 5 {
            return (mode != ListeningMode::Normal).then_some(AmbientSelection::Normal);
        }
        let level = if slot < 5 { slot + 1 } else { slot } as u8;
        if current != Some(level) || mode == ListeningMode::Normal {
            return Some(AmbientSelection::Level(level));
        }
    }
    None
}

fn ambient_gradient(position: f32) -> Color32 {
    let transparency = Color32::from_rgb(75, 214, 246);
    let neutral = Color32::from_rgb(65, 113, 143);
    let noise_canceling = Color32::from_rgb(42, 101, 235);
    if position <= 0.5 {
        lerp_color(transparency, neutral, position * 2.0)
    } else {
        lerp_color(neutral, noise_canceling, (position - 0.5) * 2.0)
    }
}

fn lerp_color(from: Color32, to: Color32, amount: f32) -> Color32 {
    let amount = amount.clamp(0.0, 1.0);
    let mix = |start: u8, end: u8| {
        (f32::from(start) + (f32::from(end) - f32::from(start)) * amount).round() as u8
    };
    Color32::from_rgb(
        mix(from.r(), to.r()),
        mix(from.g(), to.g()),
        mix(from.b(), to.b()),
    )
}

fn draw_normal_icon(painter: &egui::Painter, center: egui::Pos2, color: Color32) {
    let stroke = Stroke::new(2.0_f32, color);
    painter.circle_stroke(center + Vec2::new(0.0, -5.0), 4.0, stroke);
    painter.add(egui::Shape::line(
        vec![
            center + Vec2::new(-8.0, 8.0),
            center + Vec2::new(-6.0, 4.0),
            center + Vec2::new(-3.0, 2.0),
            center + Vec2::new(3.0, 2.0),
            center + Vec2::new(6.0, 4.0),
            center + Vec2::new(8.0, 8.0),
        ],
        stroke,
    ));
}

#[derive(Clone, Copy)]
enum TabIcon {
    Ambient,
    Equalizer,
    Controls,
}

fn tab_button(ui: &mut egui::Ui, icon: TabIcon, label: &str, active: bool) -> egui::Response {
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), 52.0), egui::Sense::click());
    let padded_rect = rect.shrink2(Vec2::new(4.0, 3.0));
    if response.hovered() {
        ui.painter().rect_filled(padded_rect, 10.0, CARD_ALT);
    }

    let color = if active { TEXT } else { MUTED };
    let font = FontId::proportional(15.0);
    let galley = ui.painter().layout_no_wrap(label.to_owned(), font, color);
    let icon_width = 28.0;
    let gap = 8.0;
    let content_width = icon_width + gap + galley.size().x;
    let content_left = rect.center().x - content_width / 2.0;
    let icon_center = egui::pos2(content_left + icon_width / 2.0, rect.center().y);
    draw_tab_icon(ui.painter(), icon, icon_center, color);
    ui.painter().galley(
        egui::pos2(
            content_left + icon_width + gap,
            rect.center().y - galley.size().y / 2.0,
        ),
        galley,
        color,
    );

    if active {
        let underline = egui::Rect::from_center_size(
            egui::pos2(rect.center().x, padded_rect.bottom() - 1.5),
            Vec2::new((content_width + 16.0).min(padded_rect.width()), 3.0),
        );
        ui.painter().rect_filled(underline, 2.0, ACCENT);
    }
    response
}

fn draw_tab_icon(painter: &egui::Painter, icon: TabIcon, center: egui::Pos2, color: Color32) {
    match icon {
        TabIcon::Ambient => {
            for (offset, half_height) in [(-9.0, 5.0), (-3.0, 10.0), (3.0, 14.0), (9.0, 7.0)] {
                painter.line_segment(
                    [
                        center + Vec2::new(offset, -half_height),
                        center + Vec2::new(offset, half_height),
                    ],
                    Stroke::new(2.2_f32, color),
                );
            }
        }
        TabIcon::Equalizer => {
            for (offset, knob_y) in [(-8.0, -5.0), (0.0, 6.0), (8.0, -1.0)] {
                painter.line_segment(
                    [
                        center + Vec2::new(offset, -12.0),
                        center + Vec2::new(offset, 12.0),
                    ],
                    Stroke::new(1.8_f32, color),
                );
                painter.circle_filled(center + Vec2::new(offset, knob_y), 3.0, color);
            }
        }
        TabIcon::Controls => {
            painter.circle_stroke(center, 10.0, Stroke::new(2.0_f32, color));
            painter.circle_filled(center, 3.5, color);
            for angle in [
                0.0_f32,
                std::f32::consts::FRAC_PI_2,
                std::f32::consts::PI,
                3.0 * std::f32::consts::FRAC_PI_2,
            ] {
                let direction = Vec2::angled(angle);
                painter.line_segment(
                    [center + direction * 10.0, center + direction * 14.0],
                    Stroke::new(2.0_f32, color),
                );
            }
        }
    }
}

fn status_dot(ui: &mut egui::Ui, color: Color32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(10.0), egui::Sense::hover());
    ui.painter().circle_filled(rect.center(), 4.0, color);
}

fn configure_style(context: &egui::Context) {
    let mut style = (*context.style()).clone();
    style.visuals = egui::Visuals::dark();
    style.visuals.panel_fill = BG;
    style.visuals.window_fill = CARD;
    style.visuals.widgets.inactive.bg_fill = CARD_ALT;
    style.visuals.widgets.hovered.bg_fill = Color32::from_rgb(43, 50, 58);
    style.visuals.widgets.active.bg_fill = ACCENT;
    style.visuals.selection.bg_fill = ACCENT;
    style.visuals.hyperlink_color = ACCENT;
    style.spacing.item_spacing = Vec2::new(10.0, 10.0);
    style.spacing.button_padding = Vec2::new(14.0, 9.0);
    context.set_style(style);
}

fn card(ui: &mut egui::Ui, content: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::new()
        .fill(CARD)
        .corner_radius(CornerRadius::same(18))
        .inner_margin(Margin::same(22))
        .show(ui, content);
}

fn section_title(ui: &mut egui::Ui, title: &str, subtitle: &str) {
    ui.vertical(|ui| {
        ui.label(RichText::new(title).size(19.0).color(TEXT).strong());
        ui.label(RichText::new(subtitle).size(12.0).color(MUTED));
    });
}

fn battery(ui: &mut egui::Ui, label: &str, value: Option<u8>) {
    ui.vertical_centered(|ui| {
        ui.label(RichText::new(label).size(12.0).color(MUTED));
        ui.add_space(5.0);
        ui.label(
            RichText::new(value.map_or_else(|| "—".into(), |value| format!("{value}%")))
                .font(FontId::proportional(25.0))
                .color(TEXT)
                .strong(),
        );
        ui.add_space(6.0);
        ui.add(
            egui::ProgressBar::new(f32::from(value.unwrap_or(0)) / 100.0)
                .fill(BATTERY)
                .desired_width(92.0),
        );
    });
}

fn load_png_texture(
    context: &egui::Context,
    name: &str,
    bytes: &[u8],
) -> Option<egui::TextureHandle> {
    let image = image::load_from_memory(bytes).ok()?.to_rgba8();
    load_rgba_texture(context, name, &image)
}

fn load_soundcore_texture(context: &egui::Context, bytes: &[u8]) -> Option<egui::TextureHandle> {
    let image = image::load_from_memory(bytes).ok()?.to_rgba8();
    let width = 378;
    let height = (image.height() * width / image.width()).max(1);
    let image =
        image::imageops::resize(&image, width, height, image::imageops::FilterType::Lanczos3);
    load_rgba_texture(context, "soundcore-logo", &image)
}

fn load_rgba_texture(
    context: &egui::Context,
    name: &str,
    image: &image::RgbaImage,
) -> Option<egui::TextureHandle> {
    let size = [image.width() as usize, image.height() as usize];
    let color_image = egui::ColorImage::from_rgba_unmultiplied(size, image.as_raw());
    let texture_options = egui::TextureOptions {
        mipmap_mode: Some(egui::TextureFilter::Linear),
        ..egui::TextureOptions::LINEAR
    };
    Some(context.load_texture(name, color_image, texture_options))
}

fn draw_buds(ui: &mut egui::Ui, texture: Option<&egui::TextureHandle>) {
    if let Some(texture) = texture {
        ui.add(
            egui::Image::new(texture)
                .fit_to_exact_size(Vec2::new(236.0, 136.0))
                .maintain_aspect_ratio(true),
        );
        return;
    }

    let (rect, _) = ui.allocate_exact_size(Vec2::new(154.0, 108.0), egui::Sense::hover());
    let painter = ui.painter();
    let left = rect.left_center() + Vec2::new(34.0, -8.0);
    let right = rect.right_center() + Vec2::new(-34.0, -8.0);
    for (center, direction) in [(left, -1.0), (right, 1.0)] {
        painter.circle_filled(center, 19.0, CARD_ALT);
        let stem = egui::Rect::from_center_size(
            center + Vec2::new(direction * 8.0, 25.0),
            Vec2::new(14.0, 38.0),
        );
        painter.rect_filled(stem, CornerRadius::same(7), CARD_ALT);
    }
}

fn preset_label(options: &[soundcore_control::domain::SelectOption], value: &str) -> String {
    options
        .iter()
        .find(|option| option.value == value)
        .map(|option| option.label.clone())
        .unwrap_or_else(|| value.to_owned())
}

fn format_frequency(frequency: u16) -> String {
    if frequency >= 1_000 {
        format!("{}k", frequency / 1_000)
    } else {
        frequency.to_string()
    }
}
