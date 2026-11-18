//! Causal, interpolating stroke curves. The newest span reaches the pen immediately;
//! one future input finalizes its tangent without revisiting the completed prefix.
use crate::model::{interpolate_angle, Point, Stroke};

#[derive(Clone, Copy, Debug)]
struct V {
    x: f32,
    y: f32,
}
impl From<Point> for V {
    fn from(p: Point) -> Self {
        Self { x: p.x, y: p.y }
    }
}
impl V {
    fn add(self, b: Self) -> Self {
        Self {
            x: self.x + b.x,
            y: self.y + b.y,
        }
    }
    fn sub(self, b: Self) -> Self {
        Self {
            x: self.x - b.x,
            y: self.y - b.y,
        }
    }
    fn scale(self, s: f32) -> Self {
        Self {
            x: self.x * s,
            y: self.y * s,
        }
    }
    fn length(self) -> f32 {
        self.x.hypot(self.y)
    }
    fn midpoint(self, b: Self) -> Self {
        self.add(b).scale(0.5)
    }
}
#[derive(Clone, Copy)]
struct Cubic([V; 4]);
impl Cubic {
    fn between(before: Point, start: Point, end: Point, after: Point) -> Self {
        let p0 = V::from(before);
        let p1 = V::from(start);
        let p2 = V::from(end);
        let p3 = V::from(after);
        // Centripetal Catmull–Rom: derivatives share the same knot parameterization
        // on both sides, including when adjacent input distances differ sharply.
        let a = p1.sub(p0).length().sqrt();
        let b = p2.sub(p1).length().sqrt();
        let c = p3.sub(p2).length().sqrt();
        let m1 = p1
            .sub(p0)
            .scale(1.0 / a)
            .sub(p2.sub(p0).scale(1.0 / (a + b)))
            .add(p2.sub(p1).scale(1.0 / b))
            .scale(b);
        let m2 = p2
            .sub(p1)
            .scale(1.0 / b)
            .sub(p3.sub(p1).scale(1.0 / (b + c)))
            .add(p3.sub(p2).scale(1.0 / c))
            .scale(b);
        Self([
            p1,
            p1.add(m1.scale(1.0 / 3.0)),
            p2.sub(m2.scale(1.0 / 3.0)),
            p2,
        ])
    }
    fn start_direction(a: Point, b: Point, c: Point) -> V {
        let ab = V::from(b).sub(V::from(a));
        let bc = V::from(c).sub(V::from(b));
        let u = ab.length().sqrt();
        let v = bc.length().sqrt();
        ab.sub(
            bc.scale(1.0 / v)
                .sub(ab.scale(1.0 / u))
                .scale(u * u / (u + v)),
        )
    }
    fn with_start(mut self, a: Point, b: Point, c: Point) -> Self {
        self.0[1] = self.0[0].add(Self::start_direction(a, b, c).scale(1.0 / 3.0));
        self
    }
    fn with_end(mut self, a: Point, b: Point, c: Point) -> Self {
        self.0[2] = self.0[3].add(Self::start_direction(c, b, a).scale(1.0 / 3.0));
        self
    }
    fn split(self) -> (Self, Self) {
        let [a, b, c, d] = self.0;
        let ab = a.midpoint(b);
        let bc = b.midpoint(c);
        let cd = c.midpoint(d);
        let abc = ab.midpoint(bc);
        let bcd = bc.midpoint(cd);
        let middle = abc.midpoint(bcd);
        (Self([a, ab, abc, middle]), Self([middle, bcd, cd, d]))
    }
    fn flatness(self) -> f32 {
        let [a, b, c, d] = self.0;
        let chord = d.sub(a);
        let length = chord.length();
        if length == 0.0 {
            return b.sub(a).length().max(c.sub(a).length());
        }
        let distance = |p: V| {
            let v = p.sub(a);
            (chord.x * v.y - chord.y * v.x).abs() / length
        };
        distance(b).max(distance(c))
    }
    fn sample(self, start: Point, end: Point, spacing: f32, out: &mut Vec<Point>) {
        let tolerance = spacing.min(1.0) * 0.15;
        let mut pending = vec![(self, 0.0_f32, 1.0_f32)];
        while let Some((curve, t0, t1)) = pending.pop() {
            let [a, b, c, d] = curve.0;
            let length = b.sub(a).length() + c.sub(b).length() + d.sub(c).length();
            let middle = (t0 + t1) * 0.5;
            if (length <= spacing && curve.flatness() <= tolerance) || middle == t0 || middle == t1
            {
                out.push(Point {
                    angle: interpolate_angle(start.angle, end.angle, t1),
                    x: d.x,
                    y: d.y,
                    pressure: if t1 == 1.0 {
                        end.pressure
                    } else {
                        start.pressure + (end.pressure - start.pressure) * t1
                    },
                });
            } else {
                let (left, right) = curve.split();
                pending.push((right, middle, t1));
                pending.push((left, t0, middle));
            }
        }
    }
}
fn reflected(origin: Point, toward: Point) -> Point {
    Point {
        angle: origin.angle,
        x: 2.0 * origin.x - toward.x,
        y: 2.0 * origin.y - toward.y,
        pressure: origin.pressure,
    }
}

#[derive(Clone)]
pub struct StrokeSmoother {
    controls: Vec<Point>,
    stable_len: usize,
}
impl StrokeSmoother {
    pub fn new(first: Point) -> Self {
        Self {
            controls: vec![first],
            stable_len: 1,
        }
    }
    pub fn stationary_angle(&mut self, angle: f32) {
        self.controls.last_mut().unwrap().angle = angle;
    }
    pub fn stationary_pressure(&mut self, pressure: f32) {
        let last = self.controls.last_mut().unwrap();
        last.pressure = last.pressure.max(pressure);
    }
    pub fn push(&mut self, stroke: &mut Stroke, point: Point, spacing: f32) {
        if self.controls.last().unwrap().distance(point) < 0.001 {
            stroke.dab(point.pressure);
            self.stationary_pressure(point.pressure);
            return;
        }
        self.controls.push(point);
        let dirty_from = self.stable_len.saturating_sub(1);
        stroke.points.truncate(self.stable_len);
        let n = self.controls.len();
        if n >= 3 {
            let start = self.controls[n - 3];
            let end = self.controls[n - 2];
            let after = self.controls[n - 1];
            let before = if n == 3 {
                reflected(start, end)
            } else {
                self.controls[n - 4]
            };
            let curve = Cubic::between(before, start, end, after);
            let curve = if n == 3 {
                curve.with_start(start, end, after)
            } else {
                curve
            };
            curve.sample(start, end, spacing, &mut stroke.points);
            self.stable_len = stroke.points.len();
        }
        let start = self.controls[n - 2];
        let end = self.controls[n - 1];
        let before = if n == 2 {
            reflected(start, end)
        } else {
            self.controls[n - 3]
        };
        let curve = Cubic::between(before, start, end, reflected(end, start));
        let curve = if n >= 3 {
            curve.with_end(before, start, end)
        } else {
            curve
        };
        curve.sample(start, end, spacing, &mut stroke.points);
        if self.controls.len() > 3 {
            self.controls.remove(0);
        }
        stroke.invalidate_from(dirty_from);
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn p(x: f32, y: f32, pressure: f32) -> Point {
        Point {
            angle: 0.0,
            x,
            y,
            pressure,
        }
    }
    fn tangent(a: V, b: V) -> V {
        let d = b.sub(a);
        d.scale(1.0 / d.length())
    }
    #[test]
    fn neighboring_spans_have_the_same_tangent_with_sparse_uneven_inputs() {
        let inputs = [
            p(0.0, 0.0, 0.2),
            p(3.0, 8.0, 0.3),
            p(280.0, 110.0, 0.7),
            p(390.0, -80.0, 0.9),
            p(395.0, -82.0, 0.1),
        ];
        let left = Cubic::between(inputs[0], inputs[1], inputs[2], inputs[3]);
        let right = Cubic::between(inputs[1], inputs[2], inputs[3], inputs[4]);
        let a = tangent(left.0[2], left.0[3]);
        let b = tangent(right.0[0], right.0[1]);
        assert!(
            a.sub(b).length() < 0.00001,
            "Curve has a corner at an input knot"
        );
    }
    #[test]
    fn rapid_circle_is_curved_and_passes_through_original_pressure_knots() {
        let inputs: Vec<_> = (0..=12)
            .map(|i| {
                let a = i as f32 / 12.0 * std::f32::consts::TAU;
                p(
                    500.0 + a.cos() * 240.0,
                    500.0 + a.sin() * 240.0,
                    0.2 + i as f32 / 20.0,
                )
            })
            .collect();
        let mut stroke = Stroke::new(18.0, inputs[0]);
        let mut smooth = StrokeSmoother::new(inputs[0]);
        for &point in &inputs[1..] {
            smooth.push(&mut stroke, point, 1.5);
            assert_eq!(
                *stroke.points.last().unwrap(),
                point,
                "Preview lags behind the pen"
            );
        }
        for point in &inputs {
            assert!(
                stroke.points.contains(point),
                "Measured input/pressure knot was lost"
            );
        }
        let max_error = stroke
            .points
            .iter()
            .map(|point| ((point.x - 500.0).hypot(point.y - 500.0) - 240.0).abs())
            .fold(0.0_f32, f32::max);
        assert!(
            max_error < 3.0,
            "Sparse curve became a polygon: radial error {max_error}"
        );
        assert!(stroke
            .points
            .windows(2)
            .all(|p| p[0].distance(p[1]) <= 1.501));
        let mut max_turn = 0.0_f32;
        for points in stroke.points.windows(3) {
            let a = V::from(points[1]).sub(V::from(points[0]));
            let b = V::from(points[2]).sub(V::from(points[1]));
            if a.length() > 0.001 && b.length() > 0.001 {
                let angle = (a.x * b.y - a.y * b.x).atan2(a.x * b.x + a.y * b.y).abs();
                max_turn = max_turn.max(angle);
            }
        }
        assert!(
            max_turn < 0.04,
            "Adjacent segments still have visible corners: {max_turn}"
        );
    }
    #[test]
    fn completed_prefix_is_immutable_and_work_stays_local_to_the_tip() {
        let first = p(0.0, 0.0, 0.2);
        let mut stroke = Stroke::new(42.0, first);
        let mut smooth = StrokeSmoother::new(first);
        for i in 1..1000 {
            let stable = smooth.stable_len;
            let prefix = stroke.points[..stable].to_vec();
            let revision = stroke.revision;
            let point = p(i as f32 * 12.0, (i as f32 * 0.1).sin() * 80.0, 0.3);
            smooth.push(&mut stroke, point, 1.5);
            assert_eq!(&stroke.points[..stable], prefix.as_slice());
            assert_eq!(stroke.changed_since(revision), stable.saturating_sub(1));
            assert!(
                stroke.points.len() - stable < 100,
                "Resampling grows with the length of the completed stroke"
            );
            assert!(smooth.controls.len() <= 3);
        }
    }
    #[test]
    fn stationary_dab_and_release_endpoint_are_preserved() {
        let first = p(10.0, 10.0, 0.1);
        let mut stroke = Stroke::new(100.0, first);
        let mut smooth = StrokeSmoother::new(first);
        for pressure in [0.2, 0.5, 0.9] {
            assert!(stroke.dab(pressure));
            smooth.stationary_pressure(pressure);
        }
        assert_eq!(stroke.points.len(), 1);
        let end = p(110.0, 50.0, 0.2);
        smooth.push(&mut stroke, end, 1.5);
        assert_eq!(stroke.points[0].pressure, 0.9);
        assert_eq!(*stroke.points.last().unwrap(), end);
        assert!(stroke
            .points
            .iter()
            .all(|p| p.pressure >= 0.2 && p.pressure <= 0.9));
    }
}

#[cfg(test)]
mod angle_tests {
    use super::*;
    #[test]
    fn smooth_marker_interpolates_the_shortest_nib_orientation_without_losing_pressure() {
        let first = Point {
            x: 0.0,
            y: 0.0,
            pressure: 0.2,
            angle: 3.13,
        };
        let end = Point {
            x: 100.0,
            y: 50.0,
            pressure: 0.9,
            angle: 0.01,
        };
        let mut stroke = Stroke::new(100.0, first);
        stroke.brush = crate::model::Brush::Marker;
        let mut smooth = StrokeSmoother::new(first);
        smooth.push(&mut stroke, end, 1.5);
        assert_eq!(*stroke.points.last().unwrap(), end);
        assert!(stroke.points[1..stroke.points.len() - 1]
            .iter()
            .all(|p| p.angle > 3.12 && p.pressure >= 0.2 && p.pressure <= 0.9));
    }
}
