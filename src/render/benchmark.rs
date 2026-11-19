//! Opt-in, real-context workload measurement. GPU query readback is confined to
//! this benchmark and never enters interactive drawing.
use super::*;
use crate::model::*;
use std::time::Instant;

fn point(x: f32, y: f32) -> Point {
    Point {
        x,
        y,
        pressure: 0.8,
        angle: 0.0,
    }
}
fn document(dense: bool) -> Document {
    let mut doc = Document::default();
    doc.settings.watermark = false;
    let (strokes, points) = if dense { (32, 2000) } else { (8, 400) };
    for i in 0..strokes {
        let mut stroke = Stroke::new(
            12.0,
            point(80.0, 90.0 + i as f32 * (576.0 / strokes as f32)),
        );
        for j in 1..points {
            stroke.points.push(point(
                80.0 + j as f32 * (1720.0 / points as f32),
                90.0 + i as f32 * (576.0 / strokes as f32) + (j as f32 * 0.06).sin() * 40.0,
            ));
        }
        doc.layers[0]
            .begin_edit(0, false)
            .strokes
            .push(Arc::new(stroke));
    }
    doc
}
pub unsafe fn run(gl: &glow::Context, size: [i32; 2]) -> serde_json::Value {
    let mut results = Vec::new();
    let names = [
        "blank",
        "hover",
        "large_round",
        "large_marker",
        "liquify",
        "liquify_dense",
        "move_prism",
        "blur",
        "blur_chromatic",
        "onion_six",
    ];
    for name in names {
        let mut renderer = Renderer::new(gl).unwrap();
        renderer.ensure_size(gl, size).unwrap();
        let output = renderer.new_raster_format(gl, glow::RGBA8).unwrap();
        let mut doc = if name == "blank" {
            Document::default()
        } else {
            document(name == "liquify_dense")
        };
        let mut index = SpatialIndex::new(
            doc.layers[0].held(0).map_or(&Cel::default(), |cel| cel),
            320.0,
        );
        if matches!(
            name,
            "large_round" | "large_marker" | "blur" | "blur_chromatic"
        ) {
            let mut stroke = Stroke::new(1200.0, point(300.0, 350.0));
            if name == "large_marker" {
                stroke.brush = Brush::Marker;
            }
            let layer = if name.starts_with("blur") { 3 } else { 1 };
            if name == "blur_chromatic" {
                doc.layers[layer].style = Style::Prism;
            }
            doc.layers[layer]
                .begin_edit(0, false)
                .strokes
                .push(Arc::new(stroke));
        }
        if name == "move_prism" {
            doc.layers[2].style = Style::Prism;
            let mut stroke = Stroke::new(240.0, point(200.0, 200.0));
            stroke.points.push(point(400.0, 300.0));
            doc.layers[2]
                .begin_edit(0, false)
                .strokes
                .push(Arc::new(stroke));
        }
        let ghosts: Vec<_> = if name == "onion_six" {
            (0..6)
                .map(|frame| {
                    let mut previous = doc.clone();
                    let cel = Arc::make_mut(previous.layers[0].cels.get_mut(&0).unwrap());
                    for stroke in &mut cel.strokes {
                        let stroke = Arc::make_mut(stroke);
                        for point in &mut stroke.points {
                            point.y += frame as f32 * 8.0;
                        }
                        stroke.invalidate_from(0);
                    }
                    Ghost {
                        scene: previous.scene(0),
                        color: [90, 140, 200],
                        opacity: 0.15,
                    }
                })
                .collect()
        } else {
            Vec::new()
        };
        let query = gl.create_query().unwrap();
        let mut cpu = Vec::new();
        let mut gpu = Vec::new();
        for step in 0..75 {
            let start = Instant::now();
            if matches!(
                name,
                "large_round" | "large_marker" | "blur" | "blur_chromatic"
            ) {
                let layer = if name.starts_with("blur") { 3 } else { 1 };
                let cel = Arc::make_mut(doc.layers[layer].cels.get_mut(&0).unwrap());
                let stroke = Arc::make_mut(cel.strokes.last_mut().unwrap());
                for j in 0..4 {
                    let phase = (step * 4 + j) as f32 * 0.02;
                    stroke
                        .points
                        .push(point(300.0 + phase * 210.0, 350.0 + phase.sin() * 150.0));
                }
            } else if name.starts_with("liquify") {
                let cel = Arc::make_mut(doc.layers[0].cels.get_mut(&0).unwrap());
                index.deform(
                    cel,
                    point(400.0 + step as f32 * 10.0, 350.0),
                    [8.0, 3.0],
                    320.0,
                    0.6,
                );
            } else if name == "move_prism" {
                let stroke = Arc::make_mut(
                    &mut Arc::make_mut(doc.layers[2].cels.get_mut(&0).unwrap()).strokes[0],
                );
                stroke.points[1].x += 5.0;
                stroke.invalidate_from(1);
            }
            let view = View {
                scene: doc.scene(0),
                ghosts: ghosts.clone(),
                mirror: false,
            };
            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(output.framebuffer));
            gl.begin_query(glow::TIME_ELAPSED, query);
            renderer.paint(
                gl,
                egui::PaintCallbackInfo {
                    viewport: egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(size[0] as f32, size[1] as f32),
                    ),
                    clip_rect: egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(size[0] as f32, size[1] as f32),
                    ),
                    pixels_per_point: 1.0,
                    screen_size_px: [size[0] as u32, size[1] as u32],
                },
                &view,
            );
            gl.end_query(glow::TIME_ELAPSED);
            let cpu_ms = start.elapsed().as_secs_f64() * 1000.0;
            let gpu_ms = f64::from(gl.get_query_parameter_u32(query, glow::QUERY_RESULT)) / 1e6;
            if step >= 15 {
                cpu.push(cpu_ms);
                gpu.push(gpu_ms);
            }
        }
        cpu.sort_by(f64::total_cmp);
        gpu.sort_by(f64::total_cmp);
        results.push(serde_json::json!({"case":name,"cpu_p95_ms":cpu[57],"cpu_max_ms":cpu[59],"gpu_p95_ms":gpu[57],"gpu_max_ms":gpu[59]}));
        gl.delete_query(query);
        Renderer::delete_raster(gl, output);
        renderer.destroy(gl);
    }
    serde_json::json!({"renderer":gl.get_parameter_string(glow::RENDERER),"pixels":size,"samples_per_case":60,"results":results})
}
