use crate::model::{Scene, Stroke};
use eframe::{
    egui,
    glow::{self, HasContext},
};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

struct Target {
    texture: glow::Texture,
    framebuffer: glow::Framebuffer,
}
struct Mesh {
    buffer: glow::Buffer,
    count: usize,
    body_count: usize,
    capacity: usize,
    points: usize,
    revision: u64,
    point_offsets: Vec<usize>,
    last_used: u64,
}
pub struct Renderer {
    ink: glow::Program,
    composite: glow::Program,
    display: glow::Program,
    vao: glow::VertexArray,
    targets: Vec<Target>,
    msaa: Option<(glow::Framebuffer, glow::Renderbuffer)>,
    size: [i32; 2],
    meshes: HashMap<u64, Mesh>,
    frame: u64,
}
#[derive(Clone)]
pub struct Ghost {
    pub scene: Scene,
    pub color: [u8; 3],
    pub opacity: f32,
}
#[derive(Clone)]
pub struct View {
    pub scene: Scene,
    pub ghosts: Vec<Ghost>,
    pub mirror: bool,
}
fn bytes(values: &[f32]) -> &[u8] {
    unsafe { std::slice::from_raw_parts(values.as_ptr().cast(), std::mem::size_of_val(values)) }
}
fn vertex(out: &mut Vec<f32>, x: f32, y: f32, flow: [f32; 2]) {
    out.extend_from_slice(&[x, y, flow[0], flow[1]]);
}
fn direction(stroke: &Stroke, index: usize) -> [f32; 2] {
    let p = stroke.points[index];
    let (a, b) = if index > 0 {
        (stroke.points[index - 1], p)
    } else {
        (p, *stroke.points.get(1).unwrap_or(&p))
    };
    let distance = a.distance(b);
    let radius = stroke.width * p.pressure * 0.5;
    if distance > 0.00001 {
        [
            (b.x - a.x) / distance * radius,
            (b.y - a.y) / distance * radius,
        ]
    } else {
        [radius, 0.0]
    }
}
fn disk(out: &mut Vec<f32>, x: f32, y: f32, r: f32, flow: [f32; 2]) {
    const N: usize = 96;
    for i in 0..N {
        let a = i as f32 / N as f32 * std::f32::consts::TAU;
        let b = (i + 1) as f32 / N as f32 * std::f32::consts::TAU;
        vertex(out, x, y, flow);
        vertex(out, x + a.cos() * r, y + a.sin() * r, flow);
        vertex(out, x + b.cos() * r, y + b.sin() * r, flow);
    }
}
#[cfg(test)]
fn append_geometry(stroke: &Stroke, from: usize) -> Vec<f32> {
    geometry(stroke, from).0
}
fn geometry(stroke: &Stroke, from: usize) -> (Vec<f32>, Vec<usize>) {
    let mut vertices = Vec::new();
    let mut offsets = Vec::with_capacity(stroke.points.len() - from);
    if from == 0 && !stroke.points.is_empty() {
        offsets.push(0);
    }
    if from == 0 {
        if let Some(p) = stroke.points.first() {
            disk(
                &mut vertices,
                p.x,
                p.y,
                stroke.width * p.pressure * 0.5,
                direction(stroke, 0),
            );
        }
    }
    for i in from.max(1)..stroke.points.len() {
        offsets.push(vertices.len());
        let p = stroke.points[i];
        let a = stroke.points[i - 1];
        let length = a.distance(p);
        if length < 0.00001 {
            continue;
        }
        let r = stroke.width * p.pressure * 0.5;
        let ra = stroke.width * a.pressure * 0.5;
        let nx = (p.y - a.y) / length;
        let ny = -(p.x - a.x) / length;
        let q = [
            [a.x + nx * ra, a.y + ny * ra],
            [a.x - nx * ra, a.y - ny * ra],
            [p.x + nx * r, p.y + ny * r],
            [p.x - nx * r, p.y - ny * r],
        ];
        let fa = direction(stroke, i - 1);
        let fp = direction(stroke, i);
        for (idx, flow) in [(0, fa), (1, fa), (2, fp), (1, fa), (3, fp), (2, fp)] {
            vertex(&mut vertices, q[idx][0], q[idx][1], flow);
        }
        if i >= 2 {
            let before = stroke.points[i - 2];
            let previous = before.distance(a);
            if previous > 0.00001 {
                let px = (a.y - before.y) / previous;
                let py = -(a.x - before.x) / previous;
                let cross = px * ny - py * nx;
                let turn = cross.atan2(px * nx + py * ny);
                let sign = if cross >= 0.0 { 1.0 } else { -1.0 };
                let angle = (py * sign).atan2(px * sign);
                let steps = (turn.abs() / 0.1).ceil().max(1.0) as usize;
                for j in 0..steps {
                    let t0 = angle + turn * j as f32 / steps as f32;
                    let t1 = angle + turn * (j + 1) as f32 / steps as f32;
                    vertex(&mut vertices, a.x, a.y, fa);
                    vertex(&mut vertices, a.x + t0.cos() * ra, a.y + t0.sin() * ra, fa);
                    vertex(&mut vertices, a.x + t1.cos() * ra, a.y + t1.sin() * ra, fa);
                }
            }
        }
    }
    (vertices, offsets)
}
impl Renderer {
    pub unsafe fn new(gl: &glow::Context) -> Result<Self, String> {
        unsafe fn program(
            gl: &glow::Context,
            vert: &str,
            frag: &str,
        ) -> Result<glow::Program, String> {
            let program = gl.create_program()?;
            for (kind, source) in [(glow::VERTEX_SHADER, vert), (glow::FRAGMENT_SHADER, frag)] {
                let shader = gl.create_shader(kind)?;
                gl.shader_source(shader, source);
                gl.compile_shader(shader);
                if !gl.get_shader_compile_status(shader) {
                    return Err(gl.get_shader_info_log(shader));
                }
                gl.attach_shader(program, shader);
                gl.delete_shader(shader);
            }
            gl.link_program(program);
            if !gl.get_program_link_status(program) {
                return Err(gl.get_program_info_log(program));
            }
            Ok(program)
        }
        let full = include_str!("../shaders/fullscreen.vert");
        Ok(Self {
            ink: program(
                gl,
                include_str!("../shaders/ink.vert"),
                include_str!("../shaders/ink.frag"),
            )?,
            composite: program(gl, full, include_str!("../shaders/composite.frag"))?,
            display: program(gl, full, include_str!("../shaders/display.frag"))?,
            vao: gl.create_vertex_array()?,
            targets: Vec::new(),
            msaa: None,
            size: [0, 0],
            meshes: HashMap::new(),
            frame: 0,
        })
    }
    unsafe fn ensure_size(&mut self, gl: &glow::Context, size: [i32; 2]) -> Result<(), String> {
        if self.size == size {
            return Ok(());
        }
        for target in self.targets.drain(..) {
            gl.delete_texture(target.texture);
            gl.delete_framebuffer(target.framebuffer);
        }
        if let Some((fbo, rbo)) = self.msaa.take() {
            gl.delete_framebuffer(fbo);
            gl.delete_renderbuffer(rbo);
        }
        self.size = size;
        let fbo = gl.create_framebuffer()?;
        let rbo = gl.create_renderbuffer()?;
        gl.bind_renderbuffer(glow::RENDERBUFFER, Some(rbo));
        gl.renderbuffer_storage_multisample(
            glow::RENDERBUFFER,
            4.min(gl.get_parameter_i32(glow::MAX_SAMPLES)),
            glow::RGBA16F,
            size[0],
            size[1],
        );
        gl.bind_framebuffer(glow::FRAMEBUFFER, Some(fbo));
        gl.framebuffer_renderbuffer(
            glow::FRAMEBUFFER,
            glow::COLOR_ATTACHMENT0,
            glow::RENDERBUFFER,
            Some(rbo),
        );
        if gl.check_framebuffer_status(glow::FRAMEBUFFER) != glow::FRAMEBUFFER_COMPLETE {
            return Err("GPU multisample target is incomplete".into());
        }
        self.msaa = Some((fbo, rbo));
        for _ in 0..3 {
            let texture = gl.create_texture()?;
            gl.bind_texture(glow::TEXTURE_2D, Some(texture));
            gl.tex_image_2d(
                glow::TEXTURE_2D,
                0,
                glow::RGBA16F as i32,
                size[0],
                size[1],
                0,
                glow::RGBA,
                glow::UNSIGNED_BYTE,
                glow::PixelUnpackData::Slice(None),
            );
            gl.tex_parameter_i32(
                glow::TEXTURE_2D,
                glow::TEXTURE_MIN_FILTER,
                glow::LINEAR as i32,
            );
            gl.tex_parameter_i32(
                glow::TEXTURE_2D,
                glow::TEXTURE_MAG_FILTER,
                glow::LINEAR as i32,
            );
            gl.tex_parameter_i32(
                glow::TEXTURE_2D,
                glow::TEXTURE_WRAP_S,
                glow::CLAMP_TO_EDGE as i32,
            );
            gl.tex_parameter_i32(
                glow::TEXTURE_2D,
                glow::TEXTURE_WRAP_T,
                glow::CLAMP_TO_EDGE as i32,
            );
            let framebuffer = gl.create_framebuffer()?;
            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(framebuffer));
            gl.framebuffer_texture_2d(
                glow::FRAMEBUFFER,
                glow::COLOR_ATTACHMENT0,
                glow::TEXTURE_2D,
                Some(texture),
                0,
            );
            if gl.check_framebuffer_status(glow::FRAMEBUFFER) != glow::FRAMEBUFFER_COMPLETE {
                return Err("GPU framebuffer is incomplete".into());
            }
            self.targets.push(Target {
                texture,
                framebuffer,
            });
        }
        Ok(())
    }
    unsafe fn uniform_i(gl: &glow::Context, p: glow::Program, key: &str, v: i32) {
        gl.uniform_1_i32(gl.get_uniform_location(p, key).as_ref(), v);
    }
    unsafe fn uniform_f(gl: &glow::Context, p: glow::Program, key: &str, v: f32) {
        gl.uniform_1_f32(gl.get_uniform_location(p, key).as_ref(), v);
    }
    unsafe fn texture(
        gl: &glow::Context,
        p: glow::Program,
        key: &str,
        slot: u32,
        texture: glow::Texture,
    ) {
        gl.active_texture(glow::TEXTURE0 + slot);
        gl.bind_texture(glow::TEXTURE_2D, Some(texture));
        Self::uniform_i(gl, p, key, slot as i32);
    }
    unsafe fn mesh(&mut self, gl: &glow::Context, stroke: &Stroke) {
        let mesh = self.meshes.entry(stroke.id).or_insert_with(|| Mesh {
            buffer: gl.create_buffer().unwrap(),
            count: 0,
            body_count: 0,
            capacity: 0,
            points: 0,
            revision: stroke.revision,
            point_offsets: Vec::new(),
            last_used: self.frame,
        });
        // Rewind only the changed tip. Revision branches absent from the journal
        // (e.g. undo) rebuild in full, preserving exact saved/history geometry.
        let mut from = mesh.points;
        if mesh.revision != stroke.revision {
            from = from.min(stroke.changed_since(mesh.revision));
        }
        if mesh.points > stroke.points.len() {
            from = if mesh.revision != stroke.revision {
                from.min(stroke.points.len().saturating_sub(1))
            } else {
                0
            };
        }
        if mesh.points == 1 && stroke.points.len() > 1 {
            from = 0;
        }
        if from < mesh.points {
            mesh.body_count = mesh.point_offsets[from];
            mesh.points = from;
            mesh.point_offsets.truncate(from);
        }
        gl.bind_buffer(glow::ARRAY_BUFFER, Some(mesh.buffer));
        if mesh.points < stroke.points.len() {
            let (mut vertices, offsets) = geometry(stroke, mesh.points);
            let body_added = vertices.len();
            if let Some(p) = stroke.points.last() {
                disk(
                    &mut vertices,
                    p.x,
                    p.y,
                    stroke.width * p.pressure * 0.5,
                    direction(stroke, stroke.points.len() - 1),
                );
            }
            let needed = mesh.body_count + vertices.len();
            if needed > mesh.capacity {
                let (mut all, all_offsets) = geometry(stroke, 0);
                mesh.point_offsets = all_offsets;
                mesh.body_count = all.len();
                if let Some(p) = stroke.points.last() {
                    disk(
                        &mut all,
                        p.x,
                        p.y,
                        stroke.width * p.pressure * 0.5,
                        direction(stroke, stroke.points.len() - 1),
                    );
                }
                mesh.capacity = all.len().next_power_of_two();
                gl.buffer_data_size(
                    glow::ARRAY_BUFFER,
                    (mesh.capacity * 4) as i32,
                    glow::DYNAMIC_DRAW,
                );
                gl.buffer_sub_data_u8_slice(glow::ARRAY_BUFFER, 0, bytes(&all));
                mesh.count = all.len();
            } else {
                gl.buffer_sub_data_u8_slice(
                    glow::ARRAY_BUFFER,
                    (mesh.body_count * 4) as i32,
                    bytes(&vertices),
                );
                mesh.count = needed;
                mesh.point_offsets
                    .extend(offsets.into_iter().map(|offset| offset + mesh.body_count));
                mesh.body_count += body_added;
            }
            mesh.points = stroke.points.len();
            mesh.revision = stroke.revision;
        }
        mesh.revision = stroke.revision;
        mesh.last_used = self.frame;
        gl.enable_vertex_attrib_array(0);
        gl.vertex_attrib_pointer_f32(0, 2, glow::FLOAT, false, 16, 0);
        gl.enable_vertex_attrib_array(1);
        gl.vertex_attrib_pointer_f32(1, 2, glow::FLOAT, false, 16, 8);
        gl.draw_arrays(glow::TRIANGLES, 0, (mesh.count / 4) as i32);
    }
    unsafe fn scene(
        &mut self,
        gl: &glow::Context,
        scene: &Scene,
        size: [i32; 2],
    ) -> Result<usize, String> {
        self.ensure_size(gl, size)?;
        gl.disable(glow::SCISSOR_TEST);
        gl.disable(glow::BLEND);
        gl.disable(glow::DEPTH_TEST);
        gl.disable(glow::CULL_FACE);
        gl.bind_vertex_array(Some(self.vao));
        gl.viewport(0, 0, size[0], size[1]);
        let mut front = 0;
        gl.bind_framebuffer(glow::FRAMEBUFFER, Some(self.targets[front].framebuffer));
        gl.clear_color(1.0, 1.0, 1.0, 1.0);
        gl.clear(glow::COLOR_BUFFER_BIT);
        for layer in &scene.layers {
            if layer.blend.effect() && layer.cel.strokes.is_empty() {
                continue;
            }
            if layer.blend != crate::model::Blend::Move {
                let blur = layer.blend == crate::model::Blend::Blur;
                // Blur is one vector field for the whole layer. Local tangents and pressure
                // are interpolated by the GPU; unrelated pixels never run the blur kernel.
                gl.bind_framebuffer(glow::FRAMEBUFFER, Some(self.msaa.unwrap().0));
                if blur {
                    gl.clear_color(0.0, 0.0, 0.0, 0.0);
                } else {
                    gl.clear_color(1.0, 1.0, 1.0, 1.0);
                }
                gl.clear(glow::COLOR_BUFFER_BIT);
                gl.use_program(Some(self.ink));
                Self::uniform_i(gl, self.ink, "flow_mode", blur as i32);
                gl.uniform_2_f32(
                    gl.get_uniform_location(self.ink, "canvas").as_ref(),
                    1920.0,
                    1920.0 / scene.aspect,
                );
                for stroke in &layer.cel.strokes {
                    self.mesh(gl, stroke);
                }
                gl.disable_vertex_attrib_array(0);
                gl.disable_vertex_attrib_array(1);
                gl.bind_framebuffer(glow::READ_FRAMEBUFFER, Some(self.msaa.unwrap().0));
                gl.bind_framebuffer(glow::DRAW_FRAMEBUFFER, Some(self.targets[2].framebuffer));
                gl.blit_framebuffer(
                    0,
                    0,
                    size[0],
                    size[1],
                    0,
                    0,
                    size[0],
                    size[1],
                    glow::COLOR_BUFFER_BIT,
                    glow::NEAREST,
                );
            }
            let motions = if layer.blend == crate::model::Blend::Move {
                layer
                    .cel
                    .strokes
                    .iter()
                    .map(|s| s.motion())
                    .collect::<Vec<_>>()
            } else {
                vec![[0.0; 2]]
            };
            for (pass, motion) in motions.iter().enumerate() {
                let next = 1 - front;
                gl.bind_framebuffer(glow::FRAMEBUFFER, Some(self.targets[next].framebuffer));
                gl.use_program(Some(self.composite));
                Self::texture(gl, self.composite, "below", 0, self.targets[front].texture);
                Self::texture(gl, self.composite, "ink", 1, self.targets[2].texture);
                Self::uniform_i(gl, self.composite, "mode", layer.blend.shader_id());
                Self::uniform_i(gl, self.composite, "style", layer.style.id());
                Self::uniform_i(
                    gl,
                    self.composite,
                    "inverted",
                    (layer.inverted && pass + 1 == motions.len()) as i32,
                );
                Self::uniform_f(gl, self.composite, "strength", layer.strength);
                gl.uniform_2_f32(
                    gl.get_uniform_location(self.composite, "canvas").as_ref(),
                    1920.0,
                    1920.0 / scene.aspect,
                );
                gl.uniform_2_f32(
                    gl.get_uniform_location(self.composite, "motion").as_ref(),
                    motion[0] / 1920.0,
                    -motion[1] / (1920.0 / scene.aspect),
                );
                gl.draw_arrays(glow::TRIANGLES, 0, 3);
                front = next;
            }
        }
        Ok(front)
    }
    unsafe fn show(
        &self,
        gl: &glow::Context,
        index: usize,
        mirror: bool,
        tint: Option<([u8; 3], f32)>,
    ) {
        gl.use_program(Some(self.display));
        Self::texture(gl, self.display, "image", 0, self.targets[index].texture);
        Self::uniform_i(gl, self.display, "mirror", mirror as i32);
        Self::uniform_i(gl, self.display, "tinting", tint.is_some() as i32);
        if let Some((color, alpha)) = tint {
            gl.uniform_3_f32(
                gl.get_uniform_location(self.display, "tint").as_ref(),
                color[0] as f32 / 255.0,
                color[1] as f32 / 255.0,
                color[2] as f32 / 255.0,
            );
            Self::uniform_f(gl, self.display, "opacity", alpha);
        }
        gl.draw_arrays(glow::TRIANGLES, 0, 3);
    }
    pub unsafe fn paint(
        &mut self,
        gl: &glow::Context,
        mut info: egui::PaintCallbackInfo,
        view: &View,
    ) {
        self.frame += 1;
        let framebuffer = gl.get_parameter_framebuffer(glow::FRAMEBUFFER_BINDING);
        info.clip_rect = info.clip_rect.intersect(info.viewport);
        let vp = info.viewport_in_pixels();
        let clip = info.clip_rect_in_pixels();
        let size = [vp.width_px.max(1), vp.height_px.max(1)];
        // The UI canvas owns this scissored region, including its white paper.
        gl.enable(glow::SCISSOR_TEST);
        gl.scissor(
            clip.left_px,
            clip.from_bottom_px,
            clip.width_px,
            clip.height_px,
        );
        gl.bind_framebuffer(glow::FRAMEBUFFER, framebuffer);
        gl.clear_color(1.0, 1.0, 1.0, 1.0);
        gl.clear(glow::COLOR_BUFFER_BIT);
        for ghost in &view.ghosts {
            if let Ok(front) = self.scene(gl, &ghost.scene, size) {
                gl.bind_framebuffer(glow::FRAMEBUFFER, framebuffer);
                gl.viewport(vp.left_px, vp.from_bottom_px, vp.width_px, vp.height_px);
                gl.enable(glow::SCISSOR_TEST);
                gl.scissor(
                    clip.left_px,
                    clip.from_bottom_px,
                    clip.width_px,
                    clip.height_px,
                );
                gl.enable(glow::BLEND);
                gl.blend_func(glow::SRC_ALPHA, glow::ONE_MINUS_SRC_ALPHA);
                self.show(gl, front, view.mirror, Some((ghost.color, ghost.opacity)));
            }
        }
        if let Ok(front) = self.scene(gl, &view.scene, size) {
            gl.bind_framebuffer(glow::FRAMEBUFFER, framebuffer);
            gl.viewport(vp.left_px, vp.from_bottom_px, vp.width_px, vp.height_px);
            gl.enable(glow::SCISSOR_TEST);
            gl.scissor(
                clip.left_px,
                clip.from_bottom_px,
                clip.width_px,
                clip.height_px,
            );
            gl.enable(glow::BLEND);
            gl.blend_func(glow::DST_COLOR, glow::ZERO);
            self.show(gl, front, view.mirror, None);
        }
        gl.blend_func_separate(
            glow::ONE,
            glow::ONE_MINUS_SRC_ALPHA,
            glow::ONE_MINUS_DST_ALPHA,
            glow::ONE,
        );
        gl.active_texture(glow::TEXTURE0);
        gl.bind_vertex_array(None);
        if self.frame.is_multiple_of(120) {
            self.meshes.retain(|_, mesh| {
                if self.frame - mesh.last_used > 120 {
                    gl.delete_buffer(mesh.buffer);
                    false
                } else {
                    true
                }
            });
        }
    }
    pub unsafe fn read_frame(
        &mut self,
        gl: &glow::Context,
        scene: &Scene,
        size: [i32; 2],
    ) -> Result<Vec<u8>, String> {
        let framebuffer = gl.get_parameter_framebuffer(glow::FRAMEBUFFER_BINDING);
        let mut viewport = [0; 4];
        gl.get_parameter_i32_slice(glow::VIEWPORT, &mut viewport);
        let result = (|| {
            let front = self.scene(gl, scene, size)?;
            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(self.targets[front].framebuffer));
            let mut pixels = vec![0; size[0] as usize * size[1] as usize * 4];
            gl.pixel_store_i32(glow::PACK_ALIGNMENT, 1);
            gl.read_pixels(
                0,
                0,
                size[0],
                size[1],
                glow::RGBA,
                glow::UNSIGNED_BYTE,
                glow::PixelPackData::Slice(Some(&mut pixels)),
            );
            Ok(pixels)
        })();
        gl.bind_framebuffer(glow::FRAMEBUFFER, framebuffer);
        gl.viewport(viewport[0], viewport[1], viewport[2], viewport[3]);
        result
    }
    pub unsafe fn verify_curve_cache(
        &mut self,
        gl: &glow::Context,
        document: &crate::model::Document,
    ) {
        let actual = self.read_frame(gl, &document.scene(0), [960, 402]).unwrap();
        let mut fresh = document.clone();
        let cel = Arc::make_mut(fresh.layers[0].cels.get_mut(&0).unwrap());
        let stroke = Arc::make_mut(&mut cel.strokes[0]);
        stroke.id = crate::model::next_revision();
        let fresh_id = stroke.id;
        let expected = self.read_frame(gl, &fresh.scene(0), [960, 402]).unwrap();
        assert_eq!(
            actual, expected,
            "Incremental curve rendering differs from full geometry"
        );
        if let Some(mesh) = self.meshes.remove(&fresh_id) {
            gl.delete_buffer(mesh.buffer);
        }
    }
    pub unsafe fn verify_blur_directions(&mut self, gl: &glow::Context) {
        use crate::model::*;
        let mut document = Document::default();
        document.settings.aspect = 2.0;
        let mut stroke = Stroke::new(
            160.0,
            Point {
                x: 400.0,
                y: 250.0,
                pressure: 0.5,
            },
        );
        stroke.points.extend([
            Point {
                x: 800.0,
                y: 250.0,
                pressure: 0.5,
            },
            Point {
                x: 800.0,
                y: 700.0,
                pressure: 1.0,
            },
        ]);
        document.layers[3].cels.insert(
            0,
            Arc::new(Cel {
                strokes: vec![Arc::new(stroke)],
            }),
        );
        self.read_frame(gl, &document.scene(0), [384, 192]).unwrap();
        let framebuffer = gl.get_parameter_framebuffer(glow::FRAMEBUFFER_BINDING);
        gl.bind_framebuffer(glow::FRAMEBUFFER, Some(self.targets[2].framebuffer));
        let sample = |x: i32, y: i32| {
            let mut bytes = [0_u8; 16];
            gl.read_pixels(
                x,
                191 - y,
                1,
                1,
                glow::RGBA,
                glow::FLOAT,
                glow::PixelPackData::Slice(Some(&mut bytes)),
            );
            [
                f32::from_ne_bytes(bytes[0..4].try_into().unwrap()),
                f32::from_ne_bytes(bytes[4..8].try_into().unwrap()),
                f32::from_ne_bytes(bytes[12..16].try_into().unwrap()),
            ]
        };
        let horizontal = sample(120, 50);
        assert!(
            (horizontal[0] - 40.0).abs() < 1.0 && horizontal[1].abs() < 0.1 && horizontal[2] > 0.99,
            "Horizontal field lost pressure or direction: {horizontal:?}"
        );
        let vertical = sample(160, 130);
        assert!(
            vertical[1] > 60.0 && vertical[1] > vertical[0].abs() * 4.0 && vertical[2] > 0.99,
            "Curved field ignores local tangent: {vertical:?}"
        );
        let outside = sample(20, 20);
        assert_eq!(outside[2], 0.0);
        gl.bind_framebuffer(glow::FRAMEBUFFER, framebuffer);
    }
    pub unsafe fn destroy(&mut self, gl: &glow::Context) {
        for mesh in self.meshes.drain().map(|(_, m)| m) {
            gl.delete_buffer(mesh.buffer);
        }
        for t in self.targets.drain(..) {
            gl.delete_texture(t.texture);
            gl.delete_framebuffer(t.framebuffer);
        }
        if let Some((fbo, rbo)) = self.msaa.take() {
            gl.delete_framebuffer(fbo);
            gl.delete_renderbuffer(rbo);
        }
        for p in [self.ink, self.composite, self.display] {
            gl.delete_program(p);
        }
        gl.delete_vertex_array(self.vao);
    }
}
pub fn callback(
    rect: egui::Rect,
    renderer: Arc<Mutex<Renderer>>,
    view: View,
) -> egui::PaintCallback {
    egui::PaintCallback {
        rect,
        callback: Arc::new(eframe::egui_glow::CallbackFn::new(
            move |info, painter| unsafe {
                renderer.lock().unwrap().paint(painter.gl(), info, &view);
            },
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Point;
    fn trail() -> Stroke {
        let mut stroke = Stroke::new(
            6.0,
            Point {
                x: 0.0,
                y: 0.0,
                pressure: 0.5,
            },
        );
        stroke.points = (0..1000)
            .map(|i| Point {
                x: i as f32,
                y: (i as f32 * 0.01).sin() * 40.0,
                pressure: 0.5,
            })
            .collect();
        stroke
    }
    #[test]
    fn thick_brush_has_the_same_geometry_cost_as_thin_brush() {
        let mut stroke = trail();
        let thin = append_geometry(&stroke, 0).len();
        stroke.width = 1920.0;
        assert_eq!(append_geometry(&stroke, 0).len(), thin);
        assert!(
            thin < 80000,
            "Round disks must not be repeated at every sample"
        );
    }
    #[test]
    fn incremental_outline_matches_a_complete_rebuild() {
        let stroke = trail();
        let mut prefix = stroke.clone();
        prefix.points.truncate(600);
        let mut incremental = append_geometry(&prefix, 0);
        incremental.extend(append_geometry(&stroke, 600));
        assert_eq!(incremental, append_geometry(&stroke, 0));
    }
}
