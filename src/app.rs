use std::{collections::HashMap, time::Duration};

use eframe::egui::{self, Align, Color32, CornerRadius, Layout, Margin, RichText, Vec2};
use soundcore_control::{
    device::{DeviceEvent, DeviceWorker},
    domain::{DeviceCommand, DeviceSnapshot},
    tray::{TrayAction, TrayController, TrayState},
};

use crate::ui::theme::{BG, configure_style};
use crate::ui::widgets::{load_png_texture, load_soundcore_texture};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum ActiveTab {
    #[default]
    Ambient,
    Equalizer,
    Controls,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) enum ConnectionView {
    #[default]
    Searching,
    Connecting,
    Connected,
    Disconnected,
    Failed,
}

pub(crate) struct SoundcoreApp {
    worker: DeviceWorker,
    tray: TrayController,
    pub(crate) connection: ConnectionView,
    pub(crate) device_name: String,
    pub(crate) snapshot: DeviceSnapshot,
    message: Option<String>,
    pub(crate) active_tab: ActiveTab,
    pub(crate) supports_manual_ambient_ranges: bool,
    pub(crate) current_icon_key: Option<String>,
    pub(crate) icon_textures: HashMap<String, egui::TextureHandle>,
    pub(crate) soundcore_texture: Option<egui::TextureHandle>,
    allow_exit: bool,
}

impl SoundcoreApp {
    pub(crate) fn new(context: &eframe::CreationContext<'_>) -> Self {
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
            current_icon_key: None,
            icon_textures: HashMap::new(),
            soundcore_texture: load_soundcore_texture(
                &context.egui_ctx,
                include_bytes!("../assets/soundcore.png"),
            ),
            allow_exit: false,
        }
    }

    /// Loads and caches a texture for `icon`'s bytes, keyed by device name, and makes it the
    /// active icon. `device_name` identifies the cache entry rather than the icon's own
    /// storage, since profile icons are loaded from the embedded device catalog at startup
    /// and aren't `'static` references we could otherwise key on.
    fn set_current_icon(
        &mut self,
        context: &egui::Context,
        device_name: &str,
        icon: Option<&[u8]>,
    ) {
        let Some(icon) = icon else {
            self.current_icon_key = None;
            return;
        };
        self.current_icon_key = Some(device_name.to_owned());
        if self.icon_textures.contains_key(device_name) {
            return;
        }
        if let Some(texture) = load_png_texture(context, "device-icon", icon) {
            self.icon_textures.insert(device_name.to_owned(), texture);
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
                    self.supports_manual_ambient_ranges = supports_manual_ambient_ranges;
                    self.set_current_icon(context, &name, icon.as_deref());
                    self.device_name = name;
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
                        button_controls = snapshot.button_controls.len(),
                        "connected to earbuds"
                    );
                    self.connection = ConnectionView::Connected;
                    self.supports_manual_ambient_ranges = supports_manual_ambient_ranges;
                    self.set_current_icon(context, &name, icon.as_deref());
                    self.device_name = name.clone();
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

    pub(crate) fn send(&mut self, command: DeviceCommand) {
        if let Err(error) = self.worker.send(command) {
            self.message = Some(error.to_string());
        }
    }

    pub(crate) fn reconnect(&mut self) {
        self.worker = DeviceWorker::spawn();
        self.connection = ConnectionView::Searching;
        self.tray.update(TrayState::searching());
        self.message = None;
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

    pub(crate) fn is_connected(&self) -> bool {
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
            crate::launch_tray_only();
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
