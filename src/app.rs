use crate::{
    audio::{DecodedAudio, Transport},
    curve::StrokeSmoother,
    export::ExportJob,
    model::*,
    preferences::{PauseBehavior, PreferenceStore, Preferences},
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
    settings_tab: usize,
    layer_scroll: f32,
    page: Page,
    return_page: Page,
    tool: Tool,
    layer: usize,
    time: f64,
    mirror: bool,
    dirty: bool,
    path: Option<PathBuf>,
    renderer: Arc<Mutex<Renderer>>,
    transport: Transport,
    play_origin: f64,
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
impl Studio {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        ui::configure(&cc.egui_ctx);
        let (preference_store, preferences, warning) = PreferenceStore::open();
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
            settings_tab: 0,
            layer_scroll: 0.0,
            history: History::default(),
            page: Page::Canvas,
            return_page: Page::Canvas,
            tool: Tool::Ink,
            layer: 0,
            time: 0.0,
            mirror: false,
            dirty: false,
            path: None,
            renderer,
            transport: Transport::default(),
            play_origin: 0.0,
            drawing: None,
            tablet: Tablet::new(cc.egui_ctx.clone()),
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
                self.page = Page::Canvas;
                self.return_page = Page::Canvas;
            }
            "Timeline" => {
                self.page = Page::Timeline;
                self.return_page = Page::Timeline;
            }
            "Settings" => {
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
            "Fit" => {
                self.canvas_zoom = 1.0;
                self.canvas_pan = Vec2::ZERO;
            }
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
            "Liquify" => {
                self.tool = if self.tool == Tool::Ink {
                    Tool::Liquify
                } else {
                    Tool::Ink
                };
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
                if self.drawing.is_none() {
                    self.record();
                }
                let width = self.doc.layers[self.layer].brush_width_mut(self.tool);
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
                        self.notify(format!("Already used by {action}"));
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
        } else {
            if self.time >= self.doc.timing.duration {
                self.time = 0.0;
            }
            match self.transport.play(self.time) {
                Ok(()) => self.play_origin = self.time,
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
                self.playback_time()
            } else {
                self.time
            })),
            ghosts,
            mirror: self.mirror,
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
                .powf(self.preferences.pressure_gamma)
                .max(self.preferences.minimum_pressure),
        }
    }
    fn start_stroke(&mut self, point: Point) {
        self.record();
        let liquify = self.tool == Tool::Liquify;
        let time = tick(self.time);
        let layer = self.layer;
        let width = self.doc.layers[layer].brush_width(self.tool);
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
            .add_filter("MP4 video", &["mp4"])
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
                Ok(path) => self.notify(format!("Video saved · {}", path.display())),
                Err(e) => self.fail(e),
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
            // Long curves edit their live suffix; incremental VBO contents must remain
            // identical to fresh geometry through batching, pressure, undo and branching.
            let mut curve_doc = Document::default();
            let first = Point {
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
            local_doc.settings.aspect = 2.0;
            let mut base = Cel::default();
            for x in [200.0, 800.0, 1400.0] {
                let mut stroke = Stroke::new(
                    18.0,
                    Point {
                        x,
                        y: 100.0,
                        pressure: 1.0,
                    },
                );
                stroke.points.push(Point {
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
                    x: 600.0,
                    y: 480.0,
                    pressure: 1.0,
                },
            );
            blur_stroke.points.push(Point {
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
                    x: 1300.0,
                    y: 650.0,
                    pressure: 1.0,
                },
            );
            second.points.push(Point {
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
        self.confirmation(ctx);
        if let Some(error) = self.error.take() {
            rfd::MessageDialog::new()
                .set_title("MORI")
                .set_level(rfd::MessageLevel::Error)
                .set_description(error)
                .show();
        }
        if self.page != Page::Canvas {
            self.feedback(ctx);
        }
        if let Err(error) = self.preference_store.save_if_changed(&self.preferences) {
            self.notify(error);
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
