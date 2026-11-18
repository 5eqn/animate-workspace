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
fn disk(out: &mut Vec<f32>, x: f32, y: f32, r: f32) {
    const N: usize = 96;
    for i in 0..N {
        let a = i as f32 / N as f32 * std::f32::consts::TAU;
        let b = (i + 1) as f32 / N as f32 * std::f32::consts::TAU;
        out.extend_from_slice(&[
            x,
            y,
            x + a.cos() * r,
            y + a.sin() * r,
            x + b.cos() * r,
            y + b.sin() * r,
        ]);
    }
}
fn append_geometry(stroke: &Stroke, from: usize) -> Vec<f32> {
    let mut vertices = Vec::new();
    if from == 0 {
        if let Some(p) = stroke.points.first() {
            disk(&mut vertices, p.x, p.y, stroke.width * p.pressure * 0.5);
        }
    }
    for i in from.max(1)..stroke.points.len() {
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
            a.x + nx * ra,
            a.y + ny * ra,
            a.x - nx * ra,
            a.y - ny * ra,
            p.x + nx * r,
            p.y + ny * r,
            p.x - nx * r,
            p.y - ny * r,
        ];
        vertices.extend_from_slice(&[
            q[0], q[1], q[2], q[3], q[4], q[5], q[2], q[3], q[6], q[7], q[4], q[5],
        ]);
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
                    vertices.extend_from_slice(&[
                        a.x,
                        a.y,
                        a.x + t0.cos() * ra,
                        a.y + t0.sin() * ra,
                        a.x + t1.cos() * ra,
                        a.y + t1.sin() * ra,
                    ]);
                }
            }
        }
    }
    vertices
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
            glow::RGBA8,
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
                glow::RGBA8 as i32,
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
            last_used: self.frame,
        });
        // Arc copy-on-write can restore an older stroke with the same ID after undo.
        // Rebuild on point count shrink or changed geometry revision.
        if mesh.revision != stroke.revision || mesh.points > stroke.points.len() {
            mesh.points = 0;
            mesh.count = 0;
            mesh.body_count = 0;
        }
        gl.bind_buffer(glow::ARRAY_BUFFER, Some(mesh.buffer));
        if mesh.points < stroke.points.len() {
            let mut vertices = append_geometry(stroke, mesh.points);
            let body_added = vertices.len();
            if let Some(p) = stroke.points.last() {
                disk(&mut vertices, p.x, p.y, stroke.width * p.pressure * 0.5);
            }
            let needed = mesh.body_count + vertices.len();
            if needed > mesh.capacity {
                let mut all = append_geometry(stroke, 0);
                mesh.body_count = all.len();
                if let Some(p) = stroke.points.last() {
                    disk(&mut all, p.x, p.y, stroke.width * p.pressure * 0.5);
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
                mesh.body_count += body_added;
            }
            mesh.points = stroke.points.len();
            mesh.revision = stroke.revision;
        }
        mesh.last_used = self.frame;
        gl.enable_vertex_attrib_array(0);
        gl.vertex_attrib_pointer_f32(0, 2, glow::FLOAT, false, 8, 0);
        gl.draw_arrays(glow::TRIANGLES, 0, (mesh.count / 2) as i32);
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
            if !layer.blend.effect() {
                gl.bind_framebuffer(glow::FRAMEBUFFER, Some(self.msaa.unwrap().0));
                gl.clear_color(1.0, 1.0, 1.0, 1.0);
                gl.clear(glow::COLOR_BUFFER_BIT);
                gl.use_program(Some(self.ink));
                gl.uniform_2_f32(
                    gl.get_uniform_location(self.ink, "canvas").as_ref(),
                    1920.0,
                    1920.0 / scene.aspect,
                );
                for stroke in &layer.cel.strokes {
                    self.mesh(gl, stroke);
                }
                gl.disable_vertex_attrib_array(0);
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
            let motions = if layer.blend.effect() {
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
            thin < 40000,
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
