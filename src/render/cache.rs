//! Raster identities and conservative damage tracking. Keys contain no vector
//! ownership: retaining a GPU image must not force a copy of a live stroke.
use crate::model::{marker_corners, Brush, Cel, Point, Scene, Stroke};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct StrokeKey {
    pub id: u64,
    pub revision: u64,
    pub len: usize,
    width: u32,
    brush: u8,
    first: [u32; 4],
    last: [u32; 4],
}
impl StrokeKey {
    pub fn new(stroke: &Stroke) -> Self {
        let point = |p: Option<&Point>| {
            p.map_or([0; 4], |p| {
                [
                    p.x.to_bits(),
                    p.y.to_bits(),
                    p.pressure.to_bits(),
                    p.angle.to_bits(),
                ]
            })
        };
        Self {
            id: stroke.id,
            revision: stroke.revision,
            len: stroke.points.len(),
            width: stroke.width.to_bits(),
            brush: stroke.brush as u8,
            first: point(stroke.points.first()),
            last: point(stroke.points.last()),
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
struct LayerKey {
    blend: i32,
    style: i32,
    inverted: bool,
    strength: u32,
    strokes: Vec<StrokeKey>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct SceneKey {
    aspect: u32,
    rhythm: Option<(u32, u32, u32)>,
    layers: Vec<LayerKey>,
}
impl SceneKey {
    pub fn new(scene: &Scene) -> Self {
        Self {
            aspect: scene.aspect.to_bits(),
            rhythm: scene.rhythm.map(|r| (r.rows, r.cols, r.filled)),
            layers: scene
                .layers
                .iter()
                .map(|l| LayerKey {
                    blend: l.blend.shader_id(),
                    style: l.style.id(),
                    inverted: l.inverted,
                    strength: l.strength.to_bits(),
                    strokes: l.cel.strokes.iter().map(|s| StrokeKey::new(s)).collect(),
                })
                .collect(),
        }
    }
}
#[derive(Clone, Copy, Debug)]
pub(super) struct Bounds {
    pub min: [f32; 2],
    pub max: [f32; 2],
}
impl Bounds {
    pub fn empty() -> Self {
        Self {
            min: [f32::INFINITY; 2],
            max: [f32::NEG_INFINITY; 2],
        }
    }
    pub fn canvas(aspect: f32) -> Self {
        Self {
            min: [0.0; 2],
            max: [1920.0, 1920.0 / aspect],
        }
    }
    fn union(self, other: Self) -> Self {
        Self {
            min: std::array::from_fn(|i| self.min[i].min(other.min[i])),
            max: std::array::from_fn(|i| self.max[i].max(other.max[i])),
        }
    }
    pub fn intersects(self, other: Self) -> bool {
        (0..2).all(|i| self.min[i] <= other.max[i] && other.min[i] <= self.max[i])
    }
    pub fn pixel_region(rect: [i32; 4], size: [i32; 2], aspect: f32) -> Self {
        let [x, y, w, h] = rect;
        let sx = 1920.0 / size[0] as f32;
        let sy = (1920.0 / aspect) / size[1] as f32;
        Self {
            min: [x as f32 * sx, (size[1] - y - h) as f32 * sy],
            max: [(x + w) as f32 * sx, (size[1] - y) as f32 * sy],
        }
    }
    pub fn pixels(self, size: [i32; 2], aspect: f32) -> Option<[i32; 4]> {
        let scale = [size[0] as f32 / 1920.0, size[1] as f32 / (1920.0 / aspect)];
        let lo: [i32; 2] = std::array::from_fn(|i| {
            ((self.min[i] * scale[i]).floor() as i32)
                .saturating_sub(2)
                .clamp(0, size[i])
        });
        let hi: [i32; 2] = std::array::from_fn(|i| {
            ((self.max[i] * scale[i]).ceil() as i32)
                .saturating_add(2)
                .clamp(0, size[i])
        });
        (hi[0] > lo[0] && hi[1] > lo[1]).then_some([
            lo[0],
            size[1] - hi[1],
            hi[0] - lo[0],
            hi[1] - lo[1],
        ])
    }
}
fn point_bounds(stroke: &Stroke, p: Point) -> Bounds {
    if stroke.brush == Brush::Marker {
        marker_corners(p, stroke.width)
            .into_iter()
            .fold(Bounds::empty(), |bounds, p| {
                bounds.union(Bounds { min: p, max: p })
            })
    } else {
        let r = stroke.width * p.pressure * 0.5;
        Bounds {
            min: [p.x - r, p.y - r],
            max: [p.x + r, p.y + r],
        }
    }
}
pub(super) struct CachedStroke {
    pub key: StrokeKey,
    segments: Vec<(Bounds, Bounds)>,
    pub bounds: Bounds,
}
impl CachedStroke {
    fn update(old: Option<Self>, stroke: &Stroke, aspect: f32) -> (Self, Bounds) {
        let key = StrokeKey::new(stroke);
        let mut dirty = Bounds::empty();
        let mut segments = Vec::new();
        if let Some(mut old) = old {
            let compatible =
                old.key.id == key.id && old.key.brush == key.brush && old.key.width == key.width;
            let from = if compatible && old.key.revision != key.revision {
                stroke
                    .changed_since(old.key.revision)
                    .min(old.segments.len())
                    .saturating_sub(2)
            } else if compatible && old.key.first == key.first && old.key.len < key.len {
                old.segments.len().saturating_sub(2)
            } else {
                0
            };
            for &(bounds, _) in &old.segments[from..] {
                dirty = dirty.union(bounds);
            }
            old.segments.truncate(from);
            segments = old.segments;
        }
        if stroke.brush == Brush::Fill {
            dirty = Bounds::canvas(aspect);
            segments = vec![(dirty, dirty)];
        } else {
            for i in segments.len()..stroke.points.len() {
                let mut bounds = point_bounds(stroke, stroke.points[i]);
                if i > 0 {
                    bounds = bounds.union(point_bounds(stroke, stroke.points[i - 1]));
                }
                let total = segments
                    .last()
                    .map_or(bounds, |(_, previous): &(Bounds, Bounds)| {
                        previous.union(bounds)
                    });
                segments.push((bounds, total));
                dirty = dirty.union(bounds);
            }
        }
        let bounds = segments.last().map_or(Bounds::empty(), |(_, total)| *total);
        (
            Self {
                key,
                segments,
                bounds,
            },
            dirty,
        )
    }
}
pub(super) struct RasterState {
    pub strokes: Vec<CachedStroke>,
    pub flow: bool,
    pub initialized: bool,
    aspect: u32,
}
impl RasterState {
    pub fn new(flow: bool) -> Self {
        Self {
            strokes: Vec::new(),
            flow,
            initialized: false,
            aspect: 0,
        }
    }
    pub fn update(&mut self, cel: &Cel, flow: bool, aspect: f32) -> Option<Bounds> {
        let mut dirty = Bounds::empty();
        let aspect_changed = self.aspect != aspect.to_bits();
        let full = !self.initialized || self.flow != flow || aspect_changed;
        let fill = cel.strokes.iter().any(|s| s.brush == Brush::Fill)
            || self
                .strokes
                .iter()
                .any(|s| s.key.brush == Brush::Fill as u8);
        let mut changed = full;
        let mut previous = std::mem::take(&mut self.strokes).into_iter();
        for stroke in &cel.strokes {
            let old = previous.next();
            if old.as_ref().is_some_and(|old| {
                old.key == StrokeKey::new(stroke)
                    && !(aspect_changed && stroke.brush == Brush::Fill)
            }) {
                self.strokes.push(old.unwrap());
            } else {
                let (new, damage) = CachedStroke::update(old, stroke, aspect);
                self.strokes.push(new);
                dirty = dirty.union(damage);
                changed = true;
            }
        }
        for old in previous {
            dirty = dirty.union(old.bounds);
            changed = true;
        }
        if full || fill {
            dirty = Bounds::canvas(aspect);
        }
        self.flow = flow;
        self.aspect = aspect.to_bits();
        self.initialized = true;
        changed.then_some(dirty)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    fn point(x: f32, y: f32) -> Point {
        Point {
            x,
            y,
            pressure: 1.0,
            angle: 0.0,
        }
    }
    #[test]
    fn linear_append_without_a_revision_still_only_dirties_the_tail() {
        let mut stroke = Stroke::new(20.0, point(100.0, 200.0));
        stroke.points = (0..1000).map(|i| point(100.0 + i as f32, 200.0)).collect();
        let mut cel = Cel {
            strokes: vec![Arc::new(stroke)],
        };
        let mut state = RasterState::new(false);
        state.update(&cel, false, 2.0);
        Arc::make_mut(&mut cel.strokes[0])
            .points
            .push(point(1100.0, 201.0));
        assert!(state.update(&cel, false, 2.0).unwrap().min[0] > 1080.0);
    }
    #[test]
    fn aspect_changes_repaint_even_when_pixel_dimensions_are_unchanged() {
        let cel = Cel {
            strokes: vec![Arc::new(Stroke::new(40.0, point(100.0, 200.0)))],
        };
        let mut state = RasterState::new(false);
        state.update(&cel, false, 2.0);
        assert_eq!(
            state
                .update(&cel, false, 2.39)
                .unwrap()
                .pixels([960, 402], 2.39),
            Some([0, 0, 960, 402])
        );
        assert!(state.update(&cel, false, 2.39).is_none());
    }
    #[test]
    fn stable_content_has_no_damage_and_an_append_only_touches_the_tip() {
        let mut stroke = Stroke::new(20.0, point(100.0, 200.0));
        stroke.points = (0..1000).map(|i| point(100.0 + i as f32, 200.0)).collect();
        let mut cel = Cel {
            strokes: vec![Arc::new(stroke)],
        };
        let mut state = RasterState::new(false);
        assert!(state.update(&cel, false, 2.0).is_some());
        assert!(state.update(&cel, false, 2.0).is_none());
        let stroke = Arc::make_mut(&mut cel.strokes[0]);
        stroke.points.push(point(1100.0, 201.0));
        stroke.invalidate_from(999);
        let damage = state.update(&cel, false, 2.0).unwrap();
        assert!(damage.min[0] > 1080.0 && damage.max[0] >= 1110.0);
    }
    #[test]
    fn pressure_shrink_and_erasing_include_the_old_covered_pixels() {
        let mut cel = Cel {
            strokes: vec![Arc::new(Stroke::new(400.0, point(500.0, 400.0)))],
        };
        let mut state = RasterState::new(false);
        state.update(&cel, false, 2.0);
        let stroke = Arc::make_mut(&mut cel.strokes[0]);
        stroke.points[0].pressure = 0.1;
        stroke.invalidate_from(0);
        let damage = state.update(&cel, false, 2.0).unwrap();
        assert!(damage.min[0] <= 300.0 && damage.max[0] >= 700.0);
        cel.strokes.clear();
        let damage = state.update(&cel, false, 2.0).unwrap();
        assert!(damage.min[0] <= 480.0 && damage.max[0] >= 520.0);
    }
    #[test]
    fn fill_and_switching_to_a_blur_field_invalidate_the_whole_mask() {
        let mut fill = Stroke::new(42.0, point(20.0, 20.0));
        fill.brush = Brush::Fill;
        let mut cel = Cel::default();
        let mut state = RasterState::new(false);
        state.update(&cel, false, 2.0);
        cel.strokes.push(Arc::new(fill));
        let damage = state.update(&cel, false, 2.0).unwrap();
        assert_eq!(damage.pixels([1920, 960], 2.0), Some([0, 0, 1920, 960]));
        assert!(state.update(&cel, false, 2.0).is_none());
        assert_eq!(
            state
                .update(&cel, true, 2.0)
                .unwrap()
                .pixels([1920, 960], 2.0),
            Some([0, 0, 1920, 960])
        );
    }
}
