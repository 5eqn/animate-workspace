use crate::{
    audio::{DecodedAudio, Transport},
    export::ExportJob,
    model::*,
    project,
    render::{self, Ghost, Renderer, View},
    tablet::Tablet,
};
use eframe::egui::{
    self, Align, Color32, FontId, Key, Layout, Pos2, Rect, RichText, Sense, Stroke as UiStroke,
    Vec2,
};
use std::{
    path::PathBuf,
    sync::{
        mpsc::{self, Receiver, TryRecvError, TrySendError},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};
const BG: Color32 = Color32::from_rgb(23, 25, 27);
const PANEL: Color32 = Color32::from_rgb(29, 31, 33);
const INK: Color32 = Color32::from_rgb(232, 231, 225);
const MUTED: Color32 = Color32::from_rgb(135, 140, 143);
const ACCENT: Color32 = Color32::from_rgb(214, 177, 132);
const LINE: Color32 = Color32::from_rgb(51, 54, 57);
#[derive(Clone, Copy, PartialEq)]
enum Page {
    Canvas,
    Timeline,
    Settings,
}
#[derive(Clone, Copy, PartialEq)]
enum Tool {
    Ink,
    Liquify,
}
struct Drawing {
    layer: usize,
    time: Tick,
    last: Point,
    previous: Point,
    liquify: bool,
    index: Option<SpatialIndex>,
}
enum Background {
    Imported(AudioAsset, DecodedAudio),
    Loaded(PathBuf, Box<Document>, Option<DecodedAudio>),
    Saved(PathBuf),
    Error(String),
}
#[derive(Clone, Copy)]
enum Pending {
    New,
    Open,
    Close,
}
pub struct Studio {
    doc: Document,
    history: History,
    page: Page,
    tool: Tool,
    layer: usize,
    time: f64,
    mirror: bool,
    dirty: bool,
    path: Option<PathBuf>,
    renderer: Arc<Mutex<Renderer>>,
    transport: Transport,
    drawing: Option<Drawing>,
    tablet: Tablet,
    audio_bytes: Option<Arc<Vec<u8>>>,
    toast: Option<(String, Instant)>,
    resized: Instant,
    export: Option<ExportJob>,
    export_renderer: Option<Renderer>,
    background: Option<Receiver<Background>>,
    busy: Option<String>,
    bind_capture: Option<usize>,
    ui_editing: bool,
    timeline_zoom: f32,
    timeline_start: f64,
    pending: Option<Pending>,
    allow_close: bool,
    fullscreen: bool,
    canvas_zoom: f32,
    canvas_pan: Vec2,
    smoke: Option<PathBuf>,
    smoke_finish: Option<PathBuf>,
    smoke_started: Instant,
}
fn text(ui: &mut egui::Ui, s: impl Into<String>, size: f32, color: Color32) {
    ui.label(RichText::new(s).size(size).color(color));
}
fn section(ui: &mut egui::Ui, kicker: &str, title: &str, description: &str) {
    ui.add_space(12.0);
    text(ui, kicker, 10.0, ACCENT);
    ui.add_space(5.0);
    text(ui, title, 25.0, INK);
    text(ui, description, 12.0, MUTED);
    ui.add_space(16.0);
}
fn chip(ui: &mut egui::Ui, label: &str, selected: bool) -> egui::Response {
    ui.add(
        egui::Button::new(
            RichText::new(label)
                .size(12.0)
                .color(if selected { BG } else { MUTED }),
        )
        .fill(if selected {
            ACCENT
        } else {
            Color32::TRANSPARENT
        })
        .corner_radius(6.0)
        .min_size(Vec2::new(64.0, 30.0)),
    )
}
impl Studio {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let mut style = (*cc.egui_ctx.style()).clone();
        style.visuals = egui::Visuals::dark();
        style.visuals.panel_fill = BG;
        style.visuals.window_fill = PANEL;
        style.visuals.extreme_bg_color = Color32::from_rgb(18, 20, 22);
        style.visuals.faint_bg_color = PANEL;
        style.visuals.override_text_color = Some(INK);
        style.visuals.selection.bg_fill = ACCENT.gamma_multiply(0.4);
        style.visuals.widgets.inactive.bg_fill = Color32::from_rgb(38, 41, 44);
        style.visuals.widgets.inactive.weak_bg_fill = Color32::from_rgb(38, 41, 44);
        style.visuals.widgets.inactive.bg_stroke = UiStroke::new(1.0_f32, LINE);
        style.visuals.widgets.hovered.bg_fill = Color32::from_rgb(54, 57, 59);
        style.visuals.widgets.active.bg_fill = ACCENT.gamma_multiply(0.5);
        style.spacing.item_spacing = Vec2::new(10.0, 9.0);
        style.spacing.button_padding = Vec2::new(12.0, 7.0);
        cc.egui_ctx.set_style(style);
        let renderer = Arc::new(Mutex::new(unsafe {
            Renderer::new(cc.gl.as_ref().expect("OpenGL renderer"))
                .expect("GPU shaders must compile")
        }));
        let args: Vec<String> = std::env::args().collect();
        let smoke = args
            .iter()
            .position(|a| a == "--smoke")
            .and_then(|i| args.get(i + 1))
            .map(PathBuf::from);
        let demo = args.iter().any(|a| a == "--demo") || smoke.is_some();
        let mut studio = Self {
            doc: Document::default(),
            history: History::default(),
            page: Page::Canvas,
            tool: Tool::Ink,
            layer: 0,
            time: 0.0,
            mirror: false,
            dirty: false,
            path: None,
            renderer,
            transport: Transport::default(),
            drawing: None,
            tablet: Tablet::new(cc.egui_ctx.clone()),
            audio_bytes: None,
            toast: None,
            resized: Instant::now() - Duration::from_secs(10),
            export: None,
            export_renderer: None,
            background: None,
            busy: None,
            bind_capture: None,
            ui_editing: false,
            timeline_zoom: 1.0,
            timeline_start: 0.0,
            pending: None,
            allow_close: false,
            fullscreen: false,
            canvas_zoom: 1.0,
            canvas_pan: Vec2::ZERO,
            smoke,
            smoke_finish: None,
            smoke_started: Instant::now(),
        };
        if demo {
            studio.make_demo();
        }
        if let Some(path) = args.iter().skip(1).find(|a| a.ends_with(".mori")) {
            studio.open_path(PathBuf::from(path));
        }
        studio
    }
    fn notify(&mut self, message: impl Into<String>) {
        self.toast = Some((message.into(), Instant::now()));
    }
    fn record(&mut self) {
        self.history.record(&self.doc);
        self.dirty = true;
    }
    fn set_time(&mut self, time: f64) {
        self.finish_stroke();
        self.transport.stop();
        self.time = self.doc.timing.snap(time).max(0.0);
    }
    fn finish_stroke(&mut self) {
        self.drawing = None;
    }
    fn action(&mut self, action: &str, ctx: &egui::Context) {
        if self.drawing.is_some() && !matches!(action, "Smaller" | "Larger") {
            return;
        }
        match action {
            "Canvas" => self.page = Page::Canvas,
            "Timeline" => self.page = Page::Timeline,
            "Settings" => self.page = Page::Settings,
            "Save" => self.save(false),
            "Open" => self.request(Pending::Open),
            "New" => self.request(Pending::New),
            "Undo" | "Redo" => {
                self.transport.stop();
                let changed = if action == "Undo" {
                    self.history.undo(&mut self.doc)
                } else {
                    self.history.redo(&mut self.doc)
                };
                if changed {
                    self.layer = self.layer.min(self.doc.layers.len() - 1);
                    self.time = self.doc.timing.snap(self.time);
                    self.dirty = true;
                    self.sync_audio();
                    self.notify(action);
                } else {
                    self.notify(format!("Nothing to {}", action.to_lowercase()));
                }
            }
            "Liquify" => {
                self.tool = if self.tool == Tool::Ink {
                    Tool::Liquify
                } else {
                    Tool::Ink
                };
                self.notify(if self.tool == Tool::Ink {
                    "Ink brush"
                } else {
                    "Liquify · pressure preserved"
                });
            }
            "Mirror" => {
                self.mirror = !self.mirror;
                self.notify(if self.mirror {
                    "Canvas mirrored"
                } else {
                    "Canvas restored"
                });
            }
            "Previous" => self.set_time(self.doc.timing.previous(self.time)),
            "Next" => self.set_time(self.doc.timing.next(self.time)),
            "Layer up" => {
                self.layer = (self.layer + 1).min(self.doc.layers.len() - 1);
                self.notify(format!(
                    "Layer {} · {}",
                    self.layer + 1,
                    self.doc.layers[self.layer].name
                ));
            }
            "Layer down" => {
                self.layer = self.layer.saturating_sub(1);
                self.notify(format!(
                    "Layer {} · {}",
                    self.layer + 1,
                    self.doc.layers[self.layer].name
                ));
            }
            "Smaller" | "Larger" => {
                if self.drawing.is_none() {
                    self.record();
                }
                let width = &mut self.doc.layers[self.layer].width;
                *width = if action == "Larger" {
                    (*width * 1.15).max(*width + 1.0)
                } else {
                    (*width / 1.15).min(*width - 1.0)
                }
                .clamp(1.0, 1920.0);
                let size = *width;
                self.resized = Instant::now();
                self.notify(format!("Brush · {size:.0} px"));
            }
            "Play" => self.toggle_play(),
            "Onion skin" => {
                self.record();
                self.doc.settings.onion.enabled = !self.doc.settings.onion.enabled;
                self.notify(if self.doc.settings.onion.enabled {
                    "Onion skin on"
                } else {
                    "Onion skin off"
                });
            }
            "Fullscreen" => {
                self.fullscreen = !self.fullscreen;
                ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(self.fullscreen));
            }
            _ => {}
        }
    }
    fn keyboard(&mut self, ctx: &egui::Context) {
        if self.pending.is_some() || self.background.is_some() {
            return;
        }
        let events = ctx.input(|i| i.events.clone());
        for event in events {
            if let egui::Event::Key {
                key,
                physical_key,
                pressed: true,
                modifiers,
                repeat,
                ..
            } = event
            {
                if let Some(index) = self.bind_capture {
                    if key == Key::Escape {
                        self.bind_capture = None;
                        continue;
                    }
                    let name = key.symbol_or_name().to_string();
                    let conflict = self
                        .doc
                        .settings
                        .bindings
                        .iter()
                        .enumerate()
                        .find(|(i, b)| {
                            *i != index
                                && b.key == name
                                && b.control
                                    == (modifiers.ctrl
                                        && (cfg!(target_os = "macos") || !modifiers.command))
                                && b.command == modifiers.command
                                && b.alt == modifiers.alt
                                && b.shift == modifiers.shift
                        })
                        .map(|(_, b)| b.action.clone());
                    if let Some(action) = conflict {
                        self.notify(format!("Already used by {action}"));
                    } else {
                        self.record();
                        let binding = &mut self.doc.settings.bindings[index];
                        binding.key = name;
                        binding.control =
                            modifiers.ctrl && (cfg!(target_os = "macos") || !modifiers.command);
                        binding.command = modifiers.command;
                        binding.alt = modifiers.alt;
                        binding.shift = modifiers.shift;
                        self.bind_capture = None;
                    }
                    continue;
                }
                if ctx.wants_keyboard_input()
                    && !modifiers.command
                    && !modifiers.alt
                    && !modifiers.ctrl
                {
                    continue;
                }
                let action = self
                    .doc
                    .settings
                    .bindings
                    .iter()
                    .find(|b| {
                        let matches = Key::from_name(&b.key)
                            .is_some_and(|k| k == key || physical_key == Some(k));
                        matches
                            && b.control
                                == (modifiers.ctrl
                                    && (cfg!(target_os = "macos") || !modifiers.command))
                            && b.command == modifiers.command
                            && b.alt == modifiers.alt
                            && b.shift == modifiers.shift
                    })
                    .map(|b| b.action.clone());
                if let Some(action) = action {
                    if !repeat
                        || matches!(
                            action.as_str(),
                            "Previous" | "Next" | "Smaller" | "Larger" | "Layer up" | "Layer down"
                        )
                    {
                        self.action(&action, ctx);
                    }
                }
            }
        }
    }
    fn toggle_play(&mut self) {
        if self.transport.playing {
            let t = self.transport.position();
            self.set_time(t);
        } else {
            if self.time >= self.doc.timing.duration {
                self.time = 0.0;
            }
            if let Err(error) = self.transport.play(self.time) {
                self.notify(error);
            }
        }
    }
    fn sync_audio(&mut self) {
        let current = self.doc.audio.as_ref().map(|a| a.bytes.clone());
        let same = match (&current, &self.audio_bytes) {
            (Some(a), Some(b)) => Arc::ptr_eq(a, b),
            (None, None) => true,
            _ => false,
        };
        if same {
            return;
        }
        self.audio_bytes = current.clone();
        if let Some(bytes) = current {
            match DecodedAudio::decode(&bytes) {
                Ok(audio) => self.transport.audio = Some(audio),
                Err(e) => self.notify(e),
            }
        } else {
            self.transport.audio = None;
        }
    }
    fn request(&mut self, pending: Pending) {
        self.transport.stop();
        if self.dirty || (matches!(pending, Pending::Close) && self.export.is_some()) {
            self.pending = Some(pending);
        } else {
            self.perform(pending);
        }
    }
    fn perform(&mut self, pending: Pending) {
        match pending {
            Pending::New => {
                self.doc = Document::default();
                self.history = History::default();
                self.path = None;
                self.layer = 0;
                self.time = 0.0;
                self.dirty = false;
                self.transport = Transport::default();
                self.audio_bytes = None;
                self.notify("New motion · 2.39:1 · four layers");
            }
            Pending::Open => {
                if let Some(path) = rfd::FileDialog::new()
                    .add_filter("MORI project", &["mori"])
                    .pick_file()
                {
                    self.open_path(path);
                }
            }
            Pending::Close => {
                self.export = None;
                self.allow_close = true;
            }
        }
    }
    fn open_path(&mut self, path: PathBuf) {
        let (tx, rx) = mpsc::channel();
        self.background = Some(rx);
        self.busy = Some("Opening project…".into());
        std::thread::spawn(move || {
            let result = project::load(&path).and_then(|doc| {
                let audio = doc
                    .audio
                    .as_ref()
                    .map(|a| DecodedAudio::decode(&a.bytes))
                    .transpose()?;
                Ok(Background::Loaded(path, Box::new(doc), audio))
            });
            let _ = tx.send(result.unwrap_or_else(Background::Error));
        });
    }
    fn save(&mut self, save_as: bool) {
        if self.background.is_some() {
            return;
        }
        let path = if save_as || self.path.is_none() {
            rfd::FileDialog::new()
                .add_filter("MORI project", &["mori"])
                .set_file_name(format!("{}.mori", self.doc.name))
                .save_file()
        } else {
            self.path.clone()
        };
        if let Some(path) = path {
            let (tx, rx) = mpsc::channel();
            self.background = Some(rx);
            self.busy = Some("Saving vectors and music…".into());
            let doc = self.doc.clone();
            std::thread::spawn(move || {
                let outcome = project::save(&path, &doc).map(|_| Background::Saved(path));
                let _ = tx.send(outcome.unwrap_or_else(Background::Error));
            });
        }
    }
    fn import_audio(&mut self) {
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("Music", &["wav", "mp3", "flac", "ogg", "m4a", "aac"])
            .pick_file()
        {
            self.transport.stop();
            let (tx, rx) = mpsc::channel();
            self.background = Some(rx);
            self.busy = Some("Reading music and waveform…".into());
            std::thread::spawn(move || {
                let outcome = std::fs::read(&path)
                    .map_err(|e| e.to_string())
                    .and_then(|bytes| {
                        let decoded = DecodedAudio::decode(&bytes)?;
                        Ok(Background::Imported(
                            AudioAsset {
                                name: path
                                    .file_name()
                                    .unwrap_or_default()
                                    .to_string_lossy()
                                    .into_owned(),
                                bytes: Arc::new(bytes),
                            },
                            decoded,
                        ))
                    });
                let _ = tx.send(outcome.unwrap_or_else(Background::Error));
            });
        }
    }
    fn poll_background(&mut self) {
        let result = self.background.as_ref().map(|rx| rx.try_recv());
        match result {
            Some(Ok(outcome)) => {
                self.background = None;
                self.busy = None;
                match outcome {
                    Background::Imported(asset, audio) => {
                        self.record();
                        self.doc.timing.duration = self.doc.timing.duration.max(audio.duration);
                        self.doc.settings.export.end = self.doc.timing.duration;
                        self.audio_bytes = Some(asset.bytes.clone());
                        self.doc.audio = Some(asset);
                        self.transport.audio = Some(audio);
                        self.notify("Music imported · click the ruler to set your playhead");
                    }
                    Background::Loaded(path, doc, audio) => {
                        self.path = Some(path);
                        self.audio_bytes = doc.audio.as_ref().map(|a| a.bytes.clone());
                        self.doc = *doc;
                        self.transport.stop();
                        self.transport.audio = audio;
                        self.history = History::default();
                        self.dirty = false;
                        self.layer = 0;
                        self.time = 0.0;
                        self.notify("Project opened");
                    }
                    Background::Saved(path) => {
                        self.path = Some(path);
                        self.dirty = false;
                        self.notify("Project saved · vectors + embedded music");
                        if let Some(pending) = self.pending.take() {
                            self.perform(pending);
                        }
                    }
                    Background::Error(error) => {
                        self.notify(error);
                    }
                }
            }
            Some(Err(TryRecvError::Disconnected)) => {
                self.background = None;
                self.busy = None;
                self.notify("Background operation ended unexpectedly");
            }
            _ => {}
        }
    }
    fn header(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("header")
            .exact_height(68.0)
            .frame(
                egui::Frame::new()
                    .fill(BG)
                    .inner_margin(egui::Margin::symmetric(24, 15)),
            )
            .show(ctx, |ui| {
                if self.background.is_some() || self.pending.is_some() {
                    ui.disable();
                }
                ui.horizontal_centered(|ui| {
                    ui.label(RichText::new("M O R I").size(22.0).strong().color(INK));
                    ui.add_space(12.0);
                    ui.menu_button("File", |ui| {
                        if ui.button("New motion").clicked() {
                            self.request(Pending::New);
                            ui.close_menu();
                        }
                        if ui.button("Open…").clicked() {
                            self.request(Pending::Open);
                            ui.close_menu();
                        }
                        if ui.button("Save").clicked() {
                            self.save(false);
                            ui.close_menu();
                        }
                        if ui.button("Save as…").clicked() {
                            self.save(true);
                            ui.close_menu();
                        }
                        ui.separator();
                        if ui.button("Import music…").clicked() {
                            self.import_audio();
                            ui.close_menu();
                        }
                    });
                    ui.add_space(22.0);
                    if chip(ui, "01  Canvas", self.page == Page::Canvas)
                        .on_hover_text(self.binding("Canvas"))
                        .clicked()
                    {
                        self.page = Page::Canvas;
                    }
                    if chip(ui, "02  Rhythm", self.page == Page::Timeline)
                        .on_hover_text(self.binding("Timeline"))
                        .clicked()
                    {
                        self.page = Page::Timeline;
                    }
                    if chip(ui, "Settings", self.page == Page::Settings)
                        .on_hover_text(self.binding("Settings"))
                        .clicked()
                    {
                        self.page = Page::Settings;
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui
                            .button(if self.transport.playing {
                                "Ⅱ  Pause"
                            } else {
                                "▶  Play"
                            })
                            .clicked()
                        {
                            self.toggle_play();
                        }
                        ui.add_space(12.0);
                        text(
                            ui,
                            format!("{}{}", self.doc.name, if self.dirty { "  •" } else { "" }),
                            12.0,
                            MUTED,
                        );
                    });
                });
            });
    }
    fn binding(&self, action: &str) -> String {
        self.doc
            .settings
            .bindings
            .iter()
            .find(|b| b.action == action)
            .map_or(String::new(), |b| b.label())
    }
    fn footer(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::bottom("footer")
            .exact_height(42.0)
            .frame(
                egui::Frame::new()
                    .fill(BG)
                    .inner_margin(egui::Margin::symmetric(24, 9)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let layer = &self.doc.layers[self.layer];
                    ui.label(
                        RichText::new(format!(
                            "{:02} / {:02}",
                            self.layer + 1,
                            self.doc.layers.len()
                        ))
                        .color(ACCENT)
                        .monospace()
                        .size(12.0),
                    );
                    text(
                        ui,
                        format!("{}  ·  {}", layer.name, layer.blend.name()),
                        12.0,
                        INK,
                    );
                    ui.add_space(16.0);
                    text(
                        ui,
                        if self.tool == Tool::Ink {
                            "Ink"
                        } else {
                            "Liquify"
                        },
                        12.0,
                        MUTED,
                    );
                    text(ui, format!("{:.0} px", layer.width), 12.0, MUTED);
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        text(ui, format!("{:.3} s", self.time), 12.0, INK);
                        text(
                            ui,
                            format!(
                                "{:.0} BPM",
                                self.doc.timing.tempos[self.doc.timing.segment(self.time)].bpm
                            ),
                            11.0,
                            MUTED,
                        );
                        if let Some(job) = &self.export {
                            text(
                                ui,
                                format!(
                                    "EXPORT {:.0}%",
                                    job.frame as f32 / job.frames as f32 * 100.0
                                ),
                                10.0,
                                ACCENT,
                            );
                        }
                        if self.mirror {
                            text(ui, "MIRRORED", 10.0, ACCENT);
                        }
                        if self.doc.settings.onion.enabled {
                            text(ui, "ONION", 10.0, MUTED);
                        }
                        if let Some((message, when)) = &self.toast {
                            if when.elapsed() < Duration::from_secs(5) {
                                text(ui, message, 11.0, ACCENT);
                            }
                        }
                    });
                });
            });
    }
    fn view(&self, onion: bool) -> View {
        let mut ghosts = Vec::new();
        let settings = &self.doc.settings.onion;
        if onion && settings.enabled && !self.transport.playing {
            // Distinct exposure times, rather than repeating a held cel at every musical tick.
            let times: std::collections::BTreeSet<Tick> = self
                .doc
                .layers
                .iter()
                .flat_map(|l| l.cels.keys().copied())
                .collect();
            for (i, t) in times
                .range(..tick(self.time))
                .rev()
                .take(settings.previous)
                .enumerate()
            {
                ghosts.push(Ghost {
                    scene: self.doc.scene(*t),
                    color: settings.before,
                    opacity: settings.opacity * settings.falloff.powi(i as i32),
                });
            }
            for (i, t) in times
                .range((
                    std::ops::Bound::Excluded(tick(self.time)),
                    std::ops::Bound::Unbounded,
                ))
                .take(settings.next)
                .enumerate()
            {
                ghosts.push(Ghost {
                    scene: self.doc.scene(*t),
                    color: settings.after,
                    opacity: settings.opacity * settings.falloff.powi(i as i32),
                });
            }
        }
        View {
            scene: self.doc.scene(tick(if self.transport.playing {
                self.transport.position()
            } else {
                self.time
            })),
            ghosts,
            mirror: self.mirror,
        }
    }
    fn canvas_page(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.horizontal(|ui| {
            ui.add_space(16.0);
            if chip(ui, "Ink", self.tool == Tool::Ink).clicked() {
                self.tool = Tool::Ink;
            }
            if chip(ui, "Liquify", self.tool == Tool::Liquify)
                .on_hover_text(self.binding("Liquify"))
                .clicked()
            {
                self.tool = Tool::Liquify;
            }
            ui.separator();
            let mut width = self.doc.layers[self.layer].width;
            let response = ui.add(
                egui::DragValue::new(&mut width)
                    .range(1.0..=1920.0)
                    .speed(0.5)
                    .suffix(" px"),
            );
            if response.changed() {
                if !self.ui_editing {
                    self.record();
                }
                self.ui_editing = ctx.input(|i| i.pointer.any_down());
                self.doc.layers[self.layer].width = width;
                self.dirty = true;
                self.resized = Instant::now();
            }
            if ui
                .selectable_label(self.doc.settings.onion.enabled, "Onion")
                .clicked()
            {
                self.action("Onion skin", ctx);
            }
            if ui.selectable_label(self.mirror, "Mirror").clicked() {
                self.action("Mirror", ctx);
            }
            let (undo, redo) = self.history.counts();
            if ui
                .add_enabled(undo > 0, egui::Button::new("Undo"))
                .on_hover_text("Undo")
                .clicked()
            {
                self.action("Undo", ctx);
            }
            if ui
                .add_enabled(redo > 0, egui::Button::new("Redo"))
                .on_hover_text("Redo")
                .clicked()
            {
                self.action("Redo", ctx);
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui.small_button("Fit").clicked() {
                    self.canvas_zoom = 1.0;
                    self.canvas_pan = Vec2::ZERO;
                }
                text(ui, format!("{:.0}%", self.canvas_zoom * 100.0), 11.0, MUTED);
                ui.add_space(12.0);
                text(ui, "W / S  layer    A / D  time", 11.0, MUTED);
            });
        });
        ui.add_space(8.0);
        let bottom = 62.0;
        let available = ui.available_rect_before_wrap();
        let workspace = Rect::from_min_max(
            available.min,
            Pos2::new(available.max.x, available.max.y - bottom),
        );
        let ws = ui.allocate_rect(workspace, Sense::click_and_drag());
        let padding = Vec2::new(42.0, 36.0);
        let fit = (workspace.width() - padding.x * 2.0)
            .min((workspace.height() - padding.y * 2.0) * self.doc.settings.aspect);
        let size = Vec2::new(fit, fit / self.doc.settings.aspect) * self.canvas_zoom;
        if ws.hovered() {
            let zoom = ctx.input(|i| i.zoom_delta());
            if zoom != 1.0 {
                self.canvas_zoom = (self.canvas_zoom * zoom).clamp(0.2, 8.0);
            }
            let scroll = ctx.input(|i| i.smooth_scroll_delta);
            if ctx.input(|i| i.modifiers.command) && scroll.y != 0.0 {
                self.canvas_zoom = (self.canvas_zoom * (scroll.y * 0.002).exp()).clamp(0.2, 8.0);
            }
            if ctx.input(|i| i.pointer.middle_down()) {
                self.canvas_pan += ctx.input(|i| i.pointer.delta());
            }
        }
        let paper = Rect::from_center_size(workspace.center() + self.canvas_pan, size);
        let visible = paper.intersect(workspace);
        let painter = ui.painter().with_clip_rect(workspace);
        painter.rect_filled(paper.expand(2.0), 1.0, Color32::from_black_alpha(100));
        self.pointer_input(ctx, paper, visible);
        painter.add(render::callback(
            paper,
            self.renderer.clone(),
            self.view(true),
        ));
        let scale = paper.width() / 1920.0;
        if let Some(pos) = ctx
            .input(|i| i.pointer.hover_pos())
            .filter(|p| visible.contains(*p))
        {
            ctx.set_cursor_icon(egui::CursorIcon::None);
            self.cursor(
                &painter,
                pos,
                self.doc.layers[self.layer].width * scale * 0.5,
            );
        } else if self.resized.elapsed() < Duration::from_millis(1200) {
            self.cursor(
                &painter,
                workspace.center(),
                self.doc.layers[self.layer].width * scale * 0.5,
            );
        }
        let layer = &self.doc.layers[self.layer];
        if layer.blend.effect() {
            if let Some(cel) = layer.held(tick(self.time)) {
                if let Some(stroke) = cel.strokes.last() {
                    if let (Some(a), Some(b)) = (stroke.points.first(), stroke.points.last()) {
                        let project = |p: &Point| {
                            Pos2::new(
                                paper.left() + if self.mirror { 1920.0 - p.x } else { p.x } * scale,
                                paper.top() + p.y * scale,
                            )
                        };
                        let start = project(a);
                        let end = project(b);
                        painter.arrow(start, end - start, UiStroke::new(1.2_f32, ACCENT));
                        painter.circle_stroke(start, 3.0, UiStroke::new(1.0_f32, ACCENT));
                    }
                }
            }
        }
        painter.text(
            Pos2::new(workspace.left() + 20.0, workspace.bottom() - 14.0),
            egui::Align2::LEFT_BOTTOM,
            format!(
                "{}  /  {:.2}:1",
                if self.tool == Tool::Ink {
                    "VECTOR INK"
                } else {
                    "VECTOR LIQUIFY"
                },
                self.doc.settings.aspect
            ),
            FontId::proportional(10.0),
            MUTED,
        );
        ui.scope_builder(
            egui::UiBuilder::new().max_rect(Rect::from_min_max(
                Pos2::new(available.left() + 20.0, workspace.bottom() + 8.0),
                available.max - Vec2::new(20.0, 0.0),
            )),
            |ui| {
                self.mini_timeline(ui);
            },
        );
    }
    fn cursor(&self, p: &egui::Painter, pos: Pos2, r: f32) {
        p.circle_stroke(
            pos,
            r.max(1.5),
            UiStroke::new(3.0_f32, Color32::from_white_alpha(150)),
        );
        p.circle_stroke(
            pos,
            r.max(1.5),
            UiStroke::new(
                1.0_f32,
                if self.tool == Tool::Liquify {
                    Color32::from_rgb(154, 112, 70)
                } else {
                    Color32::from_gray(70)
                },
            ),
        );
        if self.tool == Tool::Liquify {
            p.circle_filled(pos, 1.5, ACCENT);
        }
    }
    fn pointer_input(&mut self, ctx: &egui::Context, paper: Rect, visible: Rect) {
        if self.transport.playing || self.background.is_some() || self.pending.is_some() {
            return;
        }
        let events = ctx.input(|i| i.events.clone());
        let mut force = self.tablet.pressure().unwrap_or(1.0);
        for event in events {
            if let egui::Event::Touch {
                force: Some(value), ..
            } = event
            {
                force = value;
            }
            match event {
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    ..
                } if visible.contains(pos) => {
                    let point = self.canvas_point(pos, paper, force);
                    self.start_stroke(point);
                }
                egui::Event::PointerMoved(pos) if self.drawing.is_some() => {
                    self.move_stroke(self.canvas_point(pos, paper, force));
                }
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    ..
                } if self.drawing.is_some() => {
                    self.move_stroke(self.canvas_point(pos, paper, force));
                    self.finish_stroke();
                }
                egui::Event::WindowFocused(false) | egui::Event::PointerGone => {
                    self.finish_stroke()
                }
                _ => {}
            }
        }
        if self.drawing.is_some() {
            if let Some(pressure) = self.tablet.pressure() {
                if let Some(pos) = ctx.input(|i| i.pointer.latest_pos()) {
                    self.move_stroke(self.canvas_point(pos, paper, pressure));
                }
            }
        }
    }
    fn canvas_point(&self, pos: Pos2, paper: Rect, pressure: f32) -> Point {
        let mut x = (pos.x - paper.left()) / paper.width() * 1920.0;
        if self.mirror {
            x = 1920.0 - x;
        }
        Point {
            x,
            y: (pos.y - paper.top()) / paper.width() * 1920.0,
            pressure: pressure
                .clamp(0.0, 1.0)
                .powf(self.doc.settings.pressure_gamma)
                .max(self.doc.settings.minimum_pressure),
        }
    }
    fn start_stroke(&mut self, point: Point) {
        self.record();
        let liquify = self.tool == Tool::Liquify;
        let time = tick(self.time);
        let layer = self.layer;
        let width = self.doc.layers[layer].width;
        let movement = self.doc.layers[layer].blend == Blend::Move;
        let cel = self.doc.layers[layer].begin_edit(time, liquify);
        let index = if liquify {
            Some(SpatialIndex::new(cel, width * 0.5))
        } else {
            if movement {
                cel.strokes.clear();
            }
            cel.strokes.push(Arc::new(Stroke::new(width, point)));
            None
        };
        self.drawing = Some(Drawing {
            layer,
            time,
            last: point,
            previous: point,
            liquify,
            index,
        });
    }
    fn move_stroke(&mut self, point: Point) {
        let Some(drawing) = &mut self.drawing else {
            return;
        };
        let delta = [point.x - drawing.last.x, point.y - drawing.last.y];
        let distance = drawing.last.distance(point);
        if distance < 0.001 {
            if !drawing.liquify {
                let cel = Arc::make_mut(
                    self.doc.layers[drawing.layer]
                        .cels
                        .get_mut(&drawing.time)
                        .unwrap(),
                );
                if let Some(stroke) = cel.strokes.last_mut() {
                    let stroke = Arc::make_mut(stroke);
                    if stroke.dab(point.pressure) {
                        drawing.last.pressure = point.pressure;
                    }
                }
            }
            return;
        }
        let layer = &mut self.doc.layers[drawing.layer];
        let width = layer.width;
        let cel = Arc::make_mut(layer.cels.get_mut(&drawing.time).unwrap());
        if drawing.liquify {
            // Sweep the full pointer segment, keeping fast gestures continuous.
            let radius = width * 0.5;
            let steps = (distance / (radius * 0.2).max(1.0)).ceil() as usize;
            for i in 0..steps {
                let t = (i + 1) as f32 / steps as f32;
                let center = Point {
                    x: drawing.last.x + delta[0] * t,
                    y: drawing.last.y + delta[1] * t,
                    pressure: point.pressure,
                };
                drawing.index.as_mut().unwrap().deform(
                    cel,
                    center,
                    [delta[0] / steps as f32, delta[1] / steps as f32],
                    radius,
                    self.doc.settings.liquify_strength * point.pressure,
                );
            }
        } else {
            let stroke = Arc::make_mut(cel.strokes.last_mut().unwrap());
            if layer.blend == Blend::Move {
                // A movement layer stores only endpoints; cost is independent of gesture length.
                if stroke.points.len() == 1 {
                    stroke.points.push(point);
                } else {
                    stroke.points[1] = point;
                    stroke.revision = next_revision();
                }
            } else {
                let spacing = self.doc.settings.spacing;
                let steps = (distance / spacing).ceil().max(1.0) as usize;
                let a = drawing.last;
                let before = drawing.previous;
                for i in 1..=steps {
                    let t = i as f32 / steps as f32;
                    let (x, y) = if self.doc.settings.interpolation == Interpolation::Smooth {
                        // Local Hermite interpolation, preserving both measured endpoints.
                        let mut tangent = [(point.x - before.x) * 0.5, (point.y - before.y) * 0.5];
                        let length = tangent[0].hypot(tangent[1]);
                        if length > distance {
                            tangent[0] *= distance / length;
                            tangent[1] *= distance / length;
                        }
                        let h00 = 2.0 * t * t * t - 3.0 * t * t + 1.0;
                        let h10 = t * t * t - 2.0 * t * t + t;
                        let h01 = -2.0 * t * t * t + 3.0 * t * t;
                        let h11 = t * t * t - t * t;
                        (
                            h00 * a.x + h10 * tangent[0] + h01 * point.x + h11 * delta[0],
                            h00 * a.y + h10 * tangent[1] + h01 * point.y + h11 * delta[1],
                        )
                    } else {
                        (a.x + delta[0] * t, a.y + delta[1] * t)
                    };
                    stroke.points.push(Point {
                        x,
                        y,
                        pressure: a.pressure + (point.pressure - a.pressure) * t,
                    });
                }
            }
        }
        drawing.previous = drawing.last;
        drawing.last = point;
    }
    fn mini_timeline(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            if ui.small_button("‹").clicked() {
                self.set_time(self.doc.timing.previous(self.time));
            }
            text(ui, format!("{:07.3}", self.time), 13.0, ACCENT);
            if ui.small_button("›").clicked() {
                self.set_time(self.doc.timing.next(self.time));
            }
            let width = ui.available_width() - 8.0;
            let (rect, response) =
                ui.allocate_exact_size(Vec2::new(width, 30.0), Sense::click_and_drag());
            let end = self
                .doc
                .timing
                .duration
                .max(self.time + self.doc.timing.step_at(self.doc.timing.segment(self.time)));
            let p = ui.painter();
            p.hline(
                rect.x_range(),
                rect.center().y,
                UiStroke::new(1.0_f32, LINE),
            );
            for t in self.doc.timing.display_grid(0.0, end, rect.width()) {
                let x = rect.left() + t as f32 / end as f32 * rect.width();
                p.line_segment(
                    [
                        Pos2::new(x, rect.center().y - 3.0),
                        Pos2::new(x, rect.center().y + 3.0),
                    ],
                    UiStroke::new(1.0_f32, LINE),
                );
            }
            for time in self.doc.layers[self.layer].cels.keys() {
                let x = rect.left() + seconds(*time) as f32 / end as f32 * rect.width();
                p.circle_filled(Pos2::new(x, rect.center().y), 2.8, MUTED);
            }
            let x = rect.left() + self.time as f32 / end as f32 * rect.width();
            p.circle_filled(Pos2::new(x, rect.center().y), 4.5, ACCENT);
            if response.clicked() || response.dragged() {
                if let Some(pos) = response.interact_pointer_pos() {
                    self.set_time(((pos.x - rect.left()) / rect.width()) as f64 * end);
                }
            }
        });
    }
    fn timeline_page(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let rect = ui.available_rect_before_wrap();
        let left = Rect::from_min_max(
            rect.min + Vec2::new(20.0, 0.0),
            Pos2::new(rect.left() + 285.0, rect.bottom()),
        );
        let right = Rect::from_min_max(
            Pos2::new(left.right() + 28.0, rect.top()),
            rect.max - Vec2::new(24.0, 0.0),
        );
        ui.scope_builder(egui::UiBuilder::new().max_rect(left),|ui|{
            egui::ScrollArea::vertical().id_salt("layer_panel").auto_shrink([false,false]).max_height(left.height()).show(ui,|ui|{
                section(ui,"COMPOSITION","Layers","Build motion from the bottom up.");
                let mut select=None;
                let mut toggle=None;
                for index in (0..self.doc.layers.len()).rev(){
                    let layer=&self.doc.layers[index];
                    egui::Frame::new().fill(if index==self.layer{
                        Color32::from_rgb(47,44,40)}
                    else{
                        PANEL}
                    ).corner_radius(7.0).inner_margin(12.0).show(ui,|ui|{
                        ui.set_min_width(230.0);
                        ui.horizontal(|ui|{
                            if ui.selectable_label(layer.visible,if layer.visible{
                                "On"}
                            else{
                                "Off"}
                            ).on_hover_text("Toggle visibility").clicked(){
                                toggle=Some(index);
                            }
                            let label=format!("{:02}   {}",index+1,layer.name);
                            if ui.selectable_label(index==self.layer,RichText::new(label).color(if index==self.layer{
                                ACCENT}
                            else{
                                INK}
                            )).clicked(){
                                select=Some(index);
                            }
                        }
                        );
                        ui.horizontal(|ui|{
                            ui.add_space(32.0);
                            text(ui,format!("{}  ·  {} exposures",layer.blend.name(),layer.cels.len()),10.0,MUTED);
                        }
                        );
                    }
                    );
                    ui.add_space(5.0);
                }
                if let Some(index)=select{
                    self.layer=index;
                }
                if let Some(index)=toggle{
                    self.record();
                    self.doc.layers[index].visible= !self.doc.layers[index].visible;
                }
                ui.horizontal(|ui|{
                    if ui.button("+ Layer").clicked(){
                        self.record();
                        self.doc.layers.push(Layer::new(&format!("Layer {}",self.doc.layers.len()+1),Blend::Xor,42.0));
                        self.layer=self.doc.layers.len()-1;
                    }
                    if ui.add_enabled(self.layer>1,egui::Button::new("Down")).on_hover_text("Move down").clicked(){
                        self.record();
                        self.doc.layers.swap(self.layer,self.layer-1);
                        self.layer-=1;
                    }
                    if ui.add_enabled(self.layer>0&&self.layer+1<self.doc.layers.len(),egui::Button::new("Up")).on_hover_text("Move up").clicked(){
                        self.record();
                        self.doc.layers.swap(self.layer,self.layer+1);
                        self.layer+=1;
                    }
                }
                );
                ui.add_space(18.0);
                ui.separator();
                ui.add_space(10.0);
                text(ui,"LAYER PROPERTIES",10.0,ACCENT);
                let before=self.doc.clone();
                let mut changed=false;
                let layer=&mut self.doc.layers[self.layer];
                changed|=ui.text_edit_singleline(&mut layer.name).changed();
                if self.layer>0 {
                    let previous=layer.blend;
                    egui::ComboBox::from_id_salt("blend").selected_text(layer.blend.name()).width(200.0).show_ui(ui,|ui|{
                        for blend in Blend::OVERLAYS{
                            changed|=ui.selectable_value(&mut layer.blend,blend,blend.name()).changed();
                        }
                    }
                    );
                    if previous!=Blend::Move&&layer.blend==Blend::Move{
                        for cel in layer.cels.values_mut(){
                            let cel=Arc::make_mut(cel);
                            if cel.strokes.len()>1{
                                let last=cel.strokes.pop().unwrap();
                                cel.strokes.clear();
                                cel.strokes.push(last);
                            }
                        }
                    }
                }
                changed|=ui.checkbox(&mut layer.inverted,"Invert black / white").changed();
                changed|=ui.add(egui::Slider::new(&mut layer.width,1.0..=500.0).text("Brush").suffix(" px").logarithmic(true)).changed();
                if layer.blend.effect(){
                    ui.add_space(8.0);
                    text(ui,"EFFECT CHARACTER",10.0,MUTED);
                    egui::ComboBox::from_id_salt("shader").selected_text(layer.style.name()).width(210.0).show_ui(ui,|ui|{
                        for style in Style::ALL{
                            changed|=ui.selectable_value(&mut layer.style,style,style.name()).changed();
                        }
                    }
                    );
                    changed|=ui.add(egui::Slider::new(&mut layer.strength,0.0..=3.0).text("Amount")).changed();
                    text(ui,if layer.blend==Blend::Move{
                        "First → last point moves all layers below.\nA new gesture replaces the previous one."}
                    else{
                        "Each gesture adds a directional shutter.\nIts length controls the motion distance."}
                    ,11.0,MUTED);
                }
                self.ui_change(&before,changed,ctx);
                ui.add_space(12.0);
                ui.horizontal(|ui|{
                    if ui.add_enabled(self.doc.layers[self.layer].cels.contains_key(&tick(self.time)),egui::Button::new("Remove exposure")).on_hover_text("Resume holding the previous exposure").clicked(){
                        self.record();
                        self.doc.layers[self.layer].cels.remove(&tick(self.time));
                    }
                    if ui.add_enabled(self.layer>0,egui::Button::new("Delete layer")).clicked(){
                        self.record();
                        self.doc.layers.remove(self.layer);
                        self.layer=self.layer.saturating_sub(1);
                    }
                }
                );
                if ui.button("Blank exposure here").on_hover_text("Create an empty exposure instead of holding previous content").clicked(){
                    self.record();
                    self.doc.layers[self.layer].cels.insert(tick(self.time),Arc::default());
                }
            }
            );
        }
        );
        ui.scope_builder(egui::UiBuilder::new().max_rect(right), |ui| {
            section(
                ui,
                "NATIVE TIME",
                "Find your rhythm",
                "Music leads. Every exposure lands on a musical grid.",
            );
            let top = ui.available_rect_before_wrap();
            let preview_width = (top.width() * 0.43).min(420.0);
            let preview_size = Vec2::new(preview_width, preview_width / self.doc.settings.aspect);
            let preview = Rect::from_min_size(
                Pos2::new(top.right() - preview_width, top.top()),
                preview_size,
            );
            ui.painter().add(render::callback(
                preview,
                self.renderer.clone(),
                self.view(false),
            ));
            let controls = Rect::from_min_max(
                top.min,
                Pos2::new(preview.left() - 20.0, top.top() + preview_size.y.max(130.0)),
            );
            ui.scope_builder(egui::UiBuilder::new().max_rect(controls), |ui| {
                text(ui, "SOUNDTRACK", 10.0, MUTED);
                text(
                    ui,
                    self.doc
                        .audio
                        .as_ref()
                        .map_or("No music imported".into(), |a| a.name.clone()),
                    14.0,
                    INK,
                );
                if let Some(audio) = &self.transport.audio {
                    text(
                        ui,
                        format!(
                            "{:.1} s  ·  {} Hz  ·  {} ch",
                            audio.duration, audio.rate, audio.channels
                        ),
                        11.0,
                        MUTED,
                    );
                }
                if ui
                    .button(if self.doc.audio.is_some() {
                        "Replace music…"
                    } else {
                        "+ Import music…"
                    })
                    .clicked()
                {
                    self.import_audio();
                }
                ui.add_space(8.0);
                let before = self.doc.clone();
                let mut changed = false;
                ui.horizontal(|ui| {
                    text(ui, "Grid", 12.0, MUTED);
                    egui::ComboBox::from_id_salt("grid")
                        .selected_text(step_label(self.doc.timing.step))
                        .show_ui(ui, |ui| {
                            for step in [
                                0.03125,
                                0.0625,
                                1.0 / 12.0,
                                0.125,
                                1.0 / 6.0,
                                0.25,
                                1.0 / 3.0,
                                0.5,
                                1.0,
                                2.0,
                            ] {
                                changed |= ui
                                    .selectable_value(
                                        &mut self.doc.timing.step,
                                        step,
                                        step_label(step),
                                    )
                                    .changed();
                            }
                        });
                });
                ui.horizontal(|ui| {
                    text(ui, "Duration", 11.0, MUTED);
                    changed |= ui
                        .add(
                            egui::DragValue::new(&mut self.doc.timing.duration)
                                .range(0.1..=86400.0)
                                .speed(0.1)
                                .suffix(" s"),
                        )
                        .changed();
                });
                if changed {
                    self.time = self.doc.timing.snap(self.time);
                }
                self.ui_change(&before, changed, ctx);
            });
            ui.add_space(
                (top.top() + preview_size.y.max(175.0) - ui.cursor().min.y).max(0.0) + 16.0,
            );
            ui.separator();
            ui.add_space(14.0);
            ui.horizontal(|ui| {
                text(ui, "TEMPO MAP", 10.0, ACCENT);
                if ui.small_button("+ Start point at playhead").clicked() {
                    if self
                        .doc
                        .timing
                        .tempos
                        .iter()
                        .any(|t| (t.start - self.time).abs() < 0.00001)
                    {
                        self.notify("A tempo start already exists here");
                    } else {
                        self.record();
                        let bpm = self.doc.timing.tempos[self.doc.timing.segment(self.time)].bpm;
                        self.doc.timing.tempos.push(Tempo {
                            start: self.time,
                            bpm,
                        });
                        self.doc.timing.normalize();
                    }
                }
            });
            let before = self.doc.clone();
            let mut changed = false;
            let mut remove = None;
            egui::ScrollArea::vertical()
                .id_salt("tempos")
                .max_height(100.0)
                .show(ui, |ui| {
                    for (index, tempo) in self.doc.timing.tempos.iter_mut().enumerate() {
                        ui.horizontal(|ui| {
                            text(ui, format!("{:02}", index + 1), 11.0, MUTED);
                            if index == 0 {
                                text(ui, "0.000 s", 12.0, INK);
                            } else {
                                changed |= ui
                                    .add(
                                        egui::DragValue::new(&mut tempo.start)
                                            .range(0.000001..=86400.0)
                                            .speed(0.001)
                                            .fixed_decimals(3)
                                            .suffix(" s"),
                                    )
                                    .changed();
                            }
                            changed |= ui
                                .add(
                                    egui::DragValue::new(&mut tempo.bpm)
                                        .range(1.0..=999.0)
                                        .speed(0.1)
                                        .suffix(" BPM"),
                                )
                                .changed();
                            if index > 0 && ui.small_button("×").clicked() {
                                remove = Some(index);
                            }
                        });
                    }
                });
            if let Some(index) = remove {
                self.doc.timing.tempos.remove(index);
                changed = true;
            }
            if changed {
                self.doc.timing.normalize();
                self.time = self.doc.timing.snap(self.time);
                self.transport.stop();
            }
            self.ui_change(&before, changed, ctx);
            text(
                ui,
                "Tempo edits preserve exposure times in seconds.",
                10.0,
                MUTED,
            );
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                if ui
                    .button(if self.transport.playing { "Ⅱ" } else { "▶" })
                    .clicked()
                {
                    self.toggle_play();
                }
                text(ui, format!("{:.3} s", self.time), 18.0, ACCENT);
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.add(
                        egui::Slider::new(&mut self.timeline_zoom, 1.0..=32.0)
                            .logarithmic(true)
                            .text("Zoom"),
                    );
                });
            });
            self.timeline_tracks(ui);
        });
    }
    fn timeline_tracks(&mut self, ui: &mut egui::Ui) {
        let height = ui.available_height().max(160.0);
        egui::ScrollArea::vertical()
            .id_salt("tracks_scroll")
            .auto_shrink([false, false])
            .max_height(height - 25.0)
            .show(ui, |ui| {
                let width = ui.available_width();
                let (rect, response) = ui.allocate_exact_size(
                    Vec2::new(
                        width,
                        (height - 25.0).max(110.0 + self.doc.layers.len() as f32 * 32.0),
                    ),
                    Sense::click_and_drag(),
                );
                let label = 98.0;
                let tracks = Rect::from_min_max(rect.min + Vec2::new(label, 0.0), rect.max);
                let duration = self.doc.timing.duration;
                let span = duration / self.timeline_zoom as f64;
                self.timeline_start = self.timeline_start.clamp(0.0, (duration - span).max(0.0));
                let start = self.timeline_start;
                let end = start + span;
                let p = ui.painter().with_clip_rect(rect);
                p.rect_filled(rect, 6.0, PANEL);
                let x = |time: f64| tracks.left() + ((time - start) / span) as f32 * tracks.width();
                let ruler = Rect::from_min_size(tracks.min, Vec2::new(tracks.width(), 27.0));
                for t in self.doc.timing.display_grid(start, end, tracks.width()) {
                    let px = x(t);
                    let segment = self.doc.timing.segment(t);
                    let beat = (t - self.doc.timing.tempos[segment].start)
                        * self.doc.timing.tempos[segment].bpm
                        / 60.0;
                    let major = (beat - beat.round()).abs() < 0.0001;
                    if major
                        || tracks.width() as f64 / span * self.doc.timing.step_at(segment) > 5.0
                    {
                        p.vline(
                            px,
                            rect.y_range(),
                            UiStroke::new(
                                1.0_f32,
                                if major {
                                    LINE
                                } else {
                                    Color32::from_rgb(36, 39, 41)
                                },
                            ),
                        );
                        if major {
                            p.text(
                                Pos2::new(px + 4.0, ruler.top() + 7.0),
                                egui::Align2::LEFT_TOP,
                                format!("{:.2}", t),
                                FontId::proportional(10.0),
                                MUTED,
                            );
                        }
                    }
                }
                let wave = Rect::from_min_max(
                    Pos2::new(tracks.left(), ruler.bottom() + 8.0),
                    Pos2::new(tracks.right(), ruler.bottom() + 64.0),
                );
                p.text(
                    Pos2::new(rect.left() + 12.0, wave.center().y),
                    egui::Align2::LEFT_CENTER,
                    "AUDIO",
                    FontId::proportional(10.0),
                    MUTED,
                );
                if let Some(audio) = &self.transport.audio {
                    let columns = wave.width().ceil() as usize;
                    for i in 0..columns {
                        let a =
                            ((start + i as f64 / columns as f64 * span) * 200.0).floor() as usize;
                        let b = ((start + (i + 1) as f64 / columns as f64 * span) * 200.0).ceil()
                            as usize;
                        let peak = audio
                            .peaks
                            .get(a.min(audio.peaks.len())..b.min(audio.peaks.len()))
                            .unwrap_or(&[])
                            .iter()
                            .fold(0.0_f32, |a, v| a.max(*v));
                        let h = peak * wave.height() * 0.44;
                        p.line_segment(
                            [
                                Pos2::new(wave.left() + i as f32, wave.center().y - h),
                                Pos2::new(wave.left() + i as f32, wave.center().y + h),
                            ],
                            UiStroke::new(1.0_f32, ACCENT.gamma_multiply(0.8)),
                        );
                    }
                } else {
                    p.text(
                        wave.center(),
                        egui::Align2::CENTER_CENTER,
                        "Import music to see its waveform",
                        FontId::proportional(11.0),
                        MUTED,
                    );
                }
                let row_height = ((rect.bottom() - wave.bottom() - 12.0)
                    / self.doc.layers.len() as f32)
                    .clamp(24.0, 44.0);
                for (row, index) in (0..self.doc.layers.len()).rev().enumerate() {
                    let y = wave.bottom() + 12.0 + row as f32 * row_height;
                    let layer = &self.doc.layers[index];
                    if index == self.layer {
                        p.rect_filled(
                            Rect::from_min_size(
                                Pos2::new(rect.left(), y),
                                Vec2::new(rect.width(), row_height),
                            ),
                            0.0,
                            ACCENT.gamma_multiply(0.075),
                        );
                    }
                    p.text(
                        Pos2::new(rect.left() + 12.0, y + row_height * 0.5),
                        egui::Align2::LEFT_CENTER,
                        &layer.name,
                        FontId::proportional(11.0),
                        if index == self.layer { ACCENT } else { MUTED },
                    );
                    let keys: Vec<Tick> = layer.cels.keys().copied().collect();
                    for (i, time) in keys.iter().enumerate() {
                        let t = seconds(*time);
                        let next = keys.get(i + 1).map_or(duration, |t| seconds(*t));
                        if next < start || t > end {
                            continue;
                        }
                        let exposure = Rect::from_min_max(
                            Pos2::new(x(t).max(tracks.left()), y + 6.0),
                            Pos2::new(x(next).min(tracks.right()), y + row_height - 6.0),
                        );
                        p.rect_filled(
                            exposure,
                            3.0,
                            if index == self.layer {
                                ACCENT.gamma_multiply(0.25)
                            } else {
                                Color32::from_gray(60)
                            },
                        );
                        p.circle_filled(
                            Pos2::new(x(t), y + row_height * 0.5),
                            3.0,
                            if index == self.layer { ACCENT } else { MUTED },
                        );
                    }
                    p.hline(
                        rect.x_range(),
                        y + row_height,
                        UiStroke::new(1.0_f32, LINE.gamma_multiply(0.5)),
                    );
                }
                for tempo in &self.doc.timing.tempos {
                    if tempo.start >= start && tempo.start <= end {
                        p.line_segment(
                            [
                                Pos2::new(x(tempo.start), ruler.bottom()),
                                Pos2::new(x(tempo.start), wave.bottom()),
                            ],
                            UiStroke::new(1.0_f32, Color32::from_rgb(100, 145, 155)),
                        );
                    }
                }
                let playhead = x(if self.transport.playing {
                    self.transport.position()
                } else {
                    self.time
                });
                p.vline(playhead, rect.y_range(), UiStroke::new(1.5_f32, ACCENT));
                p.circle_filled(Pos2::new(playhead, ruler.top() + 3.0), 4.0, ACCENT);
                if response.clicked() || response.dragged() {
                    if let Some(pos) = response.interact_pointer_pos() {
                        if pos.x >= tracks.left() {
                            self.set_time(
                                start
                                    + ((pos.x - tracks.left()) / tracks.width()).clamp(0.0, 1.0)
                                        as f64
                                        * span,
                            );
                        }
                        if pos.y > wave.bottom() + 12.0 {
                            let row = ((pos.y - wave.bottom() - 12.0) / row_height) as usize;
                            if row < self.doc.layers.len() {
                                self.layer = self.doc.layers.len() - 1 - row;
                            }
                        }
                    }
                }
                if self.timeline_zoom > 1.0 {
                    ui.add(
                        egui::Slider::new(
                            &mut self.timeline_start,
                            0.0..=(duration - span).max(0.0),
                        )
                        .show_value(false),
                    );
                }
            });
    }
    fn ui_change(&mut self, before: &Document, changed: bool, ctx: &egui::Context) {
        if changed {
            if !self.ui_editing {
                self.history.record(before);
            }
            self.dirty = true;
            self.ui_editing = ctx.input(|i| i.pointer.any_down()) || ctx.wants_keyboard_input();
        }
    }
    fn settings_page(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        egui::ScrollArea::vertical().id_salt("settings_scroll").show(ui,|ui|{
            ui.add_space(10.0);
            ui.horizontal(|ui|{
                ui.add_space(24.0);
                ui.vertical(|ui|{
                    section(ui,"YOUR STUDIO","Made for your hand","A few thoughtful defaults. Every detail is yours.");
                }
                );
            }
            );
            let before=self.doc.clone();
            let mut changed=false;
            ui.columns(2,|cols|{
                let ui=&mut cols[0];
                egui::Frame::new().fill(PANEL).corner_radius(10.0).inner_margin(24.0).show(ui,|ui|{
                    text(ui,"01  /  CANVAS & ONION SKIN",11.0,ACCENT);
                    ui.add_space(14.0);
                    ui.horizontal(|ui|{
                        text(ui,"Canvas ratio",12.0,INK);
                        changed|=ui.add(egui::DragValue::new(&mut self.doc.settings.aspect).range(0.2..=8.0).speed(0.01).fixed_decimals(3)).changed();
                        for (name,ratio) in [("Cinema",2.39),("16:9",16.0/9.0),("1:1",1.0)]{
                            if ui.small_button(name).clicked(){
                                self.doc.settings.aspect=ratio;
                                changed=true;
                            }
                        }
                    }
                    );
                    text(ui,"Existing vector coordinates stay in place when the ratio changes.",11.0,MUTED);
                    let onion=&mut self.doc.settings.onion;
                    changed|=ui.checkbox(&mut onion.enabled,"Show onion skin while drawing").changed();
                    changed|=ui.add(egui::Slider::new(&mut onion.opacity,0.0..=1.0).text("Opacity")).changed();
                    changed|=ui.add(egui::Slider::new(&mut onion.falloff,0.0..=1.0).text("Distance falloff")).changed();
                    ui.horizontal(|ui|{
                        text(ui,"Previous",12.0,INK);
                        changed|=ui.add(egui::DragValue::new(&mut onion.previous).range(0..=12)).changed();
                        changed|=ui.color_edit_button_srgb(&mut onion.before).changed();
                    }
                    );
                    ui.horizontal(|ui|{
                        text(ui,"Following",12.0,INK);
                        changed|=ui.add(egui::DragValue::new(&mut onion.next).range(0..=12)).changed();
                        changed|=ui.color_edit_button_srgb(&mut onion.after).changed();
                    }
                    );
                    text(ui,"Onion skins follow distinct exposures and stay out of video exports.",11.0,MUTED);
                }
                );
                ui.add_space(16.0);
                egui::Frame::new().fill(PANEL).corner_radius(10.0).inner_margin(24.0).show(ui,|ui|{
                    text(ui,"02  /  BRUSH FEEL",11.0,ACCENT);
                    ui.add_space(14.0);
                    changed|=ui.add(egui::Slider::new(&mut self.doc.settings.pressure_gamma,0.2..=3.0).text("Pressure curve")).changed();
                    changed|=ui.add(egui::Slider::new(&mut self.doc.settings.minimum_pressure,0.0..=0.5).text("Minimum size")).changed();
                    let (rect,_)=ui.allocate_exact_size(Vec2::new(ui.available_width(),95.0),Sense::hover());
                    let p=ui.painter();
                    p.rect_filled(rect,4.0,BG);
                    let mut last=rect.left_bottom();
                    for i in 0..=100{
                        let t=i as f32/100.0;
                        let pressure=t.powf(self.doc.settings.pressure_gamma).max(self.doc.settings.minimum_pressure);
                        let next=Pos2::new(rect.left()+t*rect.width(),rect.bottom()-pressure*rect.height());
                        p.line_segment([last,next],UiStroke::new(1.8_f32,ACCENT));
                        last=next;
                    }
                    ui.add_space(8.0);
                    changed|=ui.add(egui::Slider::new(&mut self.doc.settings.spacing,0.25..=8.0).text("Sample spacing").suffix(" px")).changed();
                    ui.horizontal(|ui|{
                        text(ui,"Interpolation",12.0,INK);
                        egui::ComboBox::from_id_salt("interpolation").selected_text(format!("{:?}",self.doc.settings.interpolation)).show_ui(ui,|ui|{
                            changed|=ui.selectable_value(&mut self.doc.settings.interpolation,Interpolation::Smooth,"Smooth · Hermite").changed();
                            changed|=ui.selectable_value(&mut self.doc.settings.interpolation,Interpolation::Linear,"Linear").changed();
                        }
                        );
                    }
                    );
                    changed|=ui.add(egui::Slider::new(&mut self.doc.settings.liquify_strength,0.05..=1.0).text("Liquify strength")).changed();
                    text(ui,"Pen pressure controls diameter. Mouse input uses full pressure.\nLiquify changes geometry while preserving every point’s pressure.",11.0,MUTED);
                }
                );
                let ui=&mut cols[1];
                egui::Frame::new().fill(PANEL).corner_radius(10.0).inner_margin(24.0).show(ui,|ui|{
                    text(ui,"03  /  EXPORT MOTION",11.0,ACCENT);
                    ui.add_space(14.0);
                    let settings=&mut self.doc.settings.export;
                    ui.horizontal(|ui|{
                        text(ui,"Width",12.0,INK);
                        changed|=ui.add(egui::DragValue::new(&mut settings.width).range(64..=7680).speed(2).suffix(" px")).changed();
                        text(ui,format!("× {}",((settings.width as f32/self.doc.settings.aspect/2.0).round() as u32*2).max(2)),12.0,MUTED);
                    }
                    );
                    ui.horizontal(|ui|{
                        text(ui,"Frame rate",12.0,INK);
                        changed|=ui.add(egui::DragValue::new(&mut settings.fps).range(1..=240).suffix(" FPS")).changed();
                    }
                    );
                    ui.horizontal(|ui|{
                        text(ui,"Range",12.0,INK);
                        changed|=ui.add(egui::DragValue::new(&mut settings.start).range(0.0..=86400.0).speed(0.1).suffix(" s")).changed();
                        text(ui,"to",12.0,MUTED);
                        changed|=ui.add(egui::DragValue::new(&mut settings.end).range(0.001..=86400.0).speed(0.1).suffix(" s")).changed();
                    }
                    );
                    changed|=ui.add(egui::Slider::new(&mut settings.crf,0..=30).text("CRF · lower is finer")).changed();
                    changed|=ui.checkbox(&mut settings.audio,"Include soundtrack").changed();
                    text(ui,"MP4 / H.264  ·  AAC  ·  exact musical timestamps\nNearest exposure; no blended or invented animation frames.",11.0,MUTED);
                    ui.add_space(12.0);
                    if ui.add_enabled(self.export.is_none()&&settings.end>settings.start,egui::Button::new(RichText::new("Export video…").color(BG)).fill(ACCENT).min_size(Vec2::new(180.0,36.0))).clicked(){
                        self.start_export();
                    }
                    if let Some(job)=&self.export{
                        ui.add(egui::ProgressBar::new(job.frame as f32/job.frames as f32).text(format!("{} / {} frames",job.frame,job.frames)));
                    }
                    if self.export.is_some()&&ui.button("Cancel export").clicked(){
                        self.export=None;
                        self.notify("Export cancelled");
                    }
                    text(ui,"Requires FFmpeg. Editor overlays stay out of the export.",10.0,MUTED);
                }
                );
                ui.add_space(16.0);
                egui::Frame::new().fill(PANEL).corner_radius(10.0).inner_margin(24.0).show(ui,|ui|{
                    ui.horizontal(|ui|{
                        text(ui,"04  /  KEYBOARD",11.0,ACCENT);
                        ui.with_layout(Layout::right_to_left(Align::Center),|ui|{
                            if ui.small_button("Reset").clicked(){
                                self.doc.settings.bindings=default_bindings();
                                changed=true;
                                self.bind_capture=None;
                            }
                        }
                        );
                    }
                    );
                    ui.add_space(8.0);
                    text(ui,"Click a shortcut, then press its new combination. Esc cancels.",11.0,MUTED);
                    let mut capture=None;
                    egui::Grid::new("shortcuts").num_columns(2).min_col_width(140.0).spacing([16.0,6.0]).show(ui,|ui|{
                        for (i,binding) in self.doc.settings.bindings.iter().enumerate(){
                            text(ui,&binding.action,12.0,INK);
                            if ui.button(RichText::new(if self.bind_capture==Some(i){
                                "Press keys…".into()}
                            else{
                                binding.label()}
                            ).color(if self.bind_capture==Some(i){
                                ACCENT}
                            else{
                                MUTED}
                            )).clicked(){
                                capture=Some(i);
                            }
                            ui.end_row();
                        }
                    }
                    );
                    if let Some(index)=capture{
                        self.bind_capture=Some(index);
                    }
                }
                );
            }
            );
            self.ui_change(&before,changed,ctx);
            ui.add_space(30.0);
        }
        );
    }
    fn start_export(&mut self) {
        self.transport.stop();
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("MP4 video", &["mp4"])
            .set_file_name(format!("{}.mp4", self.doc.name))
            .save_file()
        {
            match ExportJob::start(&path, &self.doc) {
                Ok(job) => {
                    self.export = Some(job);
                    self.notify("Export started");
                }
                Err(e) => self.notify(e),
            }
        }
    }
    fn export_step(&mut self, gl: &eframe::glow::Context) {
        if self.export_renderer.is_none() {
            match unsafe { Renderer::new(gl) } {
                Ok(renderer) => self.export_renderer = Some(renderer),
                Err(error) => {
                    self.export = None;
                    self.notify(error);
                    return;
                }
            }
        }
        let mut done = None;
        if let Some(job) = &mut self.export {
            if let Ok(result) = job.result.try_recv() {
                done = Some(result);
            } else if job.frame < job.frames {
                if job.pending.is_none() {
                    let time = job.time();
                    let scene = job.document.scene(tick(time));
                    match unsafe {
                        self.export_renderer
                            .as_mut()
                            .unwrap()
                            .read_frame(gl, &scene, job.size)
                    } {
                        Ok(frame) => job.pending = Some(frame),
                        Err(e) => done = Some(Err(e)),
                    }
                }
                if let Some(frame) = job.pending.take() {
                    if let Some(sender) = &job.sender {
                        match sender.try_send(frame) {
                            Ok(()) => {
                                job.frame += 1;
                                if job.frame == job.frames {
                                    job.sender = None;
                                }
                            }
                            Err(TrySendError::Full(frame)) => job.pending = Some(frame),
                            Err(TrySendError::Disconnected(_)) => {}
                        }
                    }
                }
            }
        }
        if let Some(result) = done {
            self.export = None;
            if let Some(mut renderer) = self.export_renderer.take() {
                unsafe {
                    renderer.destroy(gl);
                }
            }
            match result {
                Ok(path) => self.notify(format!("Video saved · {}", path.display())),
                Err(e) => self.notify(e),
            }
        }
    }
    fn confirmation(&mut self, ctx: &egui::Context) {
        if let Some(pending) = self.pending {
            egui::Window::new("Save your motion?")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
                .show(ctx, |ui| {
                    text(
                        ui,
                        if self.export.is_some() {
                            "A video is exporting. Closing will cancel it."
                        } else {
                            "Your project has unsaved changes."
                        },
                        14.0,
                        INK,
                    );
                    ui.add_space(12.0);
                    ui.horizontal(|ui| {
                        if ui
                            .add_enabled(self.dirty, egui::Button::new("Save & continue"))
                            .clicked()
                        {
                            self.save(false);
                        }
                        if ui
                            .button(if self.dirty {
                                "Discard changes"
                            } else {
                                "Stop export & close"
                            })
                            .clicked()
                        {
                            self.pending = None;
                            self.perform(pending);
                        }
                        if ui.button("Cancel").clicked() {
                            self.pending = None;
                        }
                    });
                });
        }
    }
    fn make_demo(&mut self) {
        self.doc.name = "Studies in orbit".into();
        for frame in 0..12 {
            let time = tick(frame as f64 * 0.25);
            let mut cel = Cel::default();
            let phase = frame as f32 / 12.0 * std::f32::consts::TAU;
            for ring in 0..4 {
                let mut stroke = Stroke::new(
                    3.5 + ring as f32,
                    Point {
                        x: 0.0,
                        y: 0.0,
                        pressure: 1.0,
                    },
                );
                stroke.points.clear();
                for i in 0..=400 {
                    let t = i as f32 / 400.0 * std::f32::consts::TAU;
                    let r = 180.0 + ring as f32 * 21.0;
                    let wobble = (t * 3.0 + phase).sin() * 23.0;
                    stroke.points.push(Point {
                        x: 960.0 + (t + phase * 0.12).cos() * (r + wobble) * 1.35,
                        y: 400.0 + t.sin() * (r + wobble) * 0.9,
                        pressure: 0.4 + (t + phase).sin().abs() * 0.6,
                    });
                }
                cel.strokes.push(Arc::new(stroke));
            }
            for star in 0..7 {
                let t = star as f32 / 7.0 * std::f32::consts::TAU + phase * 0.2;
                let p = Point {
                    x: 960.0 + t.cos() * 440.0,
                    y: 400.0 + t.sin() * 255.0,
                    pressure: 1.0,
                };
                cel.strokes.push(Arc::new(Stroke::new(5.0, p)));
            }
            self.doc.layers[0].cels.insert(time, Arc::new(cel));
        }
        self.doc.timing.duration = 3.0;
        self.doc.settings.export.end = 3.0;
    }
    fn smoke_test(&mut self, ctx: &egui::Context, gl: &eframe::glow::Context) {
        if let Some(directory) = &self.smoke_finish {
            if self.export.is_none() {
                let path = directory.join("export.mp4");
                assert!(
                    path.metadata().unwrap().len() > 1000,
                    "Video export did not complete"
                );
                std::fs::write(
                    directory.join("export-ok.txt"),
                    "GPU video with embedded test audio encoded successfully",
                )
                .unwrap();
                self.allow_close = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            return;
        }
        if self.smoke_started.elapsed() < Duration::from_millis(700) {
            ctx.request_repaint_after(Duration::from_millis(100));
            return;
        }
        if let Some(directory) = self.smoke.take() {
            std::fs::create_dir_all(&directory).unwrap();
            let mut renderer = self.renderer.lock().unwrap();
            let mut outputs = Vec::new();
            for (name, blend, style) in [
                ("ink", Blend::Base, Style::Clean),
                ("move", Blend::Move, Style::Clean),
                ("blur", Blend::Blur, Style::Clean),
                ("prism", Blend::Move, Style::Prism),
                ("ribbon", Blend::Blur, Style::Ribbon),
            ] {
                let mut document = self.doc.clone();
                if blend.effect() {
                    let mut layer = Layer::new("Test", blend, 100.0);
                    layer.style = style;
                    let mut stroke = Stroke::new(
                        100.0,
                        Point {
                            x: 800.0,
                            y: 400.0,
                            pressure: 1.0,
                        },
                    );
                    stroke.points.push(Point {
                        x: 1040.0,
                        y: 450.0,
                        pressure: 1.0,
                    });
                    layer.cels.insert(
                        0,
                        Arc::new(Cel {
                            strokes: vec![Arc::new(stroke)],
                        }),
                    );
                    document.layers.push(layer);
                }
                let bytes =
                    unsafe { renderer.read_frame(gl, &document.scene(0), [960, 402]) }.unwrap();
                let black = bytes
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .filter(|p| p[0] < 240 || p[1] < 240 || p[2] < 240)
                    .count();
                assert!(black > 200, "{name} is blank");
                let mut ppm = b"P6\n960 402\n255\n".to_vec();
                for row in bytes.as_chunks::<{ 960 * 4 }>().0.iter().rev() {
                    for pixel in row.as_chunks::<4>().0.iter() {
                        ppm.extend_from_slice(&pixel[..3]);
                    }
                }
                std::fs::write(directory.join(format!("{name}.ppm")), ppm).unwrap();
                outputs.push(format!("{name}: {black} non-white pixels"));
            }
            let mut pressure_doc = Document::default();
            pressure_doc.settings.onion.enabled = false;
            let stroke = Stroke::new(
                200.0,
                Point {
                    x: 960.0,
                    y: 400.0,
                    pressure: 0.1,
                },
            );
            pressure_doc.layers[0].cels.insert(
                0,
                Arc::new(Cel {
                    strokes: vec![Arc::new(stroke)],
                }),
            );
            let mut previous_pixels = 0;
            for pressure in [0.1, 0.25, 0.5, 0.75, 1.0] {
                let cel = Arc::make_mut(pressure_doc.layers[0].cels.get_mut(&0).unwrap());
                Arc::make_mut(&mut cel.strokes[0]).dab(pressure);
                let bytes =
                    unsafe { renderer.read_frame(gl, &pressure_doc.scene(0), [960, 402]) }.unwrap();
                let count = bytes
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .filter(|p| p[0] < 240)
                    .count();
                assert!(
                    count > previous_pixels,
                    "Stationary pressure did not expand the GPU mark"
                );
                previous_pixels = count;
                outputs.push(format!(
                    "Stationary pressure {pressure:.2}: {count} ink pixels"
                ));
            }
            {
                use eframe::glow::HasContext;
                outputs.push(format!("GPU: {}", unsafe {
                    gl.get_parameter_string(eframe::glow::RENDERER)
                }));
            }
            let error = unsafe {
                use eframe::glow::HasContext;
                gl.get_error()
            };
            assert_eq!(error, 0, "OpenGL error");
            std::fs::write(directory.join("smoke.txt"), outputs.join("\n")).unwrap();
            drop(renderer);
            let mut document = self.doc.clone();
            document.settings.export.width = 640;
            document.settings.export.fps = 60;
            document.settings.export.end = 1.0;
            // A PCM fixture exercises decoding, embedding and AAC muxing without an audio device.
            let rate = 48000_u32;
            let mut pcm = Vec::new();
            for i in 0..rate {
                let value = if i % 12000 < 600 {
                    ((i as f32 * 440.0 / rate as f32 * std::f32::consts::TAU).sin() * 12000.0)
                        as i16
                } else {
                    0
                };
                pcm.extend_from_slice(&value.to_le_bytes());
            }
            let mut wav = b"RIFF".to_vec();
            wav.extend_from_slice(&(36 + pcm.len() as u32).to_le_bytes());
            wav.extend_from_slice(b"WAVEfmt ");
            wav.extend_from_slice(&16_u32.to_le_bytes());
            wav.extend_from_slice(&1_u16.to_le_bytes());
            wav.extend_from_slice(&1_u16.to_le_bytes());
            wav.extend_from_slice(&rate.to_le_bytes());
            wav.extend_from_slice(&(rate * 2).to_le_bytes());
            wav.extend_from_slice(&2_u16.to_le_bytes());
            wav.extend_from_slice(&16_u16.to_le_bytes());
            wav.extend_from_slice(b"data");
            wav.extend_from_slice(&(pcm.len() as u32).to_le_bytes());
            wav.extend_from_slice(&pcm);
            let decoded = DecodedAudio::decode(&wav).unwrap();
            assert!((decoded.duration - 1.0).abs() < 1e-6);
            document.audio = Some(AudioAsset {
                name: "test.wav".into(),
                bytes: Arc::new(wav),
            });
            project::save(&directory.join("roundtrip.mori"), &document).unwrap();
            let loaded = project::load(&directory.join("roundtrip.mori")).unwrap();
            let cancelled_path = directory.join("cancelled.mp4");
            std::fs::write(&cancelled_path, b"previous video").unwrap();
            let cancelled = ExportJob::start(&cancelled_path, &loaded).unwrap();
            drop(cancelled);
            assert_eq!(std::fs::read(&cancelled_path).unwrap(), b"previous video");
            let output = directory.join("export.mp4");
            if output.exists() {
                std::fs::remove_file(&output).unwrap();
            }
            self.export = Some(ExportJob::start(&output, &loaded).unwrap());
            self.smoke_finish = Some(directory);
            ctx.request_repaint();
        }
    }
}
fn step_label(step: f64) -> String {
    if step < 1.0 {
        format!("1/{} beat", (1.0 / step).round() as u32)
    } else {
        format!("{step:.0} beat")
    }
}
impl eframe::App for Studio {
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        self.poll_background();
        if let Some(error) = self.transport.take_error() {
            self.time = self.doc.timing.snap(self.transport.position());
            self.notify(error);
        }
        if ctx.input(|i| i.viewport().close_requested())
            && !self.allow_close
            && (self.dirty || self.export.is_some())
        {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.request(Pending::Close);
        }
        if self.allow_close {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        if self.transport.playing {
            let time = self.transport.position();
            if time >= self.doc.timing.duration {
                self.transport.stop();
                self.time = self.doc.timing.snap(self.doc.timing.duration);
            } else {
                self.time = self.doc.timing.at_or_before(time);
            }
            ctx.request_repaint();
        }
        if !ctx.input(|i| i.pointer.any_down()) && !ctx.wants_keyboard_input() {
            self.ui_editing = false;
        }
        self.keyboard(ctx);
        self.header(ctx);
        self.footer(ctx);
        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(BG)
                    .inner_margin(egui::Margin::symmetric(12, 8)),
            )
            .show(ctx, |ui| {
                ui.add_enabled_ui(self.background.is_none() && self.pending.is_none(), |ui| {
                    match self.page {
                        Page::Canvas => self.canvas_page(ui, ctx),
                        Page::Timeline => self.timeline_page(ui, ctx),
                        Page::Settings => self.settings_page(ui, ctx),
                    }
                });
            });
        self.confirmation(ctx);
        if let Some(message) = &self.busy {
            egui::Window::new("Working")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
                .show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label(message);
                    });
                });
            ctx.request_repaint_after(Duration::from_millis(30));
        }
        if self.toast.is_some() {
            ctx.request_repaint_after(Duration::from_millis(500));
        }
        if self.export.is_some() {
            if let Some(gl) = frame.gl() {
                self.export_step(gl);
            }
            ctx.request_repaint_after(Duration::from_millis(1));
        }
        if self.export.is_none() {
            if let Some(mut renderer) = self.export_renderer.take() {
                if let Some(gl) = frame.gl() {
                    unsafe {
                        renderer.destroy(gl);
                    }
                }
            }
        }
        if self.smoke.is_some() || self.smoke_finish.is_some() {
            if let Some(gl) = frame.gl() {
                self.smoke_test(ctx, gl);
            }
        }
    }
    fn on_exit(&mut self, gl: Option<&eframe::glow::Context>) {
        self.transport.stop();
        if let Some(gl) = gl {
            unsafe {
                self.renderer.lock().unwrap().destroy(gl);
                if let Some(renderer) = &mut self.export_renderer {
                    renderer.destroy(gl);
                }
            }
        }
    }
}
