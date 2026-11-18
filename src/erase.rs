//! Whole-stroke picking against the rendered geometry. A gesture builds its
//! spatial tree once; subsequent pointer sweeps visit only overlapping nodes.
use crate::{
    model::{Cel, Point},
    render,
};
use std::collections::HashSet;

type XY = [f32; 2];
#[derive(Clone, Copy)]
struct Bounds {
    min: XY,
    max: XY,
}
impl Bounds {
    fn points(points: &[XY]) -> Self {
        let mut bounds = Self {
            min: [f32::INFINITY; 2],
            max: [f32::NEG_INFINITY; 2],
        };
        for p in points {
            for (axis, value) in p.iter().enumerate() {
                bounds.min[axis] = bounds.min[axis].min(*value);
                bounds.max[axis] = bounds.max[axis].max(*value);
            }
        }
        bounds
    }
    fn union(self, other: Self) -> Self {
        Self {
            min: [self.min[0].min(other.min[0]), self.min[1].min(other.min[1])],
            max: [self.max[0].max(other.max[0]), self.max[1].max(other.max[1])],
        }
    }
    fn overlaps(self, other: Self) -> bool {
        (0..2).all(|i| self.min[i] <= other.max[i] && other.min[i] <= self.max[i])
    }
}
struct Triangle {
    vertices: [XY; 3],
    bounds: Bounds,
    stroke: u64,
}
struct Node {
    bounds: Bounds,
    kind: NodeKind,
}
enum NodeKind {
    Leaf(Vec<Triangle>),
    Branch(Box<Node>, Box<Node>),
}
impl Node {
    fn build(mut triangles: Vec<Triangle>) -> Option<Self> {
        let bounds = triangles.iter().map(|t| t.bounds).reduce(Bounds::union)?;
        let kind = if triangles.len() <= 12 {
            NodeKind::Leaf(triangles)
        } else {
            let axis = usize::from(bounds.max[1] - bounds.min[1] > bounds.max[0] - bounds.min[0]);
            let mid = triangles.len() / 2;
            triangles.select_nth_unstable_by(mid, |a, b| {
                (a.bounds.min[axis] + a.bounds.max[axis])
                    .total_cmp(&(b.bounds.min[axis] + b.bounds.max[axis]))
            });
            let right = triangles.split_off(mid);
            NodeKind::Branch(
                Box::new(Self::build(triangles).unwrap()),
                Box::new(Self::build(right).unwrap()),
            )
        };
        Some(Self { bounds, kind })
    }
    fn hits(&self, sweep: &Sweep, erased: &HashSet<u64>, hits: &mut HashSet<u64>) {
        if !self.bounds.overlaps(sweep.bounds) {
            return;
        }
        match &self.kind {
            NodeKind::Branch(left, right) => {
                left.hits(sweep, erased, hits);
                right.hits(sweep, erased, hits);
            }
            NodeKind::Leaf(triangles) => {
                for t in triangles {
                    if !erased.contains(&t.stroke)
                        && !hits.contains(&t.stroke)
                        && t.bounds.overlaps(sweep.bounds)
                        && sweep.hits(t.vertices)
                    {
                        hits.insert(t.stroke);
                    }
                }
            }
        }
    }
}
pub struct EraserIndex {
    root: Option<Node>,
    erased: HashSet<u64>,
}
impl EraserIndex {
    pub fn new(cel: &Cel) -> Self {
        let mut triangles = Vec::new();
        for stroke in &cel.strokes {
            for t in render::hit_geometry(stroke).as_chunks::<12>().0 {
                let vertices = [[t[0], t[1]], [t[4], t[5]], [t[8], t[9]]];
                if cross(vertices[0], vertices[1], vertices[2]).abs() > f32::EPSILON {
                    triangles.push(Triangle {
                        vertices,
                        bounds: Bounds::points(&vertices),
                        stroke: stroke.id,
                    });
                }
            }
        }
        Self {
            root: Node::build(triangles),
            erased: HashSet::new(),
        }
    }
    pub fn sweep(&mut self, from: Point, to: Point, radius: f32) -> HashSet<u64> {
        let sweep = Sweep::new(from, to, radius);
        let mut hits = HashSet::new();
        if sweep.radii[0].max(sweep.radii[1]) > 0.0 {
            if let Some(root) = &self.root {
                root.hits(&sweep, &self.erased, &mut hits);
            }
        }
        self.erased.extend(&hits);
        hits
    }
}
/// Convex hull of the two pressure-sized endpoint disks. Its tangent sides
/// preserve the changing footprint instead of widening the entire fast sweep
/// to its largest radius.
struct Sweep {
    ends: [XY; 2],
    radii: [f32; 2],
    quad: Option<[XY; 4]>,
    bounds: Bounds,
}
impl Sweep {
    fn new(from: Point, to: Point, radius: f32) -> Self {
        let a = [from.x, from.y];
        let b = [to.x, to.y];
        let ra = radius * from.pressure;
        let rb = radius * to.pressure;
        let bounds = Bounds {
            min: [a[0] - ra, a[1] - ra],
            max: [a[0] + ra, a[1] + ra],
        }
        .union(Bounds {
            min: [b[0] - rb, b[1] - rb],
            max: [b[0] + rb, b[1] + rb],
        });
        let length = from.distance(to);
        let quad = if length > (rb - ra).abs() {
            let u = [(b[0] - a[0]) / length, (b[1] - a[1]) / length];
            let along = (ra - rb) / length;
            let across = (1.0 - along * along).sqrt();
            let n = [u[0] * along - u[1] * across, u[1] * along + u[0] * across];
            let m = [u[0] * along + u[1] * across, u[1] * along - u[0] * across];
            Some([
                [a[0] + n[0] * ra, a[1] + n[1] * ra],
                [b[0] + n[0] * rb, b[1] + n[1] * rb],
                [b[0] + m[0] * rb, b[1] + m[1] * rb],
                [a[0] + m[0] * ra, a[1] + m[1] * ra],
            ])
        } else {
            None
        };
        Self {
            ends: [a, b],
            radii: [ra, rb],
            quad,
            bounds,
        }
    }
    fn hits(&self, triangle: [XY; 3]) -> bool {
        if self.radii[0] == self.radii[1] {
            return swept_hit(self.ends[0], self.ends[1], self.radii[0], triangle);
        }
        if self
            .ends
            .iter()
            .zip(self.radii)
            .any(|(&p, r)| swept_hit(p, p, r, triangle))
        {
            return true;
        }
        if let Some(q) = self.quad {
            // Vertex containment and edge crossings cover convex-polygon intersection.
            return q.iter().any(|&p| inside(p, triangle))
                || triangle
                    .iter()
                    .any(|&p| inside(p, [q[0], q[1], q[2]]) || inside(p, [q[0], q[2], q[3]]))
                || [(q[0], q[1]), (q[1], q[2]), (q[2], q[3]), (q[3], q[0])]
                    .into_iter()
                    .any(|(a, b)| swept_hit(a, b, 0.0, triangle));
        }
        false
    }
}
fn cross(a: XY, b: XY, c: XY) -> f32 {
    (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])
}
fn distance_sq(p: XY, a: XY, b: XY) -> f32 {
    let d = [b[0] - a[0], b[1] - a[1]];
    let length = d[0] * d[0] + d[1] * d[1];
    let t = if length > 0.0 {
        ((p[0] - a[0]) * d[0] + (p[1] - a[1]) * d[1]) / length
    } else {
        0.0
    }
    .clamp(0.0, 1.0);
    (p[0] - a[0] - t * d[0]).powi(2) + (p[1] - a[1] - t * d[1]).powi(2)
}
fn inside(p: XY, t: [XY; 3]) -> bool {
    if cross(t[0], t[1], t[2]).abs() <= f32::EPSILON {
        return false;
    }
    let signs = [
        cross(t[0], t[1], p),
        cross(t[1], t[2], p),
        cross(t[2], t[0], p),
    ];
    signs.iter().all(|s| *s >= 0.0) || signs.iter().all(|s| *s <= 0.0)
}
fn swept_hit(a: XY, b: XY, radius: f32, t: [XY; 3]) -> bool {
    if inside(a, t) || inside(b, t) {
        return true;
    }
    for (c, d) in [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])] {
        let ca = cross(a, b, c);
        let da = cross(a, b, d);
        let ac = cross(c, d, a);
        let bc = cross(c, d, b);
        if ((ca < 0.0 && da > 0.0) || (ca > 0.0 && da < 0.0))
            && ((ac < 0.0 && bc > 0.0) || (ac > 0.0 && bc < 0.0))
        {
            return true;
        }
        if distance_sq(a, c, d)
            .min(distance_sq(b, c, d))
            .min(distance_sq(c, a, b))
            .min(distance_sq(d, a, b))
            <= radius * radius
        {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{tick, Brush, Document, History, Stroke};
    use std::sync::Arc;
    fn p(x: f32, y: f32) -> Point {
        Point {
            x,
            y,
            pressure: 1.0,
            angle: 0.0,
        }
    }
    #[test]
    fn fast_pressure_changes_do_not_erase_beside_the_light_end() {
        let cel = Cel {
            strokes: vec![
                Arc::new(Stroke::new(2.0, p(10.0, 8.0))),
                Arc::new(Stroke::new(2.0, p(90.0, 8.0))),
            ],
        };
        let mut from = p(0.0, 0.0);
        from.pressure = 0.1;
        let mut index = EraserIndex::new(&cel);
        let hits = index.sweep(from, p(100.0, 0.0), 10.0);
        assert!(!hits.contains(&cel.strokes[0].id));
        assert!(hits.contains(&cel.strokes[1].id));
    }
    #[test]
    fn fast_sweeps_hit_complete_strokes_including_dabs_and_caps() {
        let mut stroke = Stroke::new(10.0, p(20.0, 0.0));
        stroke.points.push(p(20.0, 100.0));
        let dab = Stroke::new(8.0, p(60.0, 50.0));
        let untouched = Stroke::new(10.0, p(200.0, 200.0));
        let cel = Cel {
            strokes: vec![Arc::new(stroke), Arc::new(dab), Arc::new(untouched)],
        };
        let mut index = EraserIndex::new(&cel);
        let hits = index.sweep(p(0.0, 50.0), p(100.0, 50.0), 1.0);
        assert_eq!(hits.len(), 2);
        assert!(hits.contains(&cel.strokes[0].id) && hits.contains(&cel.strokes[1].id));
        assert!(index.sweep(p(0.0, 50.0), p(100.0, 50.0), 1.0).is_empty());
        let mut index = EraserIndex::new(&cel);
        assert!(index
            .sweep(p(20.0, 105.5), p(20.0, 105.5), 1.0)
            .contains(&cel.strokes[0].id));
        assert!(index
            .sweep(p(180.0, 180.0), p(180.0, 180.0), 1.0)
            .is_empty());
    }
    #[test]
    fn picks_actual_marker_rectangle_and_pressure_edges() {
        let mut marker = Stroke::new(40.0, p(0.0, 0.0));
        marker.brush = Brush::Marker;
        let mut cel = Cel {
            strokes: vec![Arc::new(marker)],
        };
        let mut index = EraserIndex::new(&cel);
        assert!(index.sweep(p(0.0, 10.0), p(0.0, 10.0), 1.0).is_empty());
        assert_eq!(index.sweep(p(19.0, 0.0), p(19.0, 0.0), 1.0).len(), 1);
        Arc::make_mut(&mut cel.strokes[0]).points[0].angle = std::f32::consts::FRAC_PI_2;
        let mut index = EraserIndex::new(&cel);
        assert!(index.sweep(p(19.0, 0.0), p(19.0, 0.0), 1.0).is_empty());
        assert_eq!(index.sweep(p(0.0, 19.0), p(0.0, 19.0), 1.0).len(), 1);
        let mut round = Stroke::new(40.0, p(100.0, 0.0));
        round.points[0].pressure = 0.1;
        let cel = Cel {
            strokes: vec![Arc::new(round)],
        };
        let mut index = EraserIndex::new(&cel);
        assert!(index.sweep(p(100.0, 10.0), p(100.0, 10.0), 1.0).is_empty());
        assert_eq!(index.sweep(p(100.0, 2.5), p(100.0, 2.5), 1.0).len(), 1);
    }
    #[test]
    fn erasing_held_content_preserves_previous_time_other_layers_and_undo() {
        let mut doc = Document::default();
        let first = Arc::new(Stroke::new(10.0, p(30.0, 30.0)));
        let second = Arc::new(Stroke::new(10.0, p(100.0, 30.0)));
        doc.layers[0].begin_edit(0, false).strokes = vec![first.clone(), second.clone()];
        doc.layers[1].begin_edit(0, false).strokes = vec![first.clone()];
        let mut index = EraserIndex::new(doc.layers[0].held(tick(1.0)).unwrap());
        let hits = index.sweep(p(30.0, 30.0), p(30.0, 30.0), 2.0);
        let mut history = History::default();
        history.record(&doc);
        doc.layers[0]
            .begin_edit(tick(1.0), true)
            .strokes
            .retain(|s| !hits.contains(&s.id));
        assert_eq!(doc.layers[0].held(0).unwrap().strokes.len(), 2);
        assert_eq!(
            doc.layers[0].held(tick(1.0)).unwrap().strokes[0].id,
            second.id
        );
        assert_eq!(doc.layers[1].held(tick(1.0)).unwrap().strokes.len(), 1);
        assert!(history.undo(&mut doc));
        assert!(!doc.layers[0].cels.contains_key(&tick(1.0)));
        assert!(history.redo(&mut doc));
        assert_eq!(doc.layers[0].held(tick(1.0)).unwrap().strokes.len(), 1);
    }
}
