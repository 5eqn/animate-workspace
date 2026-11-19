use crate::model::{marker_corners, Brush, Scene, Stroke};
#[allow(dead_code)]
pub mod benchmark;
mod cache;
mod verification;
use cache::{Bounds, RasterState, SceneKey};
use eframe::{
    egui,
    glow::{self, HasContext},
};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

#[derive(Clone, Copy)]
struct Target {
    texture: glow::Texture,
    framebuffer: glow::Framebuffer,
}
struct LayerRaster {
    target: Target,
    state: RasterState,
    format: u32,
}
struct SceneRaster {
    target: Target,
    key: SceneKey,
}
#[derive(PartialEq)]
struct ViewKey {
    scene: SceneKey,
    ghosts: Vec<(SceneKey, [u8; 3], u32)>,
}
impl ViewKey {
    fn new(view: &View) -> Self {
        Self {
            scene: SceneKey::new(&view.scene),
            ghosts: view
                .ghosts
                .iter()
                .map(|g| (SceneKey::new(&g.scene), g.color, g.opacity.to_bits()))
                .collect(),
        }
    }
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
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct RenderWork {
    stroke_draws: u64,
    raster_pixels: u64,
    scene_builds: u64,
    view_builds: u64,
}
pub struct Renderer {
    ink: glow::Program,
    composite: glow::Program,
    display: glow::Program,
    vao: glow::VertexArray,
    targets: Vec<Target>,
    msaa: [Option<(glow::Framebuffer, glow::Renderbuffer)>; 2],
    mask_format: u32,
    size: [i32; 2],
    meshes: HashMap<u64, Mesh>,
    frame: u64,
    layer_rasters: HashMap<usize, LayerRaster>,
    scene_rasters: Vec<SceneRaster>,
    visible_scenes: Vec<SceneKey>,
    composed_view: Option<(ViewKey, Target)>,
    work: RenderWork,
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
fn cap(out: &mut Vec<f32>, stroke: &Stroke, index: usize) {
    let p = stroke.points[index];
    let flow = direction(stroke, index);
    if stroke.brush == Brush::Marker {
        let corners = marker_corners(p, stroke.width);
        for i in [0, 1, 2, 0, 2, 3] {
            vertex(out, corners[i][0], corners[i][1], flow);
        }
    } else {
        disk(out, p.x, p.y, stroke.width * p.pressure * 0.5, flow);
    }
}
fn marker_geometry(stroke: &Stroke, from: usize) -> (Vec<f32>, Vec<usize>) {
    let mut vertices = Vec::new();
    let mut offsets = Vec::new();
    if from == 0 && !stroke.points.is_empty() {
        offsets.push(0);
        cap(&mut vertices, stroke, 0);
    }
    for i in from.max(1)..stroke.points.len() {
        offsets.push(vertices.len());
        // Convex hull of two oriented nibs is the swept hard-edged mark. Dense
        // curve sampling makes tilt changes continuous without width-dependent work.
        let mut points: Vec<([f32; 2], [f32; 2])> =
            marker_corners(stroke.points[i - 1], stroke.width)
                .into_iter()
                .map(|p| (p, direction(stroke, i - 1)))
                .chain(
                    marker_corners(stroke.points[i], stroke.width)
                        .into_iter()
                        .map(|p| (p, direction(stroke, i))),
                )
                .collect();
        points.sort_by(|a, b| a.0[0].total_cmp(&b.0[0]).then(a.0[1].total_cmp(&b.0[1])));
        points.dedup_by(|a, b| a.0 == b.0);
        let cross = |a: [f32; 2], b: [f32; 2], c: [f32; 2]| {
            (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])
        };
        let mut hull: Vec<([f32; 2], [f32; 2])> = Vec::new();
        for &point in &points {
            while hull.len() >= 2
                && cross(hull[hull.len() - 2].0, hull[hull.len() - 1].0, point.0) <= 0.0
            {
                hull.pop();
            }
            hull.push(point);
        }
        let lower = hull.len();
        for &point in points.iter().rev().skip(1) {
            while hull.len() > lower
                && cross(hull[hull.len() - 2].0, hull[hull.len() - 1].0, point.0) <= 0.0
            {
                hull.pop();
            }
            hull.push(point);
        }
        if hull.len() > 1 {
            hull.pop();
        }
        for j in 1..hull.len().saturating_sub(1) {
            for index in [0, j, j + 1] {
                let (p, flow) = hull[index];
                vertex(&mut vertices, p[0], p[1], flow);
            }
        }
    }
    (vertices, offsets)
}
#[cfg(test)]
fn append_geometry(stroke: &Stroke, from: usize) -> Vec<f32> {
    geometry(stroke, from).0
}
/// The same filled triangles used by the GPU, including the live end cap.
/// Stroke erasing queries these so rectangular nibs and pressure edges agree
/// with what is drawn.
pub(crate) fn hit_geometry(stroke: &Stroke) -> Vec<f32> {
    let mut vertices = geometry(stroke, 0).0;
    if !stroke.points.is_empty() {
        cap(&mut vertices, stroke, stroke.points.len() - 1);
    }
    vertices
}
fn geometry(stroke: &Stroke, from: usize) -> (Vec<f32>, Vec<usize>) {
    if stroke.brush == Brush::Marker {
        return marker_geometry(stroke, from);
    }
    let mut vertices = Vec::new();
    let mut offsets = Vec::with_capacity(stroke.points.len() - from);
    if from == 0 && !stroke.points.is_empty() {
        offsets.push(0);
    }
    if from == 0 && !stroke.points.is_empty() {
        cap(&mut vertices, stroke, 0);
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
        crate::timing::gpu(gl);
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
            msaa: [None; 2],
            mask_format: glow::R16F,
            size: [0, 0],
            meshes: HashMap::new(),
            frame: 0,
            layer_rasters: HashMap::new(),
            scene_rasters: Vec::new(),
            visible_scenes: Vec::new(),
            composed_view: None,
            work: RenderWork::default(),
        })
    }
    unsafe fn ensure_size(&mut self, gl: &glow::Context, size: [i32; 2]) -> Result<(), String> {
        if self.size == size {
            return Ok(());
        }
        self.clear_rasters(gl);
        for target in self.targets.drain(..) {
            gl.delete_texture(target.texture);
            gl.delete_framebuffer(target.framebuffer);
        }
        for target in &mut self.msaa {
            if let Some((fbo, rbo)) = target.take() {
                gl.delete_framebuffer(fbo);
                gl.delete_renderbuffer(rbo);
            }
        }
        self.size = size;
        for _ in 0..2 {
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
    unsafe fn multisample(
        &mut self,
        gl: &glow::Context,
        blur: bool,
    ) -> Result<glow::Framebuffer, String> {
        let index = usize::from(blur);
        if let Some((framebuffer, _)) = self.msaa[index] {
            return Ok(framebuffer);
        }
        let fbo = gl.create_framebuffer()?;
        let rbo = gl.create_renderbuffer()?;
        gl.bind_renderbuffer(glow::RENDERBUFFER, Some(rbo));
        gl.renderbuffer_storage_multisample(
            glow::RENDERBUFFER,
            4.min(gl.get_parameter_i32(glow::MAX_SAMPLES)),
            if blur {
                glow::RGBA16F
            } else {
                self.mask_format
            },
            self.size[0],
            self.size[1],
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
        self.msaa[index] = Some((fbo, rbo));
        Ok(fbo)
    }
    unsafe fn new_raster(&self, gl: &glow::Context) -> Result<Target, String> {
        self.new_raster_format(gl, glow::RGBA16F)
    }
    unsafe fn new_raster_format(&self, gl: &glow::Context, format: u32) -> Result<Target, String> {
        let texture = gl.create_texture()?;
        gl.bind_texture(glow::TEXTURE_2D, Some(texture));
        gl.tex_image_2d(
            glow::TEXTURE_2D,
            0,
            format as i32,
            self.size[0],
            self.size[1],
            0,
            glow::RGBA,
            glow::UNSIGNED_BYTE,
            glow::PixelUnpackData::Slice(None),
        );
        for parameter in [glow::TEXTURE_MIN_FILTER, glow::TEXTURE_MAG_FILTER] {
            gl.tex_parameter_i32(glow::TEXTURE_2D, parameter, glow::LINEAR as i32);
        }
        for parameter in [glow::TEXTURE_WRAP_S, glow::TEXTURE_WRAP_T] {
            gl.tex_parameter_i32(glow::TEXTURE_2D, parameter, glow::CLAMP_TO_EDGE as i32);
        }
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
            gl.delete_framebuffer(framebuffer);
            gl.delete_texture(texture);
            return Err("GPU raster cache framebuffer is incomplete".into());
        }
        Ok(Target {
            texture,
            framebuffer,
        })
    }
    unsafe fn delete_raster(gl: &glow::Context, target: Target) {
        gl.delete_texture(target.texture);
        gl.delete_framebuffer(target.framebuffer);
    }
    unsafe fn clear_rasters(&mut self, gl: &glow::Context) {
        if let Some((_, target)) = self.composed_view.take() {
            Self::delete_raster(gl, target);
        }
        for (_, raster) in self.layer_rasters.drain() {
            Self::delete_raster(gl, raster.target);
        }
        for raster in self.scene_rasters.drain(..) {
            Self::delete_raster(gl, raster.target);
        }
    }
    unsafe fn prune_scenes(&mut self, gl: &glow::Context) {
        self.scene_rasters.retain(|raster| {
            if self.visible_scenes.contains(&raster.key) {
                true
            } else {
                Self::delete_raster(gl, raster.target);
                false
            }
        });
    }
    unsafe fn scene(
        &mut self,
        gl: &glow::Context,
        scene: &Scene,
        size: [i32; 2],
    ) -> Result<Target, String> {
        self.ensure_size(gl, size)?;
        gl.bind_vertex_array(Some(self.vao));
        let key = SceneKey::new(scene);
        if let Some(cached) = self.scene_rasters.iter().find(|cached| cached.key == key) {
            return Ok(cached.target);
        }
        let rendered = self.render_scene(gl, scene, size)?;
        let target = if let Some(index) = self
            .scene_rasters
            .iter()
            .position(|old| !self.visible_scenes.contains(&old.key))
        {
            self.scene_rasters.swap_remove(index).target
        } else {
            self.new_raster(gl)?
        };
        // Retain the completed image itself. Give the compositor a spare
        // target instead of copying an entire Retina canvas after every edit.
        let target = std::mem::replace(&mut self.targets[rendered], target);
        self.scene_rasters.push(SceneRaster { key, target });
        Ok(target)
    }
    unsafe fn bind_composite_inputs(
        &self,
        gl: &glow::Context,
        below: glow::Texture,
        ink: glow::Texture,
    ) {
        // AppKit's GL driver validates every active sampler, including those in
        // branches skipped by mode. Always bind both; neither can be the draw target.
        Self::texture(gl, self.composite, "below", 0, below);
        Self::texture(gl, self.composite, "ink", 1, ink);
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
        self.work.stroke_draws += 1;
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
            if !stroke.points.is_empty() {
                cap(&mut vertices, stroke, stroke.points.len() - 1);
            }
            let needed = mesh.body_count + vertices.len();
            if needed > mesh.capacity {
                let (mut all, all_offsets) = geometry(stroke, 0);
                mesh.point_offsets = all_offsets;
                mesh.body_count = all.len();
                if !stroke.points.is_empty() {
                    cap(&mut all, stroke, stroke.points.len() - 1);
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
    unsafe fn render_scene(
        &mut self,
        gl: &glow::Context,
        scene: &Scene,
        size: [i32; 2],
    ) -> Result<usize, String> {
        self.work.scene_builds += 1;
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
        if let Some(rhythm) = scene.rhythm {
            let next = 1 - front;
            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(self.targets[next].framebuffer));
            gl.use_program(Some(self.composite));
            self.bind_composite_inputs(
                gl,
                self.targets[front].texture,
                self.targets[front].texture,
            );
            Self::uniform_i(gl, self.composite, "mode", 6);
            gl.uniform_2_i32(
                gl.get_uniform_location(self.composite, "rhythm_grid")
                    .as_ref(),
                rhythm.cols as i32,
                rhythm.rows as i32,
            );
            Self::uniform_i(gl, self.composite, "rhythm_filled", rhythm.filled as i32);
            gl.uniform_2_i32(
                gl.get_uniform_location(self.composite, "pixel_size")
                    .as_ref(),
                size[0],
                size[1],
            );
            gl.draw_arrays(glow::TRIANGLES, 0, 3);
            front = next;
        }
        self.layer_rasters.retain(|&index, raster| {
            if index < scene.layers.len() {
                true
            } else {
                Self::delete_raster(gl, raster.target);
                false
            }
        });
        for (layer_index, layer) in scene.layers.iter().enumerate() {
            if layer.blend.effect() && layer.cel.strokes.is_empty() {
                continue;
            }
            let blur = layer.blend == crate::model::Blend::Blur;
            // Ordinary ink is monochrome. R16F retains the same coverage
            // precision as RGBA16F while transferring one quarter the data.
            let format = if blur {
                glow::RGBA16F
            } else {
                self.mask_format
            };
            let old = self.layer_rasters.remove(&layer_index);
            let mut raster = match old {
                Some(raster) if raster.format == format => raster,
                old => {
                    if let Some(raster) = old {
                        Self::delete_raster(gl, raster.target);
                    }
                    LayerRaster {
                        target: self.new_raster_format(gl, format)?,
                        state: RasterState::new(blur),
                        format,
                    }
                }
            };
            let dirty = raster.state.update(&layer.cel, blur, scene.aspect);
            if let Some(dirty) = dirty {
                if let Some([x, y, w, h]) = dirty.pixels(size, scene.aspect) {
                    let msaa = self.multisample(gl, blur)?;
                    self.work.raster_pixels += (w as u64) * (h as u64);
                    let redraw = Bounds::pixel_region([x, y, w, h], size, scene.aspect);
                    gl.enable(glow::SCISSOR_TEST);
                    gl.scissor(x, y, w, h);
                    gl.bind_framebuffer(glow::FRAMEBUFFER, Some(msaa));
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
                    // Rebuild only damaged pixels, including older marks underneath
                    // an erased or reshaped tip. Unchanged pixels stay on the GPU.
                    for (i, stroke) in layer.cel.strokes.iter().enumerate() {
                        if stroke.brush != Brush::Fill
                            && !raster.state.strokes[i].bounds.intersects(redraw)
                        {
                            continue;
                        }
                        if stroke.brush == Brush::Fill {
                            let flow = direction(stroke, stroke.points.len() - 1);
                            gl.clear_color(
                                if blur { flow[0] } else { 0.0 },
                                if blur { flow[1] } else { 0.0 },
                                0.0,
                                1.0,
                            );
                            gl.clear(glow::COLOR_BUFFER_BIT);
                        } else {
                            self.mesh(gl, stroke);
                        }
                    }
                    gl.disable_vertex_attrib_array(0);
                    gl.disable_vertex_attrib_array(1);
                    gl.bind_framebuffer(glow::READ_FRAMEBUFFER, Some(msaa));
                    gl.bind_framebuffer(glow::DRAW_FRAMEBUFFER, Some(raster.target.framebuffer));
                    gl.blit_framebuffer(
                        x,
                        y,
                        x + w,
                        y + h,
                        x,
                        y,
                        x + w,
                        y + h,
                        glow::COLOR_BUFFER_BIT,
                        glow::NEAREST,
                    );
                    gl.disable(glow::SCISSOR_TEST);
                }
            }
            self.layer_rasters.insert(layer_index, raster);
            let next = 1 - front;
            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(self.targets[next].framebuffer));
            gl.use_program(Some(self.composite));
            self.bind_composite_inputs(
                gl,
                self.targets[front].texture,
                self.layer_rasters[&layer_index].target.texture,
            );
            Self::uniform_i(gl, self.composite, "mode", layer.blend.shader_id());
            Self::uniform_i(gl, self.composite, "style", layer.style.id());
            Self::uniform_i(gl, self.composite, "inverted", layer.inverted as i32);
            Self::uniform_f(gl, self.composite, "strength", layer.strength);
            gl.uniform_2_f32(
                gl.get_uniform_location(self.composite, "canvas").as_ref(),
                1920.0,
                1920.0 / scene.aspect,
            );
            gl.draw_arrays(glow::TRIANGLES, 0, 3);
            front = next;
        }
        Ok(front)
    }
    unsafe fn show(
        &self,
        gl: &glow::Context,
        texture: glow::Texture,
        mirror: bool,
        tint: Option<([u8; 3], f32)>,
    ) {
        gl.use_program(Some(self.display));
        Self::texture(gl, self.display, "image", 0, texture);
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
        let _timer = crate::timing::canvas(
            [
                info.viewport_in_pixels().width_px.max(1) as u32,
                info.viewport_in_pixels().height_px.max(1) as u32,
            ],
            info.pixels_per_point,
        );
        self.frame += 1;
        let framebuffer = gl.get_parameter_framebuffer(glow::FRAMEBUFFER_BINDING);
        info.clip_rect = info.clip_rect.intersect(info.viewport);
        let vp = info.viewport_in_pixels();
        let clip = info.clip_rect_in_pixels();
        let size = [vp.width_px.max(1), vp.height_px.max(1)];
        self.visible_scenes = std::iter::once(SceneKey::new(&view.scene))
            .chain(view.ghosts.iter().map(|g| SceneKey::new(&g.scene)))
            .collect();
        let displayed = (|| -> Result<glow::Texture, String> {
            self.ensure_size(gl, size)?;
            if view.ghosts.is_empty() {
                if let Some((_, target)) = self.composed_view.take() {
                    Self::delete_raster(gl, target);
                }
                return Ok(self.scene(gl, &view.scene, size)?.texture);
            }
            let key = ViewKey::new(view);
            if let Some((old, target)) = &self.composed_view {
                if *old == key {
                    return Ok(target.texture);
                }
            }
            self.work.view_builds += 1;
            let target = if let Some((_, target)) = self.composed_view.take() {
                target
            } else {
                self.new_raster_format(gl, glow::RGBA8)?
            };
            gl.disable(glow::SCISSOR_TEST);
            gl.disable(glow::BLEND);
            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(target.framebuffer));
            gl.viewport(0, 0, size[0], size[1]);
            gl.clear_color(1.0, 1.0, 1.0, 1.0);
            gl.clear(glow::COLOR_BUFFER_BIT);
            for ghost in &view.ghosts {
                let rendered = self.scene(gl, &ghost.scene, size)?;
                gl.bind_framebuffer(glow::FRAMEBUFFER, Some(target.framebuffer));
                gl.viewport(0, 0, size[0], size[1]);
                gl.disable(glow::SCISSOR_TEST);
                gl.enable(glow::BLEND);
                gl.blend_func(glow::SRC_ALPHA, glow::ONE_MINUS_SRC_ALPHA);
                self.show(
                    gl,
                    rendered.texture,
                    false,
                    Some((ghost.color, ghost.opacity)),
                );
            }
            let rendered = self.scene(gl, &view.scene, size)?;
            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(target.framebuffer));
            gl.viewport(0, 0, size[0], size[1]);
            gl.disable(glow::SCISSOR_TEST);
            gl.enable(glow::BLEND);
            gl.blend_func(glow::DST_COLOR, glow::ZERO);
            self.show(gl, rendered.texture, false, None);
            self.composed_view = Some((key, target));
            Ok(target.texture)
        })();
        gl.bind_framebuffer(glow::FRAMEBUFFER, framebuffer);
        gl.viewport(vp.left_px, vp.from_bottom_px, vp.width_px, vp.height_px);
        gl.enable(glow::SCISSOR_TEST);
        gl.scissor(
            clip.left_px,
            clip.from_bottom_px,
            clip.width_px,
            clip.height_px,
        );
        gl.disable(glow::BLEND);
        gl.disable(glow::DEPTH_TEST);
        gl.disable(glow::CULL_FACE);
        gl.bind_vertex_array(Some(self.vao));
        if let Ok(texture) = displayed {
            self.show(gl, texture, view.mirror, None);
        }
        gl.blend_func_separate(
            glow::ONE,
            glow::ONE_MINUS_SRC_ALPHA,
            glow::ONE_MINUS_DST_ALPHA,
            glow::ONE,
        );
        self.prune_scenes(gl);
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
        self.visible_scenes = vec![SceneKey::new(scene)];
        let result = (|| {
            let front = self.scene(gl, scene, size)?;
            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(front.framebuffer));
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
        self.prune_scenes(gl);
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
        document.settings.watermark = false;
        let mut stroke = Stroke::new(
            160.0,
            Point {
                angle: 0.0,
                x: 400.0,
                y: 250.0,
                pressure: 0.5,
            },
        );
        stroke.points.extend([
            Point {
                angle: 0.0,
                x: 800.0,
                y: 250.0,
                pressure: 0.5,
            },
            Point {
                angle: 0.0,
                x: 800.0,
                y: 700.0,
                pressure: 1.0,
            },
        ]);
        document.layers[2].cels.insert(
            0,
            Arc::new(Cel {
                strokes: vec![Arc::new(stroke)],
            }),
        );
        self.read_frame(gl, &document.scene(0), [384, 192]).unwrap();
        let framebuffer = gl.get_parameter_framebuffer(glow::FRAMEBUFFER_BINDING);
        gl.bind_framebuffer(
            glow::FRAMEBUFFER,
            Some(self.layer_rasters[&2].target.framebuffer),
        );
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
        self.clear_rasters(gl);
        for mesh in self.meshes.drain().map(|(_, m)| m) {
            gl.delete_buffer(mesh.buffer);
        }
        for t in self.targets.drain(..) {
            gl.delete_texture(t.texture);
            gl.delete_framebuffer(t.framebuffer);
        }
        for target in &mut self.msaa {
            if let Some((fbo, rbo)) = target.take() {
                gl.delete_framebuffer(fbo);
                gl.delete_renderbuffer(rbo);
            }
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
                angle: 0.0,
                x: 0.0,
                y: 0.0,
                pressure: 0.5,
            },
        );
        stroke.points = (0..1000)
            .map(|i| Point {
                angle: 0.0,
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
