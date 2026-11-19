use crate::{
    audio::{DecodedAudio, Transport},
    curve::StrokeSmoother,
    erase::EraserIndex,
    export::ExportJob,
    i18n::{self, Language},
    model::*,
    native_menu::NativeMenu,
    preferences::{PauseBehavior, PreferenceStore, Preferences},
    project,
    render::{self, Ghost, Renderer, View},
    tablet::{self, PointerAction, Tablet},
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
mod ui;
const BG: Color32 = Color32::from_rgb(17, 18, 20);
const PANEL: Color32 = Color32::from_rgb(23, 24, 27);
const INK: Color32 = Color32::from_rgb(227, 228, 232);
const MUTED: Color32 = Color32::from_rgb(121, 125, 136);
const ACCENT: Color32 = Color32::from_rgb(183, 193, 221);
const LINE: Color32 = Color32::from_rgb(41, 43, 49);
#[derive(Clone, Copy, PartialEq)]
enum Page {
    Canvas,
    Timeline,
    Settings,
}
type Tool = Brush;
struct Drawing {
    layer: usize,
    time: Tick,
    last: Point,
    smoother: Option<StrokeSmoother>,
    liquify: bool,
    index: Option<SpatialIndex>,
    eraser: Option<EraserIndex>,
    recorded: bool,
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
    preferences: Preferences,
    preference_store: PreferenceStore,
    native_menu: NativeMenu,
    system_locale: String,
    reset_preferences: bool,
    settings_tab: usize,
    layer_scroll: f32,
    page: Page,
    return_page: Page,
    layer: usize,
    time: f64,
    mirror: bool,
    dirty: bool,
    path: Option<PathBuf>,
    renderer: Arc<Mutex<Renderer>>,
    pacing: Arc<eframe::FramePacing>,
    transport: Transport,
    play_origin: f64,
    play_timeline_start: f64,
    play_timeline_zoom: f32,
    drawing: Option<Drawing>,
    tablet: Tablet,
    audio_bytes: Option<Arc<Vec<u8>>>,
    toast: Option<(String, Instant)>,
    error: Option<String>,
    resized: Instant,
    export: Option<ExportJob>,
    export_renderer: Option<Renderer>,
    background: Option<Receiver<Background>>,
    busy: Option<String>,
    bind_capture: Option<usize>,
    ui_editing: bool,
    timing_origin: Option<Document>,
    timeline_zoom: f32,
    timeline_start: f64,
    pending: Option<Pending>,
    allow_close: bool,
    fullscreen: bool,
    brush_wheel: Option<Pos2>,
    frame_drag: Option<(usize, Tick, Tick)>,
    smoke: Option<PathBuf>,
    smoke_finish: Option<PathBuf>,
    smoke_started: Instant,
}
fn text(ui: &mut egui::Ui, s: impl Into<String>, size: f32, color: Color32) {
    ui.label(
        RichText::new(i18n::tr(ui.ctx(), s.into()))
            .size(size)
            .color(color),
    );
}
impl Studio {
    pub fn new(cc: &eframe::CreationContext<'_>, pacing: Arc<eframe::FramePacing>) -> Self {
        ui::configure(&cc.egui_ctx);
        let (preference_store, preferences, warning) = PreferenceStore::open();
        pacing.set_max_fps(preferences.frame_rate);
        let system_locale = i18n::system_locale();
        i18n::use_language(&cc.egui_ctx, preferences.language, &system_locale);
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
            doc: preferences.new_document(),
            preferences,
            preference_store,
            native_menu: NativeMenu::new(cc.egui_ctx.clone()),
            system_locale,
            reset_preferences: false,
            settings_tab: 0,
            layer_scroll: 0.0,
            history: History::default(),
            page: Page::Canvas,
            return_page: Page::Canvas,
            layer: 0,
            time: 0.0,
            mirror: false,
            dirty: false,
            path: None,
            renderer,
            pacing,
            transport: Transport::default(),
            play_origin: 0.0,
            play_timeline_start: 0.0,
            play_timeline_zoom: 1.0,
            drawing: None,
            tablet: Tablet::new(cc),
            audio_bytes: None,
            toast: None,
            error: None,
            resized: Instant::now() - Duration::from_secs(10),
            export: None,
            export_renderer: None,
            background: None,
            busy: None,
            bind_capture: None,
            ui_editing: false,
            timing_origin: None,
            timeline_zoom: 1.0,
            timeline_start: 0.0,
            pending: None,
            allow_close: false,
            fullscreen: false,
            brush_wheel: None,
            frame_drag: None,
            smoke,
            smoke_finish: None,
            smoke_started: Instant::now(),
        };
        if let Some(warning) = warning {
            studio.notify(warning);
        }
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
    fn fail(&mut self, message: impl Into<String>) {
        self.error = Some(message.into());
    }
    fn translated(&self, text: &str) -> String {
        i18n::translate(self.preferences.language.chinese(&self.system_locale), text).into_owned()
    }
    fn export_preferences(&mut self) {
        if let Some(path) = rfd::FileDialog::new()
            .set_title(self.translated("Export settings…"))
            .add_filter(self.translated("Settings file"), &["json"])
            .set_file_name("mori-settings.json")
            .save_file()
        {
            match crate::preferences::export_settings(&path, &self.preferences) {
                Ok(()) => self.notify("Settings exported"),
                Err(error) => self.fail(error),
            }
        }
    }
    fn import_preferences(&mut self) {
        if let Some(path) = rfd::FileDialog::new()
            .set_title(self.translated("Import settings…"))
            .add_filter(self.translated("Settings file"), &["json"])
            .pick_file()
        {
            match crate::preferences::import_settings(&path) {
                Ok(preferences) => {
                    self.preferences = preferences;
                    self.bind_capture = None;
                    self.notify("Settings transferred");
                }
                Err(error) => self.fail(error),
            }
        }
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
            "Canvas" => {
                self.frame_drag = None;
                self.page = Page::Canvas;
                self.return_page = Page::Canvas;
            }
            "Timeline" => {
                self.page = Page::Timeline;
                self.return_page = Page::Timeline;
            }
            "Settings" => {
                self.frame_drag = None;
                self.brush_wheel = None;
                if self.page == Page::Settings {
                    self.page = self.return_page;
                } else {
                    self.return_page = self.page;
                    self.page = Page::Settings;
                }
            }
            "Save" => self.save(false),
            "Save as" => self.save(true),
            "Import music" => self.import_audio(),
            "Export" => self.start_export(),
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
                } else {
                    self.notify(format!("Nothing to {}", action.to_lowercase()));
                }
            }
            "Blank frame" | "Copy previous frame" if self.page == Page::Canvas => {
                self.transport.stop();
                let time = tick(self.time);
                if action == "Copy previous frame"
                    && self.doc.layers[self.layer]
                        .cels
                        .range(..time)
                        .next_back()
                        .is_none()
                {
                    self.notify("No previous frame");
                    return;
                }
                self.record();
                if action == "Blank frame" {
                    self.doc.blank_frame(self.layer, time);
                } else {
                    self.doc.copy_previous_frame(self.layer, time);
                }
            }
            "Mirror" => {
                self.mirror = !self.mirror;
            }
            "Previous" => self.set_time(self.doc.timing.previous(self.time)),
            "Next" => self.set_time(self.doc.timing.next(self.time)),
            "Layer up" => {
                self.layer = (self.layer + 1).min(self.doc.layers.len() - 1);
            }
            "Layer down" => {
                self.layer = self.layer.saturating_sub(1);
            }
            "Smaller" | "Larger" => {
                if self.doc.layers[self.layer].brush == Brush::Fill {
                    return;
                }
                if self.drawing.as_ref().is_none_or(|d| !d.recorded) {
                    self.record();
                    if let Some(drawing) = &mut self.drawing {
                        drawing.recorded = true;
                    }
                }
                let brush = self.doc.layers[self.layer].brush;
                let width = self.doc.layers[self.layer].brush_width_mut(brush);
                *width = if action == "Larger" {
                    (*width * 1.15).max(*width + 1.0)
                } else {
                    (*width / 1.15).min(*width - 1.0)
                }
                .clamp(1.0, 1920.0);
                self.resized = Instant::now();
            }
            "Play" => self.toggle_play(),
            "Onion skin" => {
                self.preferences.onion.enabled = !self.preferences.onion.enabled;
            }
            "Fullscreen" => {
                self.fullscreen =
                    !ctx.input(|i| i.viewport().fullscreen.unwrap_or(self.fullscreen));
                ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(self.fullscreen));
            }
            _ => {}
        }
    }
    fn keyboard(&mut self, ctx: &egui::Context) {
        if self.pending.is_some() || self.background.is_some() || self.reset_preferences {
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
                        .preferences
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
                        self.notify(format!(
                            "{} {}",
                            self.translated("Already used by"),
                            self.translated(&action)
                        ));
                    } else {
                        let binding = &mut self.preferences.bindings[index];
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
                    .preferences
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
                    if ctx.wants_keyboard_input() && matches!(action.as_str(), "Undo" | "Redo") {
                        continue;
                    }
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
    fn playback_time(&self) -> f64 {
        let time = self.transport.position();
        if self.transport.audio.is_some() {
            self.preferences.visual_time(time)
        } else {
            time
        }
    }
    fn toggle_play(&mut self) {
        if self.transport.playing {
            let t = self.preferences.pause_behavior.position(
                &self.doc.timing,
                self.play_origin,
                self.playback_time(),
            );
            self.set_time(t);
            if self.preferences.pause_behavior == PauseBehavior::ReturnToStart {
                self.timeline_start = self.play_timeline_start;
                self.timeline_zoom = self.play_timeline_zoom;
            }
        } else {
            if self.time >= self.doc.timing.duration {
                self.time = 0.0;
            }
            match self.transport.play(self.time) {
                Ok(()) => {
                    self.play_origin = self.time;
                    self.play_timeline_start = self.timeline_start;
                    self.play_timeline_zoom = self.timeline_zoom;
                }
                Err(error) => self.fail(error),
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
                Err(e) => self.fail(e),
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
                self.doc = self.preferences.new_document();
                self.history = History::default();
                self.path = None;
                self.layer = 0;
                self.time = 0.0;
                self.dirty = false;
                self.transport = Transport::default();
                self.audio_bytes = None;
                self.notify("New project");
            }
            Pending::Open => {
                if let Some(path) = rfd::FileDialog::new()
                    .add_filter(self.translated("MORI project"), &["mori"])
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
                .add_filter(self.translated("MORI project"), &["mori"])
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
            .add_filter(
                self.translated("Music"),
                &["wav", "mp3", "flac", "ogg", "m4a", "aac"],
            )
            .pick_file()
        {
            self.import_audio_path(path);
        }
    }
    fn import_audio_path(&mut self, path: PathBuf) {
        if self.background.is_some() {
            return;
        }
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
                        self.notify("Audio imported");
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
                        self.notify("Saved");
                        if let Some(pending) = self.pending.take() {
                            self.perform(pending);
                        }
                    }
                    Background::Error(error) => {
                        self.fail(error);
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
    fn view(&self, onion: bool) -> View {
        let mut ghosts = Vec::new();
        let settings = &self.preferences.onion;
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
                    scene: {
                        let mut scene = self.doc.scene(*t);
                        scene.rhythm = None;
                        scene
                    },
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
                    scene: {
                        let mut scene = self.doc.scene(*t);
                        scene.rhythm = None;
                        scene
                    },
                    color: settings.after,
                    opacity: settings.opacity * settings.falloff.powi(i as i32),
                });
            }
        }
        View {
            scene: self.doc.scene(tick(if self.transport.playing {
                self.playback_time()
            } else {
                self.time
            })),
            ghosts,
            mirror: self.mirror,
        }
    }
    fn pointer_input(&mut self, ctx: &egui::Context, paper: Rect, visible: Rect) {
        if self.transport.playing
            || self.background.is_some()
            || self.pending.is_some()
            || self.brush_wheel.is_some()
        {
            return;
        }
        let events = ctx.input(|i| i.events.clone());
        let native = self.tablet.take_samples();
        let samples = native.unwrap_or_else(|| {
            tablet::event_samples(
                &events,
                self.tablet.pressure().unwrap_or(1.0),
                self.tablet.angle().unwrap_or(0.0),
            )
        });
        crate::timing::input_samples(
            samples
                .iter()
                .filter(|s| s.action == PointerAction::Move)
                .count(),
        );
        for sample in samples {
            let point = self.canvas_sample(sample.position, paper, sample.pressure, sample.angle);
            match sample.action {
                PointerAction::Down if visible.contains(sample.position) => {
                    self.start_stroke(point)
                }
                PointerAction::Move if self.drawing.is_some() => self.move_stroke(point),
                PointerAction::Up if self.drawing.is_some() => {
                    self.move_stroke(point);
                    self.finish_stroke();
                }
                PointerAction::Cancel => self.finish_stroke(),
                _ => {}
            }
        }
        if events.iter().any(|event| {
            matches!(
                event,
                egui::Event::WindowFocused(false) | egui::Event::PointerGone
            )
        }) {
            self.finish_stroke();
        }
    }
    fn canvas_sample(&self, pos: Pos2, paper: Rect, pressure: f32, angle: f32) -> Point {
        let mut x = (pos.x - paper.left()) / paper.width() * 1920.0;
        if self.mirror {
            x = 1920.0 - x;
        }
        Point {
            angle: if self.mirror {
                std::f32::consts::PI - angle
            } else {
                angle
            },
            x,
            y: (pos.y - paper.top()) / paper.width() * 1920.0,
            pressure: pressure
                .clamp(0.0, 1.0)
                .powf(self.preferences.pressure_gamma)
                .max(self.preferences.minimum_pressure),
        }
    }
    fn start_stroke(&mut self, point: Point) {
        if self.doc.layers[self.layer].brush == Tool::Eraser {
            let cel = self.doc.layers[self.layer].held(tick(self.time));
            let index = EraserIndex::new(cel.map_or(&Cel::default(), |c| c));
            self.drawing = Some(Drawing {
                layer: self.layer,
                time: tick(self.time),
                last: point,
                smoother: None,
                liquify: false,
                index: None,
                eraser: Some(index),
                recorded: false,
            });
            self.erase_segment(point);
            return;
        }
        self.record();
        let brush = self.doc.layers[self.layer].brush;
        let liquify = brush == Tool::Liquify;
        let time = tick(self.time);
        let layer = self.layer;
        let width = self.doc.layers[layer].brush_width(self.doc.layers[self.layer].brush);
        let movement = self.doc.layers[layer].blend == Blend::Move;
        let cel = self.doc.layers[layer].begin_edit(time, liquify);
        let index = if liquify {
            Some(SpatialIndex::new(cel, width * 0.5))
        } else {
            if movement {
                cel.strokes.clear();
            }
            let mut stroke = Stroke::new(width, point);
            stroke.brush = brush;
            cel.strokes.push(Arc::new(stroke));
            None
        };
        self.drawing = Some(Drawing {
            layer,
            time,
            last: point,
            smoother: if !liquify
                && !movement
                && self.preferences.interpolation == Interpolation::Smooth
            {
                Some(StrokeSmoother::new(point))
            } else {
                None
            },
            liquify,
            index,
            eraser: None,
            recorded: true,
        });
    }
    fn erase_segment(&mut self, point: Point) {
        let Some(mut drawing) = self.drawing.take() else {
            return;
        };
        let width = self.doc.layers[drawing.layer].brush_width(Tool::Eraser);
        let hits = drawing
            .eraser
            .as_mut()
            .unwrap()
            .sweep(drawing.last, point, width * 0.5);
        if !hits.is_empty() {
            if !drawing.recorded {
                self.record();
                drawing.recorded = true;
            }
            self.doc.layers[drawing.layer].erase_at(drawing.time, &hits);
        }
        drawing.last = point;
        self.drawing = Some(drawing);
    }
    fn move_stroke(&mut self, point: Point) {
        if self.drawing.as_ref().is_some_and(|d| d.eraser.is_some()) {
            self.erase_segment(point);
            return;
        }
        let Some(drawing) = &mut self.drawing else {
            return;
        };
        let delta = [point.x - drawing.last.x, point.y - drawing.last.y];
        let distance = drawing.last.distance(point);
        if self.doc.layers[drawing.layer].blend != Blend::Move
            && self.doc.layers[drawing.layer].brush == Tool::Fill
        {
            drawing.last = point;
            return;
        }
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
                    if stroke.brush == Brush::Marker
                        && interpolate_angle(stroke.points.last().unwrap().angle, point.angle, 0.5)
                            != stroke.points.last().unwrap().angle
                    {
                        stroke.points.last_mut().unwrap().angle = point.angle;
                        stroke.invalidate_from(stroke.points.len() - 1);
                        if let Some(smoother) = &mut drawing.smoother {
                            smoother.stationary_angle(point.angle);
                        }
                    }
                    if let Some(smoother) = &mut drawing.smoother {
                        smoother.stationary_pressure(point.pressure);
                    }
                    if stroke.dab(point.pressure) {
                        drawing.last.pressure = point.pressure;
                    }
                }
            }
            return;
        }
        let layer = &mut self.doc.layers[drawing.layer];
        let width = layer.brush_width(if drawing.liquify {
            Tool::Liquify
        } else {
            Tool::Ink
        });
        let cel = Arc::make_mut(layer.cels.get_mut(&drawing.time).unwrap());
        if drawing.liquify {
            // Sweep the full pointer segment, keeping fast gestures continuous.
            let radius = width * 0.5;
            let steps = (distance / (radius * 0.2).max(1.0)).ceil() as usize;
            for i in 0..steps {
                let t = (i + 1) as f32 / steps as f32;
                let center = Point {
                    angle: 0.0,
                    x: drawing.last.x + delta[0] * t,
                    y: drawing.last.y + delta[1] * t,
                    pressure: point.pressure,
                };
                drawing.index.as_mut().unwrap().deform(
                    cel,
                    center,
                    [delta[0] / steps as f32, delta[1] / steps as f32],
                    radius,
                    self.preferences.liquify_strength * point.pressure,
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
                    stroke.invalidate_from(0);
                }
            } else if let Some(smoother) = &mut drawing.smoother {
                smoother.push(stroke, point, self.preferences.spacing);
            } else {
                let steps = (distance / self.preferences.spacing).ceil().max(1.0) as usize;
                let start = drawing.last;
                for i in 1..=steps {
                    let t = i as f32 / steps as f32;
                    stroke.points.push(Point {
                        angle: interpolate_angle(start.angle, point.angle, t),
                        x: start.x + delta[0] * t,
                        y: start.y + delta[1] * t,
                        pressure: start.pressure + (point.pressure - start.pressure) * t,
                    });
                }
            }
        }
        drawing.last = point;
    }
    fn start_export(&mut self) {
        self.transport.stop();
        if let Some(path) = rfd::FileDialog::new()
            .add_filter(self.translated("MP4 video"), &["mp4"])
            .set_file_name(format!("{}.mp4", self.doc.name))
            .save_file()
        {
            let mut document = self.doc.clone();
            self.preferences.apply_output(&mut document.settings.export);
            match ExportJob::start(&path, &document) {
                Ok(job) => {
                    self.export = Some(job);
                    self.notify("Export started");
                }
                Err(e) => self.fail(e),
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
                Ok(path) => self.notify(format!(
                    "{} · {}",
                    self.translated("Video saved"),
                    path.display()
                )),
                Err(e) => self.fail(e),
            }
        }
    }
    fn confirmation(&mut self, ctx: &egui::Context) {
        if let Some(pending) = self.pending {
            egui::Window::new(self.translated("Save your motion?"))
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
                            .add_enabled(
                                self.dirty,
                                egui::Button::new(self.translated("Save & continue")),
                            )
                            .clicked()
                        {
                            self.save(false);
                        }
                        if ui
                            .button(self.translated(if self.dirty {
                                "Discard changes"
                            } else {
                                "Stop export & close"
                            }))
                            .clicked()
                        {
                            self.pending = None;
                            self.perform(pending);
                        }
                        if ui.button(self.translated("Cancel")).clicked() {
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
                        angle: 0.0,
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
                        angle: 0.0,
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
                    angle: 0.0,
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
            unsafe {
                renderer.verify_sampler_bindings(gl);
            }
            outputs.push("All compositor samplers are complete and do not alias the output, including watermark-only and movement-only scenes".into());
            outputs.push(unsafe { Renderer::verify_mask_precision(gl) });
            outputs.push(unsafe { renderer.verify_retained_rasters(gl) });

            let mut fill = Document::default();
            fill.settings.watermark = false;
            fill.layers.truncate(1);
            let point = Point {
                x: 500.0,
                y: 200.0,
                pressure: 1.0,
                angle: 0.0,
            };
            let mut stroke = Stroke::new(42.0, point);
            stroke.brush = Brush::Fill;
            fill.layers[0]
                .begin_edit(0, false)
                .strokes
                .push(Arc::new(stroke));
            let pixels = unsafe { renderer.read_frame(gl, &fill.scene(0), [960, 402]).unwrap() };
            assert!(pixels
                .as_chunks::<4>()
                .0
                .iter()
                .all(|p| p[0] < 3 && p[1] < 3 && p[2] < 3));
            outputs.push(
                "Fill paints the canvas black with ordinary canvas-sized render targets".into(),
            );
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
                            angle: 0.0,
                            x: 800.0,
                            y: 400.0,
                            pressure: 1.0,
                        },
                    );
                    stroke.points.push(Point {
                        angle: 0.0,
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
            // Exact integer-aligned rhythm cells, including group reset and project switch.
            let mut rhythm_doc = Document::default();
            for (time, filled) in [(0.0, 1), (0.5, 2), (2.0, 5), (3.5, 8), (4.0, 1)] {
                let pixels =
                    unsafe { renderer.read_frame(gl, &rhythm_doc.scene(tick(time)), [1920, 804]) }
                        .unwrap();
                for row in 0..2 {
                    for col in 0..4 {
                        for y in 0..5 {
                            for x in 0..5 {
                                let px = 1870 + col * 7 + x;
                                let py = 24 + (1 - row) * 7 + y;
                                let i = (py * 1920 + px) * 4;
                                let expected = if row * 4 + col < filled { 0 } else { 204 };
                                assert_eq!(
                                    &pixels[i..i + 3],
                                    &[expected; 3],
                                    "Rhythm cells are not pixel aligned or have wrong beat state"
                                );
                            }
                        }
                    }
                }
                let gap = (26 * 1920 + 1875) * 4;
                assert_eq!(
                    &pixels[gap..gap + 3],
                    &[255; 3],
                    "Rhythm grid gap is not pixel-perfect"
                );
            }
            let mut effect = Layer::new("Watermark move", Blend::Move, 100.0);
            let mut motion = Stroke::new(
                100.0,
                Point {
                    x: 0.0,
                    y: 0.0,
                    pressure: 1.0,
                    angle: 0.0,
                },
            );
            motion.points.push(Point {
                x: -200.0,
                y: -100.0,
                pressure: 1.0,
                angle: 0.0,
            });
            effect.cels.insert(
                0,
                Arc::new(Cel {
                    strokes: vec![Arc::new(motion)],
                }),
            );
            rhythm_doc.layers.push(effect);
            let moved =
                unsafe { renderer.read_frame(gl, &rhythm_doc.scene(0), [1920, 804]) }.unwrap();
            let original = (33 * 1920 + 1872) * 4;
            let translated = (133 * 1920 + 1672) * 4;
            assert_eq!(
                &moved[original..original + 3],
                &[255; 3],
                "Watermark was reapplied above movement"
            );
            assert!(
                moved[translated] < 40,
                "Movement did not affect the virtual watermark layer"
            );
            rhythm_doc.layers.pop();
            rhythm_doc.layers[0].inverted = true;
            let inverted =
                unsafe { renderer.read_frame(gl, &rhythm_doc.scene(0), [1920, 804]) }.unwrap();
            assert_eq!(
                &inverted[original..original + 3],
                &[255; 3],
                "XOR inversion did not apply to watermark"
            );
            assert_eq!(&inverted[..3], &[0; 3]);
            rhythm_doc.layers[0].inverted = false;
            rhythm_doc.settings.watermark = false;
            let pixels =
                unsafe { renderer.read_frame(gl, &rhythm_doc.scene(0), [1920, 804]) }.unwrap();
            assert!(pixels.as_chunks::<4>().0.iter().all(|p| p[..3] == [255; 3]));
            outputs.push("Rhythm watermark: 1920px integer cells, every beat, group reset, gaps and disabled state verified".into());
            // The hard marker nib rotates by tilt, respects pressure and round-trips as vectors.
            let mut marker_doc = Document::default();
            marker_doc.settings.watermark = false;
            let mut marker = Stroke::new(
                200.0,
                Point {
                    x: 960.0,
                    y: 400.0,
                    pressure: 1.0,
                    angle: 0.0,
                },
            );
            marker.brush = Brush::Marker;
            marker_doc.layers[0].cels.insert(
                0,
                Arc::new(Cel {
                    strokes: vec![Arc::new(marker)],
                }),
            );
            for (angle, pressure, w, h) in [
                (0.0, 1.0, 100, 26),
                (std::f32::consts::FRAC_PI_2, 1.0, 26, 100),
                (0.0, 0.5, 50, 14),
            ] {
                let stroke = Arc::make_mut(
                    &mut Arc::make_mut(marker_doc.layers[0].cels.get_mut(&0).unwrap()).strokes[0],
                );
                stroke.points[0].angle = angle;
                stroke.points[0].pressure = pressure;
                stroke.invalidate_from(0);
                let pixels =
                    unsafe { renderer.read_frame(gl, &marker_doc.scene(0), [960, 402]) }.unwrap();
                let mut bounds = [960, 402, 0, 0];
                for (i, p) in pixels.as_chunks::<4>().0.iter().enumerate() {
                    if p[0] < 250 {
                        let x = i % 960;
                        let y = i / 960;
                        bounds[0] = bounds[0].min(x);
                        bounds[1] = bounds[1].min(y);
                        bounds[2] = bounds[2].max(x);
                        bounds[3] = bounds[3].max(y);
                    }
                }
                assert!(
                    (bounds[2] - bounds[0] + 1).abs_diff(w) <= 1
                        && (bounds[3] - bounds[1] + 1).abs_diff(h) <= 1,
                    "Marker pressure/tilt footprint is wrong: {bounds:?}"
                );
            }
            let first = Point {
                x: 400.0,
                y: 400.0,
                pressure: 0.4,
                angle: 0.0,
            };
            let mut marker = Stroke::new(70.0, first);
            marker.brush = Brush::Marker;
            marker_doc.layers[0].cels.insert(
                0,
                Arc::new(Cel {
                    strokes: vec![Arc::new(marker)],
                }),
            );
            let mut smoother = StrokeSmoother::new(first);
            for i in 1..=18 {
                let a = i as f32 * 0.11;
                let p = Point {
                    x: 400.0 + i as f32 * 45.0,
                    y: 400.0 + (a * 2.0).sin() * 100.0,
                    pressure: 0.4 + i as f32 / 40.0,
                    angle: a,
                };
                let cel = Arc::make_mut(marker_doc.layers[0].cels.get_mut(&0).unwrap());
                smoother.push(Arc::make_mut(&mut cel.strokes[0]), p, 1.5);
                if i % 3 == 0 {
                    unsafe {
                        renderer.verify_curve_cache(gl, &marker_doc);
                    }
                }
            }
            let pixels =
                unsafe { renderer.read_frame(gl, &marker_doc.scene(0), [960, 402]) }.unwrap();
            let mut ppm = b"P6\n960 402\n255\n".to_vec();
            for row in pixels.as_chunks::<{ 960 * 4 }>().0.iter().rev() {
                for p in row.as_chunks::<4>().0 {
                    ppm.extend_from_slice(&p[..3]);
                }
            }
            std::fs::write(directory.join("marker-tilt.ppm"), ppm).unwrap();
            project::save(&directory.join("marker.mori"), &marker_doc).unwrap();
            let loaded = project::load(&directory.join("marker.mori")).unwrap();
            assert_eq!(loaded.layers[0].cels[&0].strokes[0].brush, Brush::Marker);
            assert!(
                loaded.layers[0].cels[&0].strokes[0]
                    .points
                    .last()
                    .unwrap()
                    .angle
                    > 1.8
            );
            outputs.push("Marker: hard rectangle, tilt rotation, pressure diameter, incremental curve and project persistence verified".into());
            // Long curves edit their live suffix; incremental VBO contents must remain
            // identical to fresh geometry through batching, pressure, undo and branching.
            let mut curve_doc = Document::default();
            let first = Point {
                angle: 0.0,
                x: 1240.0,
                y: 400.0,
                pressure: 0.2,
            };
            curve_doc.layers[0].cels.insert(
                0,
                Arc::new(Cel {
                    strokes: vec![Arc::new(Stroke::new(12.0, first))],
                }),
            );
            let mut smoother = StrokeSmoother::new(first);
            let mut checkpoint = None;
            for i in 1..=36 {
                let a = i as f32 / 12.0 * std::f32::consts::TAU;
                let point = Point {
                    angle: 0.0,
                    x: 960.0 + a.cos() * 280.0,
                    y: 400.0 + a.sin() * 280.0,
                    pressure: 0.2 + i as f32 / 60.0,
                };
                let cel = Arc::make_mut(curve_doc.layers[0].cels.get_mut(&0).unwrap());
                smoother.push(Arc::make_mut(&mut cel.strokes[0]), point, 1.5);
                if i == 6 {
                    checkpoint = Some((curve_doc.clone(), smoother.clone()));
                }
                if i % 3 == 0 {
                    unsafe {
                        renderer.verify_curve_cache(gl, &curve_doc);
                    }
                }
                if i == 12 {
                    let pixels =
                        unsafe { renderer.read_frame(gl, &curve_doc.scene(0), [960, 402]) }
                            .unwrap();
                    let mut ppm = b"P6\n960 402\n255\n".to_vec();
                    for row in pixels.as_chunks::<{ 960 * 4 }>().0.iter().rev() {
                        for pixel in row.as_chunks::<4>().0 {
                            ppm.extend_from_slice(&pixel[..3]);
                        }
                    }
                    std::fs::write(directory.join("smooth-sparse-circle.ppm"), ppm).unwrap();
                }
            }
            let (mut previous, mut previous_smoother) = checkpoint.unwrap();
            unsafe {
                renderer.verify_curve_cache(gl, &previous);
                renderer.verify_curve_cache(gl, &curve_doc);
            }
            let cel = Arc::make_mut(previous.layers[0].cels.get_mut(&0).unwrap());
            previous_smoother.push(
                Arc::make_mut(&mut cel.strokes[0]),
                Point {
                    angle: 0.0,
                    x: 600.0,
                    y: 450.0,
                    pressure: 0.6,
                },
                1.5,
            );
            unsafe {
                renderer.verify_curve_cache(gl, &previous);
            }
            let cel = Arc::make_mut(previous.layers[0].cels.get_mut(&0).unwrap());
            Arc::make_mut(&mut cel.strokes[0]).dab(1.0);
            unsafe {
                renderer.verify_curve_cache(gl, &previous);
            }
            outputs.push("Smooth sparse input: incremental GPU geometry matches full rebuild through pressure, undo/redo and branching".into());
            // Blur regressions: all styles stay inside the swept, pressure-sized footprint.
            let mut local_doc = Document::default();
            local_doc.settings.watermark = false;
            local_doc.settings.aspect = 2.0;
            let mut base = Cel::default();
            for x in [200.0, 800.0, 1400.0] {
                let mut stroke = Stroke::new(
                    18.0,
                    Point {
                        angle: 0.0,
                        x,
                        y: 100.0,
                        pressure: 1.0,
                    },
                );
                stroke.points.push(Point {
                    angle: 0.0,
                    x,
                    y: 860.0,
                    pressure: 1.0,
                });
                base.strokes.push(Arc::new(stroke));
            }
            local_doc.layers[0].cels.insert(0, Arc::new(base));
            let original =
                unsafe { renderer.read_frame(gl, &local_doc.scene(0), [384, 192]) }.unwrap();
            let mut blur_stroke = Stroke::new(
                200.0,
                Point {
                    angle: 0.0,
                    x: 600.0,
                    y: 480.0,
                    pressure: 1.0,
                },
            );
            blur_stroke.points.push(Point {
                angle: 0.0,
                x: 1000.0,
                y: 480.0,
                pressure: 1.0,
            });
            local_doc.layers[3].cels.insert(
                0,
                Arc::new(Cel {
                    strokes: vec![Arc::new(blur_stroke)],
                }),
            );
            for style in Style::ALL {
                for inverted in [false, true] {
                    local_doc.layers[3].style = style;
                    local_doc.layers[3].inverted = inverted;
                    let blurred =
                        unsafe { renderer.read_frame(gl, &local_doc.scene(0), [384, 192]) }
                            .unwrap();
                    let mut changed_inside = 0;
                    for y in 0..192 {
                        for x in 0..384 {
                            let i = (y * 384 + x) * 4;
                            let px = x as f32 * 5.0 + 2.5;
                            let py = (191 - y) as f32 * 5.0 + 2.5;
                            let inside =
                                (495.0..=1105.0).contains(&px) && (375.0..=585.0).contains(&py);
                            let changed = original[i..i + 4] != blurred[i..i + 4];
                            assert!(
                                !changed || inside,
                                "Blur {:?} changed pixels outside its trajectory",
                                style
                            );
                            changed_inside += usize::from(changed);
                        }
                    }
                    assert!(changed_inside > 50, "Local blur had no effect");
                }
            }
            local_doc.layers[3].style = Style::Clean;
            local_doc.layers[3].inverted = false;
            let once = unsafe { renderer.read_frame(gl, &local_doc.scene(0), [384, 192]) }.unwrap();
            let mut second = Stroke::new(
                120.0,
                Point {
                    angle: 0.0,
                    x: 1300.0,
                    y: 650.0,
                    pressure: 1.0,
                },
            );
            second.points.push(Point {
                angle: 0.0,
                x: 1500.0,
                y: 650.0,
                pressure: 1.0,
            });
            Arc::make_mut(local_doc.layers[3].cels.get_mut(&0).unwrap())
                .strokes
                .push(Arc::new(second));
            let twice =
                unsafe { renderer.read_frame(gl, &local_doc.scene(0), [384, 192]) }.unwrap();
            let mut second_changes = 0;
            for y in 0..192 {
                for x in 0..384 {
                    let i = (y * 384 + x) * 4;
                    if x < 230 {
                        assert_eq!(
                            once[i..i + 4],
                            twice[i..i + 4],
                            "Second blur gesture changed the first region"
                        );
                    } else {
                        second_changes += usize::from(once[i..i + 4] != twice[i..i + 4]);
                    }
                }
            }
            assert!(second_changes > 20, "Second blur stroke did not survive");
            outputs.push(
                "Local blur: all styles/inversions preserve outside pixels; two strokes survive"
                    .into(),
            );
            unsafe {
                renderer.verify_blur_directions(gl);
            }
            outputs
                .push("Local blur: curved tangents, pressure and coverage verified on GPU".into());
            let mut pressure_doc = Document::default();
            pressure_doc.settings.watermark = false;

            let stroke = Stroke::new(
                200.0,
                Point {
                    angle: 0.0,
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
            let mut loaded = project::load(&directory.join("roundtrip.mori")).unwrap();
            loaded.settings.export.width = 640;
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
impl eframe::App for Studio {
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        crate::timing::frame(ctx, frame, &self.pacing);
        let language_at_frame_start = self.preferences.language;
        i18n::use_language(ctx, self.preferences.language, &self.system_locale);
        let chinese = self.preferences.language.chinese(&self.system_locale);
        let menu_enabled = self.drawing.is_none()
            && self.background.is_none()
            && self.pending.is_none()
            && !self.reset_preferences;
        self.native_menu
            .sync(chinese, &self.preferences.bindings, menu_enabled);
        for action in self.native_menu.take_actions() {
            if !menu_enabled || self.pending.is_some() || self.background.is_some() {
                continue;
            }
            if action == "Settings" {
                if self.page != Page::Settings {
                    self.return_page = self.page;
                }
                self.page = Page::Settings;
                self.brush_wheel = None;
            } else {
                self.action(action, ctx);
            }
        }
        self.poll_background();
        if let Some(error) = self.transport.take_error() {
            self.time = self.doc.timing.snap(self.transport.position());
            self.fail(error);
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
            let time = self.playback_time();
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
            self.timing_origin = None;
        }
        self.keyboard(ctx);
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(BG).inner_margin(0))
            .show(ctx, |ui| {
                ui.add_enabled_ui(self.background.is_none() && self.pending.is_none(), |ui| {
                    match self.page {
                        Page::Canvas => self.canvas_page(ui, ctx),
                        Page::Timeline => self.timeline_page(ui, ctx),
                        Page::Settings => self.settings_page(ui, ctx),
                    }
                });
            });
        self.tablet.clear_samples();
        self.confirmation(ctx);
        if let Some(error) = self.error.take() {
            rfd::MessageDialog::new()
                .set_title("MORI")
                .set_level(rfd::MessageLevel::Error)
                .set_description(self.translated(&error))
                .show();
        }
        if self.page != Page::Canvas {
            self.feedback(ctx);
        }
        if let Err(error) = self.preference_store.save_if_changed(&self.preferences) {
            self.notify(error);
        }
        if self.preferences.language != language_at_frame_start {
            ctx.request_repaint();
        }
        if self.busy.is_some() {
            ctx.set_cursor_icon(egui::CursorIcon::Wait);
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
        self.pacing.set_max_fps(self.preferences.frame_rate);
        // Continue at display cadence during gestures. When input stops, the
        // final repaint consumes its last samples and leaves the event loop idle.
        if ctx.input(|i| i.focused && (i.pointer.is_moving() || i.pointer.any_down())) {
            ctx.request_repaint();
        }
        crate::timing::overlay(ctx);
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
