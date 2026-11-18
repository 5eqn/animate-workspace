use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
};

static NEXT_ID: AtomicU64 = AtomicU64::new(1);
pub fn next_revision() -> u64 {
    NEXT_ID.fetch_add(1, Ordering::Relaxed)
}
pub type Tick = i64;
pub fn tick(seconds: f64) -> Tick {
    (seconds * 1_000_000.0).round() as Tick
}
pub fn seconds(tick: Tick) -> f64 {
    tick as f64 / 1_000_000.0
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct Point {
    pub x: f32,
    pub y: f32,
    pub pressure: f32,
}
impl Point {
    pub fn distance(self, other: Self) -> f32 {
        (self.x - other.x).hypot(self.y - other.y)
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Stroke {
    pub id: u64,
    pub width: f32,
    pub points: Vec<Point>,
    #[serde(skip)]
    pub revision: u64,
}
impl Stroke {
    pub fn new(width: f32, point: Point) -> Self {
        Self {
            id: NEXT_ID.fetch_add(1, Ordering::Relaxed),
            width,
            points: vec![point],
            revision: 0,
        }
    }
    pub fn dab(&mut self, pressure: f32) -> bool {
        if let Some(point) = self.points.last_mut() {
            if pressure > point.pressure {
                point.pressure = pressure;
                self.revision = next_revision();
                return true;
            }
        }
        false
    }
    pub fn motion(&self) -> [f32; 2] {
        match (self.points.first(), self.points.last()) {
            (Some(a), Some(b)) => [b.x - a.x, b.y - a.y],
            _ => [0.0; 2],
        }
    }
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Cel {
    pub strokes: Vec<Arc<Stroke>>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Blend {
    Base,
    Xor,
    Darken,
    Lighten,
    Move,
    Blur,
}
impl Blend {
    pub const OVERLAYS: [Self; 5] = [
        Self::Xor,
        Self::Darken,
        Self::Lighten,
        Self::Move,
        Self::Blur,
    ];
    pub fn name(self) -> &'static str {
        match self {
            Self::Base => "Base",
            Self::Xor => "XOR",
            Self::Darken => "Darken",
            Self::Lighten => "Lighten",
            Self::Move => "Move",
            Self::Blur => "Motion blur",
        }
    }
    pub fn effect(self) -> bool {
        matches!(self, Self::Move | Self::Blur)
    }
    pub fn shader_id(self) -> i32 {
        match self {
            Self::Base => 0,
            Self::Xor => 1,
            Self::Darken => 2,
            Self::Lighten => 3,
            Self::Move => 4,
            Self::Blur => 5,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Style {
    Clean,
    Echo,
    Prism,
    Ribbon,
}
impl Style {
    pub const ALL: [Self; 4] = [Self::Clean, Self::Echo, Self::Prism, Self::Ribbon];
    pub fn name(self) -> &'static str {
        match self {
            Self::Clean => "Essential",
            Self::Echo => "Staccato echoes",
            Self::Prism => "Chromatic prism",
            Self::Ribbon => "Neon ribbons",
        }
    }
    pub fn id(self) -> i32 {
        match self {
            Self::Clean => 0,
            Self::Echo => 1,
            Self::Prism => 2,
            Self::Ribbon => 3,
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Layer {
    pub name: String,
    pub blend: Blend,
    pub style: Style,
    pub inverted: bool,
    pub visible: bool,
    pub width: f32,
    pub strength: f32,
    pub cels: BTreeMap<Tick, Arc<Cel>>,
}
impl Layer {
    pub fn new(name: &str, blend: Blend, width: f32) -> Self {
        Self {
            name: name.into(),
            blend,
            style: Style::Clean,
            inverted: false,
            visible: true,
            width,
            strength: 1.0,
            cels: BTreeMap::new(),
        }
    }
    pub fn held(&self, time: Tick) -> Option<&Arc<Cel>> {
        self.cels.range(..=time).next_back().map(|(_, c)| c)
    }
    pub fn begin_edit(&mut self, time: Tick, liquify: bool) -> &mut Cel {
        if !self.cels.contains_key(&time) {
            let cel = if liquify {
                self.held(time).cloned().unwrap_or_default()
            } else {
                Arc::default()
            };
            self.cels.insert(time, cel);
        }
        Arc::make_mut(self.cels.get_mut(&time).unwrap())
    }
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct Tempo {
    pub start: f64,
    pub bpm: f64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Timing {
    pub tempos: Vec<Tempo>,
    pub step: f64,
    pub duration: f64,
}
impl Default for Timing {
    fn default() -> Self {
        Self {
            tempos: vec![Tempo {
                start: 0.0,
                bpm: 120.0,
            }],
            step: 0.125,
            duration: 8.0,
        }
    }
}
impl Timing {
    pub fn segment(&self, time: f64) -> usize {
        self.tempos
            .partition_point(|p| p.start <= time + 0.0000001)
            .saturating_sub(1)
    }
    pub fn step_at(&self, index: usize) -> f64 {
        60.0 / self.tempos[index].bpm * self.step
    }
    pub fn normalize(&mut self) {
        self.tempos.sort_by(|a, b| a.start.total_cmp(&b.start));
        self.tempos
            .dedup_by(|a, b| (a.start - b.start).abs() < 0.000001);
    }
    pub fn next(&self, time: f64) -> f64 {
        let i = self.segment(time);
        let anchor = self.tempos[i].start;
        let step = self.step_at(i);
        let next = anchor + (((time - anchor) / step + 0.00001).floor() + 1.0) * step;
        self.tempos.get(i + 1).map_or(next, |t| next.min(t.start))
    }
    pub fn previous(&self, time: f64) -> f64 {
        if time <= 0.000001 {
            return 0.0;
        }
        let i = self.segment(time - 0.000001);
        let anchor = self.tempos[i].start;
        let step = self.step_at(i);
        (anchor + (((time - anchor) / step - 0.00001).ceil() - 1.0) * step).max(0.0)
    }
    pub fn at_or_before(&self, time: f64) -> f64 {
        let time = time.max(0.0);
        let i = self.segment(time);
        let anchor = self.tempos[i].start;
        let step = self.step_at(i);
        anchor + ((time - anchor) / step + 1e-9).floor() * step
    }
    pub fn snap(&self, time: f64) -> f64 {
        let time = time.max(0.0);
        let i = self.segment(time);
        let anchor = self.tempos[i].start;
        let step = self.step_at(i);
        let lower = anchor + ((time - anchor) / step).floor() * step;
        let upper = self
            .tempos
            .get(i + 1)
            .map_or(lower + step, |t| (lower + step).min(t.start));
        if time - lower <= upper - time {
            lower
        } else {
            upper
        }
    }
    /// Display only lines separated by at least four pixels; snapping always uses the full lattice.
    pub fn display_grid(&self, start: f64, end: f64, pixels: f32) -> Vec<f64> {
        let mut result = Vec::new();
        if end <= start || pixels <= 0.0 {
            return result;
        }
        let pixels_per_second = pixels as f64 / (end - start);
        for (i, tempo) in self.tempos.iter().enumerate() {
            let lo = start.max(tempo.start);
            let hi = end.min(self.tempos.get(i + 1).map_or(end, |t| t.start));
            if hi < lo {
                continue;
            }
            let step = self.step_at(i);
            let stride = (4.0 / (pixels_per_second * step)).ceil().max(1.0) as u64;
            let stride = stride.next_power_of_two() as f64;
            let interval = step * stride;
            let first = ((lo - tempo.start) / interval).ceil();
            let last = ((hi - tempo.start) / interval).floor();
            for index in first as i64..=last as i64 {
                result.push(tempo.start + index as f64 * interval);
            }
            if tempo.start >= start && tempo.start <= end {
                result.push(tempo.start);
            }
        }
        result.sort_by(f64::total_cmp);
        result.dedup_by(|a, b| (*a - *b).abs() < 1e-8);
        result
    }
    #[cfg(test)]
    pub fn grid(&self, start: f64, end: f64) -> Vec<f64> {
        let mut t = self.snap(start).max(0.0);
        let mut grid = Vec::new();
        while t <= end + 0.000001 {
            grid.push(t);
            let next = self.next(t);
            if next <= t {
                break;
            }
            t = next;
        }
        grid
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Interpolation {
    Linear,
    Smooth,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Onion {
    pub enabled: bool,
    pub previous: usize,
    pub next: usize,
    pub opacity: f32,
    pub before: [u8; 3],
    pub after: [u8; 3],
    pub falloff: f32,
}
impl Default for Onion {
    fn default() -> Self {
        Self {
            enabled: true,
            previous: 1,
            next: 1,
            opacity: 0.22,
            before: [90, 130, 180],
            after: [209, 123, 94],
            falloff: 0.65,
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Binding {
    pub action: String,
    pub key: String,
    #[serde(default)]
    pub control: bool,
    pub command: bool,
    pub alt: bool,
    pub shift: bool,
}
impl Binding {
    pub fn label(&self) -> String {
        format!(
            "{}{}{}{}{}",
            if self.control { "Control " } else { "" },
            if self.command {
                if cfg!(target_os = "macos") {
                    "⌘ "
                } else {
                    "Ctrl "
                }
            } else {
                ""
            },
            if self.alt {
                if cfg!(target_os = "macos") {
                    "Option "
                } else {
                    "Alt "
                }
            } else {
                ""
            },
            if self.shift { "Shift " } else { "" },
            self.key
        )
    }
}
pub fn default_bindings() -> Vec<Binding> {
    [
        ("Canvas", "1", false, true, false),
        ("Timeline", "2", false, true, false),
        ("Settings", ",", true, false, false),
        ("Save", "S", true, false, false),
        ("Open", "O", true, false, false),
        ("New", "N", true, false, false),
        ("Undo", "Z", false, false, false),
        ("Redo", "Y", false, false, false),
        ("Liquify", "L", false, false, false),
        ("Mirror", "M", false, false, false),
        ("Previous", "A", false, false, false),
        ("Next", "D", false, false, false),
        ("Layer up", "W", false, false, false),
        ("Layer down", "S", false, false, false),
        ("Smaller", "[", false, false, false),
        ("Larger", "]", false, false, false),
        ("Play", "Space", false, false, false),
        ("Onion skin", "O", false, false, false),
        ("Fullscreen", "F", false, false, false),
    ]
    .into_iter()
    .map(|(a, k, c, o, s)| Binding {
        action: a.into(),
        key: k.into(),
        control: false,
        command: c,
        alt: o,
        shift: s,
    })
    .collect()
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ExportSettings {
    pub width: u32,
    pub fps: u32,
    pub start: f64,
    pub end: f64,
    pub crf: u32,
    pub audio: bool,
}
impl Default for ExportSettings {
    fn default() -> Self {
        Self {
            width: 1920,
            fps: 60,
            start: 0.0,
            end: 8.0,
            crf: 18,
            audio: true,
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Settings {
    pub aspect: f32,
    pub onion: Onion,
    pub pressure_gamma: f32,
    pub minimum_pressure: f32,
    pub spacing: f32,
    pub interpolation: Interpolation,
    pub liquify_strength: f32,
    pub bindings: Vec<Binding>,
    pub export: ExportSettings,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            aspect: 2.39,
            onion: Onion::default(),
            pressure_gamma: 1.0,
            minimum_pressure: 0.05,
            spacing: 1.5,
            interpolation: Interpolation::Smooth,
            liquify_strength: 0.65,
            bindings: default_bindings(),
            export: ExportSettings::default(),
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AudioAsset {
    pub name: String,
    #[serde(skip)]
    pub bytes: Arc<Vec<u8>>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Document {
    pub version: u32,
    pub name: String,
    pub layers: Vec<Layer>,
    pub timing: Timing,
    pub settings: Settings,
    pub audio: Option<AudioAsset>,
}
impl Default for Document {
    fn default() -> Self {
        Self {
            version: 1,
            name: "Untitled motion".into(),
            layers: vec![
                Layer::new("Paper", Blend::Base, 6.0),
                Layer::new("Ink", Blend::Xor, 42.0),
                Layer::new("Movement", Blend::Move, 100.0),
                Layer::new("Atmosphere", Blend::Blur, 140.0),
            ],
            timing: Timing::default(),
            settings: Settings::default(),
            audio: None,
        }
    }
}
impl Document {
    pub fn validate(&self) -> Result<(), String> {
        if self.version != 1 {
            return Err("Unsupported project version".into());
        }
        if self.layers.is_empty()
            || self.layers[0].blend != Blend::Base
            || self.layers.iter().skip(1).any(|l| l.blend == Blend::Base)
        {
            return Err("Invalid base layer".into());
        }
        if !self.settings.aspect.is_finite() || self.settings.aspect <= 0.0 {
            return Err("Invalid canvas ratio".into());
        }
        if self.timing.tempos.is_empty()
            || self.timing.tempos[0].start != 0.0
            || self.timing.tempos.iter().any(|t| {
                !t.start.is_finite() || t.start < 0.0 || !t.bpm.is_finite() || t.bpm <= 0.0
            })
            || self
                .timing
                .tempos
                .windows(2)
                .any(|t| t[0].start >= t[1].start)
            || !self.timing.step.is_finite()
            || self.timing.step <= 0.0
            || !self.timing.duration.is_finite()
            || self.timing.duration <= 0.0
        {
            return Err("Invalid musical timing".into());
        }
        let s = &self.settings;
        if !s.pressure_gamma.is_finite()
            || s.pressure_gamma <= 0.0
            || !s.minimum_pressure.is_finite()
            || !(0.0..=1.0).contains(&s.minimum_pressure)
            || !s.spacing.is_finite()
            || s.spacing <= 0.0
            || !s.liquify_strength.is_finite()
        {
            return Err("Invalid brush settings".into());
        }
        if s.export.fps == 0
            || s.export.width < 2
            || !s.export.start.is_finite()
            || !s.export.end.is_finite()
            || s.export.start < 0.0
            || s.export.end <= s.export.start
        {
            return Err("Invalid export range".into());
        }
        for l in &self.layers {
            if !l.width.is_finite() || l.width <= 0.0 || !l.strength.is_finite() {
                return Err("Invalid layer settings".into());
            }
            for c in l.cels.values() {
                for stroke in &c.strokes {
                    if !stroke.width.is_finite()
                        || stroke.width <= 0.0
                        || stroke.points.is_empty()
                        || stroke.points.iter().any(|p| {
                            !p.x.is_finite()
                                || !p.y.is_finite()
                                || !p.pressure.is_finite()
                                || p.pressure < 0.0
                                || p.pressure > 1.0
                        })
                    {
                        return Err("Invalid vector geometry".into());
                    }
                    NEXT_ID.fetch_max(stroke.id.saturating_add(1), Ordering::Relaxed);
                }
            }
        }
        Ok(())
    }
    pub fn scene(&self, time: Tick) -> Scene {
        Scene {
            aspect: self.settings.aspect,
            layers: self
                .layers
                .iter()
                .filter(|l| l.visible)
                .map(|l| SceneLayer {
                    blend: l.blend,
                    style: l.style,
                    inverted: l.inverted,
                    strength: l.strength,
                    cel: l.held(time).cloned().unwrap_or_default(),
                })
                .collect(),
        }
    }
}
#[derive(Clone)]
pub struct Scene {
    pub aspect: f32,
    pub layers: Vec<SceneLayer>,
}
#[derive(Clone)]
pub struct SceneLayer {
    pub blend: Blend,
    pub style: Style,
    pub inverted: bool,
    pub strength: f32,
    pub cel: Arc<Cel>,
}
#[derive(Default)]
pub struct History {
    past: Vec<Document>,
    future: Vec<Document>,
}
impl History {
    pub fn record(&mut self, doc: &Document) {
        self.past.push(doc.clone());
        self.future.clear();
    }
    pub fn undo(&mut self, doc: &mut Document) -> bool {
        if let Some(prev) = self.past.pop() {
            self.future.push(std::mem::replace(doc, prev));
            true
        } else {
            false
        }
    }
    pub fn redo(&mut self, doc: &mut Document) -> bool {
        if let Some(next) = self.future.pop() {
            self.past.push(std::mem::replace(doc, next));
            true
        } else {
            false
        }
    }
    pub fn counts(&self) -> (usize, usize) {
        (self.past.len(), self.future.len())
    }
}

/// A dynamic point grid; liquify visits only cells under the brush, and never changes pressure.
pub struct SpatialIndex {
    cells: std::collections::HashMap<(i32, i32), Vec<(usize, usize)>>,
    cell: f32,
}
impl SpatialIndex {
    pub fn new(cel: &Cel, radius: f32) -> Self {
        let mut result = Self {
            cells: Default::default(),
            cell: radius.max(16.0),
        };
        for (s, stroke) in cel.strokes.iter().enumerate() {
            for (p, point) in stroke.points.iter().enumerate() {
                result
                    .cells
                    .entry(result.key(*point))
                    .or_default()
                    .push((s, p));
            }
        }
        result
    }
    fn key(&self, p: Point) -> (i32, i32) {
        (
            (p.x / self.cell).floor() as i32,
            (p.y / self.cell).floor() as i32,
        )
    }
    pub fn deform(
        &mut self,
        cel: &mut Cel,
        center: Point,
        delta: [f32; 2],
        radius: f32,
        strength: f32,
    ) {
        let lo = self.key(Point {
            x: center.x - radius,
            y: center.y - radius,
            pressure: 1.0,
        });
        let hi = self.key(Point {
            x: center.x + radius,
            y: center.y + radius,
            pressure: 1.0,
        });
        let mut candidates = Vec::new();
        for x in lo.0..=hi.0 {
            for y in lo.1..=hi.1 {
                if let Some(points) = self.cells.remove(&(x, y)) {
                    candidates.extend(points);
                }
            }
        }
        let mut touched = std::collections::HashSet::new();
        for (s, p) in candidates {
            let old = cel.strokes[s].points[p];
            let distance = old.distance(center);
            let new = if distance < radius {
                let t = 1.0 - distance / radius;
                let weight = t * t * (3.0 - 2.0 * t) * strength;
                let stroke = Arc::make_mut(&mut cel.strokes[s]);
                stroke.points[p].x += delta[0] * weight;
                stroke.points[p].y += delta[1] * weight;
                touched.insert(s);
                stroke.points[p]
            } else {
                old
            };
            self.cells.entry(self.key(new)).or_default().push((s, p));
        }
        for s in touched {
            Arc::make_mut(&mut cel.strokes[s]).revision = next_revision();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn p(x: f32, y: f32) -> Point {
        Point {
            x,
            y,
            pressure: 0.7,
        }
    }
    #[test]
    fn variable_tempo_grid_crosses_anchors_both_ways() {
        let t = Timing {
            tempos: vec![
                Tempo {
                    start: 0.0,
                    bpm: 120.0,
                },
                Tempo {
                    start: 1.03,
                    bpm: 90.0,
                },
            ],
            step: 0.125,
            duration: 8.0,
        };
        assert!((t.next(1.0) - 1.03).abs() < 1e-9);
        assert!((t.previous(1.03) - 1.0).abs() < 1e-9);
        assert!((t.next(1.03) - 1.1133333333).abs() < 1e-8);
        assert!((t.snap(1.027) - 1.03).abs() < 1e-9);
        for time in t.grid(0.0, 3.0).windows(2) {
            assert!(time[1] > time[0]);
            assert!((t.previous(time[1]) - time[0]).abs() < 1e-6);
        }
    }
    #[test]
    fn playback_never_shows_the_next_exposure_before_its_musical_onset() {
        let timing = Timing::default();
        assert_eq!(timing.at_or_before(0.249), 0.1875);
        assert_eq!(timing.at_or_before(0.25), 0.25);
    }
    #[test]
    fn long_music_grid_stays_bounded_by_display_width() {
        let timing = Timing {
            duration: 86400.0,
            ..Default::default()
        };
        let lines = timing.display_grid(0.0, 86400.0, 1200.0);
        assert!(lines.len() < 310);
        assert_eq!(timing.next(0.0), 0.0625);
        assert_eq!(timing.snap(0.06), 0.0625);
    }
    #[test]
    fn first_brush_clears_but_first_liquify_copies_held_cel() {
        let mut layer = Layer::new("Ink", Blend::Xor, 20.0);
        layer
            .begin_edit(0, false)
            .strokes
            .push(Arc::new(Stroke::new(20.0, p(10.0, 10.0))));
        assert_eq!(layer.held(10).unwrap().strokes.len(), 1);
        let mut copied = layer.clone();
        copied.begin_edit(10, true);
        assert_eq!(copied.cels[&10].strokes.len(), 1);
        layer.begin_edit(10, false);
        assert!(layer.cels[&10].strokes.is_empty());
        assert_eq!(layer.cels[&0].strokes.len(), 1);
    }
    #[test]
    fn liquify_keeps_pressure_and_previous_frame_immutable() {
        let mut layer = Layer::new("Ink", Blend::Xor, 20.0);
        layer
            .begin_edit(0, false)
            .strokes
            .push(Arc::new(Stroke::new(20.0, p(10.0, 10.0))));
        let cel = layer.begin_edit(10, true);
        let mut index = SpatialIndex::new(cel, 50.0);
        index.deform(cel, p(10.0, 10.0), [20.0, 0.0], 50.0, 1.0);
        assert_eq!(cel.strokes[0].points[0].x, 30.0);
        assert_eq!(cel.strokes[0].points[0].pressure, 0.7);
        assert_eq!(layer.cels[&0].strokes[0].points[0].x, 10.0);
    }
    #[test]
    fn stationary_pressure_expands_ink_and_never_erases_it() {
        let mut stroke = Stroke::new(
            100.0,
            Point {
                x: 10.0,
                y: 10.0,
                pressure: 0.05,
            },
        );
        for pressure in [0.1, 0.2, 0.4, 0.7, 1.0] {
            assert!(stroke.dab(pressure));
            assert_eq!(stroke.points.len(), 1);
            assert_eq!(stroke.points[0].pressure, pressure);
        }
        assert!(!stroke.dab(0.1));
        assert_eq!(stroke.points[0].pressure, 1.0);
    }
    #[test]
    fn history_is_branching_and_has_no_fixed_limit() {
        let mut history = History::default();
        let mut doc = Document::default();
        for _ in 0..1500 {
            history.record(&doc);
            doc.layers[0].width += 1.0;
        }
        for _ in 0..1500 {
            assert!(history.undo(&mut doc));
        }
        assert_eq!(doc.layers[0].width, 6.0);
        assert!(history.redo(&mut doc));
        history.record(&doc);
        assert!(!history.redo(&mut doc));
    }
}
