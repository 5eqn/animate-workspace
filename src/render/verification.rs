//! Real-context regressions for retained drawing and display work.
use super::*;

impl Renderer {
    pub unsafe fn verify_retained_rasters(&mut self, gl: &glow::Context) -> String {
        use crate::{curve::StrokeSmoother, model::*};
        let size = [960, 402];
        let point = |x, y, pressure, angle| Point {
            x,
            y,
            pressure,
            angle,
        };
        let compare = |renderer: &mut Self, doc: &Document| {
            let actual = renderer.read_frame(gl, &doc.scene(0), size).unwrap();
            let mut fresh = Self::new(gl).unwrap();
            let expected = fresh.read_frame(gl, &doc.scene(0), size).unwrap();
            fresh.destroy(gl);
            let mismatch = actual.iter().zip(&expected).filter(|(a, b)| a != b).count();
            assert!(
                mismatch == 0,
                "Retained raster differs from full rendering in {mismatch} channels"
            );
        };
        for brush in [Brush::Ink, Brush::Marker] {
            let mut doc = Document::default();
            doc.layers.truncate(1);
            doc.settings.watermark = false;
            let start = point(200.0, 300.0, 0.1, 0.0);
            let mut stroke = Stroke::new(240.0, start);
            stroke.brush = brush;
            doc.layers[0]
                .begin_edit(0, false)
                .strokes
                .push(Arc::new(stroke));
            let mut smoother = StrokeSmoother::new(start);
            compare(self, &doc);
            for i in 1..=16 {
                let cel = Arc::make_mut(doc.layers[0].cels.get_mut(&0).unwrap());
                let stroke = Arc::make_mut(&mut cel.strokes[0]);
                smoother.push(
                    stroke,
                    point(
                        200.0 + i as f32 * 75.0,
                        300.0 + (i as f32 * 0.7).sin() * 80.0,
                        0.15 + (i as f32 * 0.5).sin().abs() * 0.85,
                        i as f32 * 0.08,
                    ),
                    1.5,
                );
                compare(self, &doc);
            }
            let cel = Arc::make_mut(doc.layers[0].cels.get_mut(&0).unwrap());
            let mut spatial = SpatialIndex::new(cel, 180.0);
            spatial.deform(
                cel,
                point(800.0, 300.0, 1.0, 0.0),
                [25.0, -15.0],
                180.0,
                0.8,
            );
            compare(self, &doc);
            let saved = doc.clone();
            let cel = Arc::make_mut(doc.layers[0].cels.get_mut(&0).unwrap());
            let stroke = Arc::make_mut(&mut cel.strokes[0]);
            stroke.points.truncate(stroke.points.len() / 2);
            stroke.invalidate_from(0);
            compare(self, &doc);
            compare(self, &saved);
            Arc::make_mut(doc.layers[0].cels.get_mut(&0).unwrap())
                .strokes
                .clear();
            compare(self, &doc);
            let mut fill = Stroke::new(42.0, start);
            fill.brush = Brush::Fill;
            Arc::make_mut(doc.layers[0].cels.get_mut(&0).unwrap())
                .strokes
                .push(Arc::new(fill));
            compare(self, &doc);
        }
        let mut doc = Document::default();
        doc.layers.truncate(1);
        doc.settings.watermark = false;
        for frame in 0..7 {
            let mut cel = Cel::default();
            for row in 0..6 {
                let mut stroke =
                    Stroke::new(400.0, point(200.0, 150.0 + row as f32 * 80.0, 1.0, 0.0));
                stroke.points = (0..1001)
                    .map(|i| {
                        point(
                            200.0 + i as f32 * 1.3,
                            150.0 + row as f32 * 80.0 + frame as f32,
                            1.0,
                            0.0,
                        )
                    })
                    .collect();
                cel.strokes.push(Arc::new(stroke));
            }
            doc.layers[0]
                .cels
                .insert(tick(frame as f64 * 0.25), Arc::new(cel));
        }
        let mut view = View {
            scene: doc.scene(tick(0.75)),
            ghosts: (0..7)
                .filter(|i| *i != 3)
                .map(|i| Ghost {
                    scene: doc.scene(tick(i as f64 * 0.25)),
                    color: [180, 120, 100],
                    opacity: 0.15,
                })
                .collect(),
            mirror: false,
        };
        let framebuffer = gl.get_parameter_framebuffer(glow::FRAMEBUFFER_BINDING);
        let destination = self.new_raster(gl).unwrap();
        let paint = |renderer: &mut Self, view: &View| {
            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(destination.framebuffer));
            let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(960.0, 402.0));
            renderer.paint(
                gl,
                egui::PaintCallbackInfo {
                    viewport: rect,
                    clip_rect: rect,
                    pixels_per_point: 1.0,
                    screen_size_px: [960, 402],
                },
                view,
            );
        };
        paint(self, &view);
        gl.finish();
        // The retained onion image must match the former direct-to-window
        // sequence, including its 8-bit blending and ordering.
        let read_target = |target: Target| {
            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(target.framebuffer));
            let mut pixels = vec![0; 960 * 402 * 4];
            gl.read_pixels(
                0,
                0,
                960,
                402,
                glow::RGBA,
                glow::UNSIGNED_BYTE,
                glow::PixelPackData::Slice(Some(&mut pixels)),
            );
            pixels
        };
        let composed = read_target(destination);
        let direct = self.new_raster_format(gl, glow::RGBA8).unwrap();
        gl.disable(glow::SCISSOR_TEST);
        gl.disable(glow::BLEND);
        gl.viewport(0, 0, 960, 402);
        gl.clear_color(1.0, 1.0, 1.0, 1.0);
        gl.clear(glow::COLOR_BUFFER_BIT);
        for ghost in &view.ghosts {
            let image = self.scene(gl, &ghost.scene, size).unwrap();
            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(direct.framebuffer));
            gl.enable(glow::BLEND);
            gl.blend_func(glow::SRC_ALPHA, glow::ONE_MINUS_SRC_ALPHA);
            self.show(gl, image.texture, false, Some((ghost.color, ghost.opacity)));
        }
        let image = self.scene(gl, &view.scene, size).unwrap();
        gl.bind_framebuffer(glow::FRAMEBUFFER, Some(direct.framebuffer));
        gl.blend_func(glow::DST_COLOR, glow::ZERO);
        self.show(gl, image.texture, false, None);
        let expected = read_target(direct);
        // The canvas is opaque. Its visible RGB must match; the direct
        // window blend's unused intermediate alpha is not carried to display.
        assert!(
            composed
                .as_chunks::<4>()
                .0
                .iter()
                .zip(expected.as_chunks::<4>().0)
                .all(|(a, b)| a[..3] == b[..3]),
            "Cached onion colors differ from direct composition"
        );
        Self::delete_raster(gl, direct);
        gl.bind_framebuffer(glow::FRAMEBUFFER, Some(destination.framebuffer));
        let warm = self.work;
        let start = std::time::Instant::now();
        for _ in 0..30 {
            paint(self, &view);
        }
        gl.finish();
        let milliseconds = start.elapsed().as_secs_f64() * 1000.0 / 30.0;
        assert_eq!(
            self.work, warm,
            "Cursor-only frames redrew vector geometry or recomposited onions"
        );
        let stroke = Arc::make_mut(
            &mut Arc::make_mut(doc.layers[0].cels.get_mut(&tick(0.75)).unwrap()).strokes[5],
        );
        stroke.points.push(point(1502.0, 550.0, 1.0, 0.0));
        stroke.invalidate_from(999);
        view.scene = doc.scene(tick(0.75));
        paint(self, &view);
        gl.finish();
        let changed = self.work;
        assert_eq!(
            changed.scene_builds - warm.scene_builds,
            1,
            "Drawing rebuilt an unchanged onion frame"
        );
        assert_eq!(changed.view_builds - warm.view_builds, 1);
        assert!(
            changed.raster_pixels - warm.raster_pixels < (size[0] * size[1]) as u64,
            "A local tip update repainted the full canvas"
        );
        assert_eq!(self.scene_rasters.len(), 7);
        view.ghosts.clear();
        paint(self, &view);
        assert_eq!(
            self.scene_rasters.len(),
            1,
            "Off-screen onion caches were retained"
        );
        assert!(self.composed_view.is_none());
        Self::delete_raster(gl, destination);
        gl.bind_framebuffer(glow::FRAMEBUFFER, framebuffer);
        format!("Retained rendering: round/marker pixels match fresh renders through pressure, tilt, erasing and undo; six onions redraw zero strokes when hovering ({milliseconds:.2} ms/view on this GPU); tip damage {} pixels, one changed scene",changed.raster_pixels-warm.raster_pixels)
    }
}
