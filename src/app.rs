//! The app UI: microphone locks, pinned default devices, activity log, plus a
//! tray icon so locking keeps running when the window is closed. The footer
//! with the auto-update toggle and version is required on every factory app.

use crate::audio::{Config, DeviceInfo, Engine};
use crate::logic::{self, Flow, LockedMic, Pin, Slot};
use eframe::egui;
use std::sync::Arc;
use tray_icon::menu::{CheckMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};

const WIDTH: f32 = 520.0;
const HEIGHT: f32 = 580.0;

const ID_SHOW: &str = "show";
const ID_PAUSE: &str = "pause";
const ID_QUIT: &str = "quit";

pub fn run(args: Vec<String>) -> eframe::Result<()> {
    let start_hidden = args.iter().any(|a| a == "--minimized");
    let viewport = egui::ViewportBuilder::default()
        .with_title("Audio Lock")
        .with_inner_size([WIDTH, HEIGHT])
        .with_min_inner_size([420.0, 380.0])
        .with_visible(!start_hidden);
    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };
    eframe::run_native(
        "Audio Lock",
        options,
        Box::new(move |cc| Ok(Box::new(App::new(&cc.egui_ctx, start_hidden)))),
    )
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tab {
    Microphones,
    Defaults,
    Activity,
}

struct Tray {
    _icon: TrayIcon,
    pause_item: CheckMenuItem,
}

pub struct App {
    settings: crate::settings::Settings,
    version: String,
    /// "What would make this better?" (src/feedback.rs).
    feedback: crate::feedback::Feedback,
    engine: Arc<Engine>,
    tray: Option<Tray>,
    tab: Tab,
    autostart: bool,
    last_paused: bool,
}

impl App {
    fn new(ctx: &egui::Context, start_hidden: bool) -> Self {
        let settings = crate::settings::load();
        let engine = Arc::new(Engine::start(
            Config {
                locks: settings.locked_mics.clone(),
                pins: settings.pins.clone(),
                paused: false,
            },
            ctx.clone(),
        ));
        let tray = build_tray(ctx, engine.clone());
        if tray.is_none() && start_hidden {
            // No tray to restore from: never leave the app invisible.
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
        }
        Self {
            settings,
            version: crate::update::current_version(),
            feedback: Default::default(),
            engine,
            tray,
            tab: Tab::Microphones,
            autostart: crate::autostart::is_enabled(),
            last_paused: false,
        }
    }

    fn save_rules(&mut self) {
        crate::settings::save(&self.settings);
        self.engine
            .set_rules(self.settings.locked_mics.clone(), self.settings.pins.clone());
    }

    fn header(&mut self, ui: &mut egui::Ui) {
        let paused = self.engine.paused();
        let locks = self.settings.locked_mics.len();
        let pins = logic::Slot::ALL
            .iter()
            .filter(|s| self.settings.pins.get(**s).is_some())
            .count();
        ui.horizontal(|ui| {
            if paused {
                ui.colored_label(egui::Color32::from_rgb(220, 150, 40), "Paused");
                ui.weak("Changes are not being reverted.");
            } else if locks + pins == 0 {
                ui.weak("Nothing locked yet. Lock a microphone or pin a default device.");
            } else {
                ui.colored_label(egui::Color32::from_rgb(70, 170, 90), "Active");
                ui.weak(format!(
                    "{locks} microphone{} locked, {pins} default{} pinned",
                    if locks == 1 { "" } else { "s" },
                    if pins == 1 { "" } else { "s" },
                ));
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let label = if paused {
                    "Resume locking"
                } else {
                    "Pause locking"
                };
                if ui
                    .button(label)
                    .on_hover_text("Ctrl+P. Pause while you intend to change a level or device.")
                    .clicked()
                {
                    self.engine.set_paused(!paused);
                }
            });
        });
    }

    fn mics_tab(&mut self, ui: &mut egui::Ui) {
        let (mics, ready, error): (Vec<DeviceInfo>, bool, Option<String>) = {
            let s = self.engine.state();
            (
                s.snapshot
                    .devices
                    .iter()
                    .filter(|d| d.flow == Flow::Record)
                    .cloned()
                    .collect(),
                s.snapshot.ready,
                s.snapshot.error.clone(),
            )
        };
        if let Some(e) = error {
            ui.colored_label(egui::Color32::from_rgb(220, 80, 80), e);
            return;
        }
        if !ready {
            ui.weak("Looking for microphones...");
            return;
        }
        ui.label("Tick a microphone to hold its level. Apps that change it are reverted at once.");
        ui.add_space(6.0);
        let mut changed = false;
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            if mics.is_empty() && self.settings.locked_mics.is_empty() {
                ui.add_space(20.0);
                ui.vertical_centered(|ui| {
                    ui.strong("No microphones found");
                    ui.weak("Connect a microphone and it will appear here.");
                });
            }
            for mic in &mics {
                let pos = self.settings.locked_mics.iter().position(|l| l.id == mic.id);
                ui.group(|ui| {
                    ui.set_width(ui.available_width());
                    let mut locked = pos.is_some();
                    ui.horizontal(|ui| {
                        if ui.checkbox(&mut locked, &mic.name).changed() {
                            if locked {
                                self.settings.locked_mics.push(LockedMic {
                                    id: mic.id.clone(),
                                    name: mic.name.clone(),
                                    level: mic.volume.unwrap_or(1.0),
                                });
                            } else if let Some(p) = pos {
                                self.settings.locked_mics.remove(p);
                            }
                            changed = true;
                        }
                    });
                    match pos.and_then(|p| self.settings.locked_mics.get_mut(p)) {
                        Some(lock) => {
                            let mut pct = logic::percent(lock.level) as f32;
                            ui.horizontal(|ui| {
                                ui.label("Locked at");
                                if ui
                                    .add(egui::Slider::new(&mut pct, 0.0..=100.0).suffix("%").integer())
                                    .changed()
                                {
                                    lock.level = pct / 100.0;
                                    changed = true;
                                }
                            });
                            if let Some(v) = mic.volume {
                                ui.weak(format!("Current level {}%", logic::percent(v)));
                            }
                        }
                        None => {
                            if let Some(v) = mic.volume {
                                ui.weak(format!("Level {}% (not locked)", logic::percent(v)));
                            }
                        }
                    }
                });
            }
            // Locked microphones that are currently unplugged.
            let mut remove: Option<usize> = None;
            for (i, lock) in self.settings.locked_mics.iter().enumerate() {
                if mics.iter().any(|m| m.id == lock.id) {
                    continue;
                }
                ui.group(|ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        ui.label(format!("{} (not connected)", lock.name));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.button("Remove").clicked() {
                                remove = Some(i);
                            }
                        });
                    });
                    ui.weak(format!(
                        "Will be locked at {}% when it is connected again.",
                        logic::percent(lock.level)
                    ));
                });
            }
            if let Some(i) = remove {
                self.settings.locked_mics.remove(i);
                changed = true;
            }
        });
        if changed {
            self.save_rules();
        }
    }

    fn defaults_tab(&mut self, ui: &mut egui::Ui) {
        let (devices, defaults, ready) = {
            let s = self.engine.state();
            (
                s.snapshot.devices.clone(),
                s.snapshot.defaults.clone(),
                s.snapshot.ready,
            )
        };
        if !ready {
            ui.weak("Looking for audio devices...");
            return;
        }
        ui.label("Pin a device and Windows (or an app) can no longer switch away from it.");
        ui.add_space(6.0);
        let mut changed = false;
        for slot in Slot::ALL {
            ui.group(|ui| {
                ui.set_width(ui.available_width());
                ui.strong(slot.label());
                let pinned = self.settings.pins.get(slot).cloned();
                let selected = match &pinned {
                    None => "Not pinned".to_string(),
                    Some(p) if devices.iter().any(|d| d.id == p.id) => p.name.clone(),
                    Some(p) => format!("{} (not connected)", p.name),
                };
                let options: Vec<&DeviceInfo> =
                    devices.iter().filter(|d| d.flow == slot.flow()).collect();
                egui::ComboBox::from_id_salt(slot.index())
                    .width(ui.available_width() - 8.0)
                    .selected_text(selected)
                    .show_ui(ui, |ui| {
                        if ui.selectable_label(pinned.is_none(), "Not pinned").clicked() {
                            self.settings.pins.set(slot, None);
                            changed = true;
                        }
                        for d in &options {
                            let is = pinned.as_ref().is_some_and(|p| p.id == d.id);
                            if ui.selectable_label(is, &d.name).clicked() {
                                self.settings.pins.set(
                                    slot,
                                    Some(Pin {
                                        id: d.id.clone(),
                                        name: d.name.clone(),
                                    }),
                                );
                                changed = true;
                            }
                        }
                    });
                if options.is_empty() {
                    ui.weak("No devices found.");
                } else if let Some(cur) = defaults[slot.index()]
                    .as_ref()
                    .and_then(|id| devices.iter().find(|d| &d.id == id))
                {
                    ui.weak(format!("Windows is currently using: {}", cur.name));
                }
            });
        }
        if changed {
            self.save_rules();
        }
    }

    fn activity_tab(&mut self, ui: &mut egui::Ui) {
        let entries = self.engine.state().log.newest_first();
        ui.horizontal(|ui| {
            ui.label("What Audio Lock changed back, newest first.");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .add_enabled(!entries.is_empty(), egui::Button::new("Clear"))
                    .clicked()
                {
                    self.engine.state().log.clear();
                }
            });
        });
        ui.add_space(6.0);
        if entries.is_empty() {
            ui.add_space(20.0);
            ui.vertical_centered(|ui| {
                ui.strong("Nothing yet");
                ui.weak("When an app changes a locked level or Windows switches a pinned device, it is listed here.");
            });
            return;
        }
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            for e in &entries {
                ui.horizontal_wrapped(|ui| {
                    ui.weak(&e.time);
                    if e.count > 1 {
                        ui.label(format!("{} (x{})", e.message, e.count));
                    } else {
                        ui.label(&e.message);
                    }
                });
            }
        });
    }

    fn footer(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            let mut auto_update = self.settings.auto_update;
            if ui
                .checkbox(&mut auto_update, "Automatically update")
                .on_hover_text("Checks for new versions about once a day and installs them in the background.")
                .changed()
            {
                self.settings.auto_update = auto_update;
                crate::settings::save(&self.settings);
            }
            if ui
                .checkbox(&mut self.autostart, "Start with Windows")
                .on_hover_text("Starts Audio Lock hidden in the tray when you sign in.")
                .changed()
            {
                crate::autostart::set_enabled(self.autostart);
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.weak(format!("v{}", self.version));
                ui.add_space(8.0);
                self.feedback.link(ui);
            });
        });
    }

    fn sync_tray(&mut self) {
        let paused = self.engine.paused();
        if paused == self.last_paused {
            return;
        }
        self.last_paused = paused;
        if let Some(t) = &self.tray {
            t.pause_item.set_checked(paused);
            let _ = t._icon.set_tooltip(Some(if paused {
                "Audio Lock (paused)"
            } else {
                "Audio Lock"
            }));
        }
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui_root: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui_root.ctx().clone();

        // Closing the window keeps locking alive in the tray.
        if self.tray.is_some() && ctx.input(|i| i.viewport().close_requested()) {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::P)) {
            let p = self.engine.paused();
            self.engine.set_paused(!p);
        }
        for (key, tab) in [
            (egui::Key::Num1, Tab::Microphones),
            (egui::Key::Num2, Tab::Defaults),
            (egui::Key::Num3, Tab::Activity),
        ] {
            if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, key)) {
                self.tab = tab;
            }
        }
        self.sync_tray();

        egui::Panel::bottom("footer").show(ui_root, |ui| self.footer(ui));
        egui::CentralPanel::default().show(ui_root, |ui| {
            self.header(ui);
            ui.separator();
            ui.horizontal(|ui| {
                ui.selectable_value(&mut self.tab, Tab::Microphones, "Microphones");
                ui.selectable_value(&mut self.tab, Tab::Defaults, "Default devices");
                let n = self.engine.state().log.len();
                let label = if n == 0 {
                    "Activity".to_string()
                } else {
                    format!("Activity ({n})")
                };
                ui.selectable_value(&mut self.tab, Tab::Activity, label);
            });
            ui.separator();
            match self.tab {
                Tab::Microphones => self.mics_tab(ui),
                Tab::Defaults => self.defaults_tab(ui),
                Tab::Activity => self.activity_tab(ui),
            }
        });
            let feedback_ctx = ui_root.ctx().clone();
        self.feedback.show(&feedback_ctx, &self.version, false);
    }
}

/// 32x32 blue disc with a white padlock-like glyph.
fn make_icon() -> Option<Icon> {
    const N: u32 = 32;
    let mut rgba = vec![0u8; (N * N * 4) as usize];
    for y in 0..N {
        for x in 0..N {
            let (dx, dy) = (x as f32 - 15.5, y as f32 - 15.5);
            let i = ((y * N + x) * 4) as usize;
            let in_disc = dx * dx + dy * dy <= 15.0 * 15.0;
            let body = (10..=21).contains(&x) && (15..=24).contains(&y);
            let r = (dx * dx + (y as f32 - 14.0).powi(2)).sqrt();
            let shackle = y <= 15 && (4.5..=6.5).contains(&r) && y >= 6;
            let px = if in_disc && (body || shackle) {
                [255, 255, 255, 255]
            } else if in_disc {
                [37, 99, 235, 255]
            } else {
                [0, 0, 0, 0]
            };
            rgba[i..i + 4].copy_from_slice(&px);
        }
    }
    Icon::from_rgba(rgba, N, N).ok()
}

fn build_tray(ctx: &egui::Context, engine: Arc<Engine>) -> Option<Tray> {
    let menu = Menu::new();
    let show = MenuItem::with_id(ID_SHOW, "Open Audio Lock", true, None);
    let pause = CheckMenuItem::with_id(ID_PAUSE, "Pause locking", true, false, None);
    let quit = MenuItem::with_id(ID_QUIT, "Quit", true, None);
    menu.append_items(&[&show, &pause, &PredefinedMenuItem::separator(), &quit])
        .ok()?;
    let icon = TrayIconBuilder::new()
        .with_menu(Box::new(menu))
        .with_menu_on_left_click(false)
        .with_tooltip("Audio Lock")
        .with_icon(make_icon()?)
        .build()
        .ok()?;

    let show_ctx = ctx.clone();
    TrayIconEvent::set_event_handler(Some(move |e: TrayIconEvent| {
        if let TrayIconEvent::Click {
            button: MouseButton::Left,
            button_state: MouseButtonState::Up,
            ..
        } = e
        {
            show_window(&show_ctx);
        }
    }));
    let menu_ctx = ctx.clone();
    MenuEvent::set_event_handler(Some(move |e: MenuEvent| match e.id.as_ref() {
        ID_SHOW => show_window(&menu_ctx),
        // The check item flips itself; mirror it into the engine.
        ID_PAUSE => {
            let p = engine.paused();
            engine.set_paused(!p);
            menu_ctx.request_repaint();
        }
        ID_QUIT => std::process::exit(0),
        _ => {}
    }));
    Some(Tray {
        _icon: icon,
        pause_item: pause,
    })
}

fn show_window(ctx: &egui::Context) {
    ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
    ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
    ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
    ctx.request_repaint();
}
