//! Direct vector transforms. A drag always reads its initial vectors, so
//! motion events never build a transform chain or compound rounding errors.
use crate::model::{Brush, Cel, Point};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Similarity {
    pub linear: [f64; 2],
    pub translation: [f64; 2],
}
impl Similarity {
    pub const IDENTITY: Self = Self {
        linear: [1.0, 0.0],
        translation: [0.0; 2],
    };
    pub fn moving(origin: Point, target: Point) -> Self {
        Self {
            translation: [
                f64::from(target.x) - f64::from(origin.x),
                f64::from(target.y) - f64::from(origin.y),
            ],
            ..Self::IDENTITY
        }
    }
    pub fn around(center: Point, grab: Point, target: Point) -> Option<Self> {
        if grab.x == target.x && grab.y == target.y {
            return Some(Self::IDENTITY);
        }
        let a = f64::from(grab.x) - f64::from(center.x);
        let b = f64::from(grab.y) - f64::from(center.y);
        let x = f64::from(target.x) - f64::from(center.x);
        let y = f64::from(target.y) - f64::from(center.y);
        let denominator = a * a + b * b;
        if denominator == 0.0 {
            return None;
        }
        let real = (x * a + y * b) / denominator;
        let imaginary = (y * a - x * b) / denominator;
        Some(Self {
            linear: [real, imaginary],
            translation: [
                f64::from(center.x) * (1.0 - real) + imaginary * f64::from(center.y),
                f64::from(center.y) * (1.0 - real) - imaginary * f64::from(center.x),
            ],
        })
    }
    #[cfg(test)]
    pub fn point(self, p: Point) -> Point {
        self.point_with_rotation(p, self.linear[1].atan2(self.linear[0]))
    }
    fn point_with_rotation(self, mut p: Point, rotation: f64) -> Point {
        let [a, b] = self.linear;
        let (x, y) = (f64::from(p.x), f64::from(p.y));
        p.x = (a * x - b * y + self.translation[0]) as f32;
        p.y = (b * x + a * y + self.translation[1]) as f32;
        p.angle = (f64::from(p.angle) + rotation) as f32;
        p
    }
    /// Reuse the current point storage; change the same geometry revision used
    /// by liquify. The source is held only for this gesture, not in the project.
    pub fn apply(self, source: &Cel, current: &mut Cel) {
        let scale = self.linear[0].hypot(self.linear[1]);
        let rotation = self.linear[1].atan2(self.linear[0]);
        for (original, stroke) in source.strokes.iter().zip(&mut current.strokes) {
            let stroke = std::sync::Arc::make_mut(stroke);
            stroke.width = (f64::from(original.width) * scale) as f32;
            for (point, original) in stroke.points.iter_mut().zip(&original.points) {
                *point = self.point_with_rotation(*original, rotation);
            }
            stroke.invalidate_all();
        }
    }
}
#[derive(Clone)]
pub enum Gesture {
    Center,
    Move {
        source: Cel,
        origin: Point,
    },
    Scale {
        source: Cel,
        center: Point,
        grab: Point,
    },
}
impl Gesture {
    pub fn mapping(&self, target: Point) -> Option<(&Cel, Similarity)> {
        match self {
            Self::Center => None,
            Self::Move { source, origin } => Some((source, Similarity::moving(*origin, target))),
            Self::Scale {
                source,
                center,
                grab,
            } => Similarity::around(*center, *grab, target).map(|mapping| (source, mapping)),
        }
    }
}
#[derive(Default)]
pub struct ScaleTool {
    pub center: Option<Point>,
    owner: Option<(usize, Brush)>,
}
impl ScaleTool {
    pub fn sync(&mut self, layer: usize, brush: Brush) {
        let owner = Some((layer, brush));
        if self.owner != owner {
            self.center = None;
            self.owner = owner;
        }
    }
    pub fn reset(&mut self) {
        self.center = None;
        self.owner = None;
    }
    pub fn begin(&self, grab: Point, source: Cel) -> Gesture {
        match self.center {
            None => Gesture::Center,
            Some(center) => Gesture::Scale {
                source,
                center,
                grab,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Stroke;
    use std::sync::Arc;
    fn point(x: f32, y: f32) -> Point {
        Point {
            x,
            y,
            pressure: 0.6,
            angle: 0.0,
        }
    }
    fn drawing() -> Cel {
        let mut stroke = Stroke::new(20.0, point(2.0, 0.0));
        stroke.brush = Brush::Marker;
        stroke.points.push(point(4.0, 1.0));
        Cel {
            strokes: vec![Arc::new(stroke)],
        }
    }
    #[test]
    fn scaling_rotates_the_grab_to_the_cursor_and_scales_width_not_pressure() {
        let source = drawing();
        let mut current = source.clone();
        let center = point(0.0, 0.0);
        let grab = point(2.0, 0.0);
        let target = point(0.0, 4.0);
        let mapping = Similarity::around(center, grab, target).unwrap();
        mapping.apply(&source, &mut current);
        assert_eq!(
            (mapping.point(center).x, mapping.point(center).y),
            (center.x, center.y)
        );
        assert_eq!(current.strokes[0].points[0].x, target.x);
        assert_eq!(current.strokes[0].points[0].y, target.y);
        assert_eq!(current.strokes[0].width, 40.0);
        assert_eq!(current.strokes[0].points[0].pressure, 0.6);
        assert!((current.strokes[0].points[0].angle - std::f32::consts::FRAC_PI_2).abs() < 1e-6);
        assert_eq!(source.strokes[0].points[0], grab);
    }
    #[test]
    fn repeated_preview_uses_original_vectors_including_return_from_zero_scale() {
        let source = drawing();
        let mut current = source.clone();
        let center = point(0.0, 0.0);
        let grab = point(2.0, 0.0);
        for i in 0..1000 {
            let target = point(i as f32 * 0.03, (i as f32 * 0.1).sin());
            Similarity::around(center, grab, target)
                .unwrap()
                .apply(&source, &mut current);
        }
        Similarity::around(center, grab, center)
            .unwrap()
            .apply(&source, &mut current);
        assert_eq!(current.strokes[0].width, 0.0);
        Similarity::around(center, grab, grab)
            .unwrap()
            .apply(&source, &mut current);
        assert_eq!(current.strokes[0].width, source.strokes[0].width);
        assert_eq!(current.strokes[0].points, source.strokes[0].points);
        assert!(Similarity::around(center, center, grab).is_none());
    }
    #[test]
    fn changing_tool_or_layer_clears_the_center_even_on_return() {
        let mut tool = ScaleTool::default();
        tool.sync(0, Brush::Scale);
        tool.center = Some(point(100.0, 200.0));
        tool.sync(0, Brush::Scale);
        assert!(tool.center.is_some());
        tool.sync(1, Brush::Scale);
        tool.sync(0, Brush::Scale);
        assert!(tool.center.is_none());
        tool.center = Some(point(100.0, 200.0));
        tool.sync(0, Brush::Move);
        tool.sync(0, Brush::Scale);
        assert!(tool.center.is_none());
        assert!(matches!(
            tool.begin(point(200.0, 300.0), drawing()),
            Gesture::Center
        ));
    }
}
