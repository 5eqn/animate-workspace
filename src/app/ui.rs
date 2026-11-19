use super::*;
fn tr_ui(ui: &egui::Ui, text: impl AsRef<str>) -> String {
    i18n::tr(ui.ctx(), text)
}
use egui::scroll_area::ScrollBarVisibility;
use egui::{epaint::PathShape, Align2, Id, PointerButton, TextStyle};

pub(super) fn configure(ctx: &egui::Context) {
    ctx.set_theme(egui::Theme::Dark);
    let mut fonts = egui::FontDefinitions::default();
    fonts.font_data.insert(
        "Inter".into(),
        Arc::new(egui::FontData::from_static(include_bytes!(
            "../../assets/fonts/Inter.ttf"
        ))),
    );
    fonts
        .families
        .get_mut(&egui::FontFamily::Proportional)
        .unwrap()
        .insert(0, "Inter".into());
    fonts.font_data.insert(
        "NotoSansSC".into(),
        Arc::new(egui::FontData::from_static(include_bytes!(
            "../../assets/fonts/NotoSansSC.otf"
        ))),
    );
    for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
        fonts
            .families
            .get_mut(&family)
            .unwrap()
            .push("NotoSansSC".into());
    }
    ctx.set_fonts(fonts);
    let mut style = (*ctx.style()).clone();
    style.visuals = egui::Visuals::dark();
    style.visuals.panel_fill = BG;
    style.visuals.window_fill = PANEL;
    style.visuals.extreme_bg_color = BG;
    style.visuals.faint_bg_color = PANEL;
    style.visuals.override_text_color = Some(INK);
    style.visuals.selection.bg_fill = Color32::from_rgb(51, 57, 73);
    style.visuals.code_bg_color = Color32::from_rgb(29, 32, 39);
    style.visuals.text_cursor.stroke = UiStroke::new(1.2_f32, ACCENT);
    style.visuals.selection.stroke = UiStroke::new(1.0_f32, Color32::from_rgb(224, 231, 246));
    for (widget, fill, weak, stroke) in [
        (
            &mut style.visuals.widgets.noninteractive,
            PANEL,
            PANEL,
            LINE,
        ),
        (
            &mut style.visuals.widgets.inactive,
            Color32::from_rgb(32, 35, 43),
            Color32::TRANSPARENT,
            Color32::from_rgb(49, 54, 65),
        ),
        (
            &mut style.visuals.widgets.hovered,
            Color32::from_rgb(43, 48, 59),
            Color32::from_rgb(36, 41, 51),
            Color32::from_rgb(71, 80, 99),
        ),
        (
            &mut style.visuals.widgets.active,
            Color32::from_rgb(51, 60, 78),
            Color32::from_rgb(43, 52, 69),
            Color32::from_rgb(104, 122, 154),
        ),
        (
            &mut style.visuals.widgets.open,
            Color32::from_rgb(40, 46, 58),
            Color32::from_rgb(40, 46, 58),
            Color32::from_rgb(71, 80, 99),
        ),
    ] {
        widget.bg_fill = fill;
        widget.weak_bg_fill = weak;
        widget.bg_stroke = UiStroke::new(1.0_f32, stroke);
        widget.fg_stroke = UiStroke::new(1.0_f32, INK);
        widget.corner_radius = 6.into();
        widget.expansion = 0.0;
    }
    style.visuals.menu_corner_radius = 10.into();
    style.visuals.window_corner_radius = 9.into();
    style.visuals.window_stroke = UiStroke::new(1.0_f32, LINE);
    style.visuals.popup_shadow = egui::epaint::Shadow {
        offset: [0, 12],
        blur: 32,
        spread: 0,
        color: Color32::from_black_alpha(85),
    };
    style.spacing.item_spacing = Vec2::new(12.0, 8.0);
    style.spacing.menu_margin = egui::Margin::same(10);
    style.spacing.button_padding = Vec2::new(12.0, 8.0);
    style.spacing.interact_size = Vec2::new(36.0, 30.0);
    style
        .text_styles
        .insert(TextStyle::Body, FontId::proportional(13.0));
    style
        .text_styles
        .insert(TextStyle::Button, FontId::proportional(13.0));
    style
        .text_styles
        .insert(TextStyle::Small, FontId::proportional(11.0));
    // Native theme notifications may arrive after CreationContext initialization.
    // Both style slots are explicit so popups cannot fall back to egui's light palette.
    ctx.set_style_of(egui::Theme::Light, style.clone());
    ctx.set_style_of(egui::Theme::Dark, style);
}
fn hairline(p: &egui::Painter, rect: Rect, y: f32) {
    p.hline(rect.x_range(), y, UiStroke::new(1.0_f32, LINE));
}
fn label(
    p: &egui::Painter,
    pos: Pos2,
    align: Align2,
    value: impl ToString,
    size: f32,
    color: Color32,
) {
    p.text(
        pos,
        align,
        i18n::tr(p.ctx(), value.to_string()),
        FontId::proportional(size),
        color,
    );
}
fn label_raw(
    p: &egui::Painter,
    pos: Pos2,
    align: Align2,
    value: impl ToString,
    size: f32,
    color: Color32,
) {
    p.text(
        pos,
        align,
        value.to_string(),
        FontId::proportional(size),
        color,
    );
}
fn row(ui: &mut egui::Ui, name: &str, control: impl FnOnce(&mut egui::Ui)) {
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), 51.0), Sense::hover());
    if response.hovered() {
        ui.painter().rect_filled(
            rect.expand2(Vec2::new(10.0, 0.0)),
            5.0,
            Color32::from_rgb(21, 24, 30),
        );
    }
    label(
        ui.painter(),
        Pos2::new(rect.left(), rect.center().y),
        Align2::LEFT_CENTER,
        name,
        13.0,
        INK,
    );
    ui.scope_builder(
        egui::UiBuilder::new()
            .max_rect(Rect::from_min_max(
                Pos2::new(rect.right() - 240.0, rect.top() + 9.0),
                rect.max - Vec2::new(0.0, 9.0),
            ))
            .layout(Layout::right_to_left(Align::Center)),
        |ui| {
            ui.style_mut().visuals.widgets.inactive.weak_bg_fill = Color32::from_rgb(29, 33, 41);
            control(ui);
        },
    );
    hairline(ui.painter(), rect, rect.bottom());
}
fn group(ui: &mut egui::Ui, name: &str) {
    ui.add_space(26.0);
    text(ui, name, 12.0, MUTED);
    ui.add_space(8.0);
}
fn toggle(ui: &mut egui::Ui, value: &mut bool) -> egui::Response {
    let (rect, mut response) = ui.allocate_exact_size(Vec2::new(32.0, 19.0), Sense::click());
    if response.clicked() {
        *value = !*value;
        response.mark_changed();
    }
    let t = ui.ctx().animate_bool(response.id, *value);
    ui.painter().rect_filled(
        rect,
        10.0,
        if *value {
            ACCENT
        } else {
            Color32::from_rgb(52, 55, 63)
        },
    );
    ui.painter().circle_filled(
        Pos2::new(rect.left() + 9.5 + t * 13.0, rect.center().y),
        6.5,
        if *value { BG } else { INK },
    );
    response
}
fn number(
    ui: &mut egui::Ui,
    value: &mut f32,
    suffix: &str,
    range: std::ops::RangeInclusive<f32>,
) -> bool {
    ui.add(
        egui::DragValue::new(value)
            .range(range)
            .speed(0.2)
            .max_decimals(2)
            .suffix(suffix),
    )
    .changed()
}
fn menu_surface(ui: &mut egui::Ui) {
    ui.set_width(224.0);
    ui.spacing_mut().item_spacing = Vec2::new(8.0, 5.0);
    ui.spacing_mut().button_padding = Vec2::new(10.0, 7.0);
    ui.visuals_mut().widgets.inactive.bg_stroke = UiStroke::NONE;
}
fn menu_row(ui: &mut egui::Ui, name: &str, control: impl FnOnce(&mut egui::Ui)) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 34.0), Sense::hover());
    label(
        ui.painter(),
        rect.left_center() + Vec2::new(2.0, 0.0),
        Align2::LEFT_CENTER,
        name,
        12.0,
        MUTED,
    );
    ui.scope_builder(
        egui::UiBuilder::new()
            .max_rect(Rect::from_min_max(
                Pos2::new(rect.right() - 132.0, rect.top() + 2.0),
                rect.max - Vec2::new(0.0, 2.0),
            ))
            .layout(Layout::right_to_left(Align::Center)),
        control,
    );
}
fn wheel_directions() -> [(Brush, f32); 5] {
    std::array::from_fn(|i| {
        (
            [
                Brush::Ink,
                Brush::Marker,
                Brush::Fill,
                Brush::Eraser,
                Brush::Liquify,
            ][i],
            -std::f32::consts::FRAC_PI_2 + i as f32 * std::f32::consts::TAU / 5.0,
        )
    })
}
fn wheel_choice(center: Pos2, pos: Pos2) -> Option<Brush> {
    let d = pos - center;
    if d.length() < 14.0 {
        return None;
    }
    wheel_directions()
        .into_iter()
        .max_by(|(_, a), (_, b)| d.dot(Vec2::angled(*a)).total_cmp(&d.dot(Vec2::angled(*b))))
        .map(|(brush, _)| brush)
}
fn brush_icon(p: &egui::Painter, c: Pos2, brush: Brush, color: Color32) {
    match brush {
        Brush::Ink => {
            p.circle_filled(c, 9.0, color);
        }
        Brush::Marker => {
            let corners = marker_corners(
                Point {
                    x: c.x,
                    y: c.y,
                    pressure: 1.0,
                    angle: -0.5,
                },
                25.0,
            )
            .map(|p| Pos2::new(p[0], p[1]))
            .to_vec();
            p.add(egui::Shape::convex_polygon(corners, color, UiStroke::NONE));
        }
        Brush::Fill => {
            let corners = vec![
                c + Vec2::new(-10.0, 0.0),
                c + Vec2::new(0.0, -10.0),
                c + Vec2::new(10.0, 0.0),
                c + Vec2::new(0.0, 10.0),
            ];
            p.add(PathShape::closed_line(
                corners,
                UiStroke::new(1.5_f32, color),
            ));
            p.line_segment(
                [c + Vec2::new(-6.0, 0.0), c + Vec2::new(6.0, 0.0)],
                UiStroke::new(1.5_f32, color),
            );
            p.circle_filled(c + Vec2::new(11.0, 9.0), 2.0, color);
        }
        Brush::Eraser => {
            let points = vec![
                c + Vec2::new(-11.0, 3.0),
                c + Vec2::new(1.0, -10.0),
                c + Vec2::new(12.0, 0.0),
                c + Vec2::new(2.0, 11.0),
                c + Vec2::new(-3.0, 11.0),
            ];
            p.add(PathShape::closed_line(
                points,
                UiStroke::new(1.6_f32, color),
            ));
            p.line_segment(
                [c + Vec2::new(-5.0, -3.0), c + Vec2::new(6.0, 7.0)],
                UiStroke::new(1.6_f32, color),
            );
        }
        Brush::Liquify => {
            let mut points = vec![c + Vec2::new(0.0, -12.0)];
            for i in 0..=32 {
                // Tangents from the pointed tip to the circular body.
                let tangent = (8.0_f32 / 15.0).asin();
                let angle = -tangent + i as f32 / 32.0 * (std::f32::consts::PI + 2.0 * tangent);
                points.push(c + Vec2::new(angle.cos() * 8.0, 3.0 + angle.sin() * 8.0));
            }
            p.add(egui::Shape::convex_polygon(points, color, UiStroke::NONE));
        }
    }
}
fn diamond(p: &egui::Painter, c: Pos2, r: f32, color: Color32) {
    p.add(egui::Shape::convex_polygon(
        vec![
            c + Vec2::new(0.0, -r),
            c + Vec2::new(r, 0.0),
            c + Vec2::new(0.0, r),
            c + Vec2::new(-r, 0.0),
        ],
        color,
        UiStroke::NONE,
    ));
}

impl Studio {
    pub(super) fn feedback(&mut self, ctx: &egui::Context) {
        if let Some((message, when)) = &self.toast {
            if when.elapsed() < Duration::from_secs(4) {
                egui::Area::new(Id::new("feedback"))
                    .anchor(Align2::CENTER_BOTTOM, Vec2::new(0.0, -22.0))
                    .interactable(false)
                    .show(ctx, |ui| {
                        egui::Frame::new()
                            .fill(PANEL)
                            .corner_radius(8.0)
                            .stroke(UiStroke::new(1.0_f32, LINE))
                            .inner_margin(egui::Margin::symmetric(18, 10))
                            .show(ui, |ui| {
                                text(ui, message, 12.0, INK);
                            });
                    });
            }
        }
    }
    pub(super) fn canvas_page(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let workspace = ui.max_rect();
        ui.allocate_rect(workspace, Sense::click_and_drag());
        let width = workspace
            .width()
            .min(workspace.height() * self.doc.settings.aspect);
        let paper = Rect::from_center_size(
            workspace.center(),
            Vec2::new(width, width / self.doc.settings.aspect),
        );
        self.brush_picker(ctx, workspace);
        self.pointer_input(ctx, paper, paper.intersect(workspace));
        let painter = ui.painter().with_clip_rect(workspace);
        painter.add(render::callback(
            paper,
            self.renderer.clone(),
            self.view(true),
        ));
        let pos = ctx
            .input(|i| i.pointer.hover_pos())
            .filter(|p| paper.intersect(workspace).contains(*p));
        if let Some(pos) = pos.filter(|_| self.brush_wheel.is_none()) {
            ctx.set_cursor_icon(egui::CursorIcon::None);
            self.cursor(
                &painter,
                pos,
                self.doc.layers[self.layer].brush_width(self.doc.layers[self.layer].brush) * width
                    / 3840.0,
            );
        } else if self.resized.elapsed() < Duration::from_millis(1000) {
            self.cursor(
                &painter,
                workspace.center(),
                self.doc.layers[self.layer].brush_width(self.doc.layers[self.layer].brush) * width
                    / 3840.0,
            );
        }
    }
    fn brush_picker(&mut self, ctx: &egui::Context, workspace: Rect) {
        let events = ctx.input(|i| i.events.clone());
        for event in events {
            if matches!(
                event,
                egui::Event::WindowFocused(false)
                    | egui::Event::Key {
                        key: Key::Escape,
                        pressed: true,
                        ..
                    }
            ) {
                self.brush_wheel = None;
            }
            if let egui::Event::PointerButton {
                pos,
                button: PointerButton::Secondary,
                pressed,
                ..
            } = event
            {
                if pressed && workspace.contains(pos) && self.drawing.is_none() {
                    self.brush_wheel = Some(pos);
                }
                if !pressed {
                    if let Some(center) = self.brush_wheel.take() {
                        if let Some(brush) = wheel_choice(center, pos) {
                            if self.doc.layers[self.layer].brush != brush {
                                self.doc.layers[self.layer].brush = brush;
                                self.dirty = true;
                            }
                        }
                    }
                }
            }
        }
        if let Some(center) = self.brush_wheel {
            ctx.set_cursor_icon(egui::CursorIcon::Default);
            let selected = ctx
                .input(|i| i.pointer.latest_pos())
                .and_then(|p| wheel_choice(center, p))
                .unwrap_or(self.doc.layers[self.layer].brush);
            let p = ctx.layer_painter(egui::LayerId::new(
                egui::Order::Foreground,
                Id::new("brush_wheel"),
            ));
            p.circle_filled(center, 82.0, PANEL);
            p.circle_stroke(center, 82.0, UiStroke::new(1.0_f32, LINE));
            for (brush, angle) in wheel_directions() {
                let from = angle - std::f32::consts::PI / 5.0;
                let to = angle + std::f32::consts::PI / 5.0;
                let mut points = vec![center];
                for i in 0..=32 {
                    let a = from + i as f32 / 32.0 * (to - from);
                    points.push(center + Vec2::angled(a) * 80.0);
                }
                p.add(egui::Shape::convex_polygon(
                    points,
                    if brush == selected {
                        Color32::from_rgb(51, 59, 76)
                    } else {
                        PANEL
                    },
                    UiStroke::NONE,
                ));
                brush_icon(
                    &p,
                    center + Vec2::angled(angle) * 49.0,
                    brush,
                    if brush == selected { INK } else { MUTED },
                );
            }
            p.circle_filled(center, 15.0, PANEL);
            p.circle_stroke(center, 15.0, UiStroke::new(1.0_f32, LINE));
        }
    }
    fn cursor(&self, p: &egui::Painter, pos: Pos2, r: f32) {
        if self.doc.layers[self.layer].blend == Blend::Move {
            for axis in [Vec2::X, Vec2::Y] {
                let side = Vec2::new(-axis.y, axis.x);
                p.line_segment(
                    [pos - axis * 11.0, pos + axis * 11.0],
                    UiStroke::new(3.0_f32, Color32::WHITE),
                );
                for sign in [-1.0, 1.0] {
                    let tip = pos + axis * (11.0 * sign);
                    p.line_segment(
                        [tip - axis * (4.0 * sign) + side * 4.0, tip],
                        UiStroke::new(1.3_f32, Color32::from_gray(64)),
                    );
                    p.line_segment(
                        [tip - axis * (4.0 * sign) - side * 4.0, tip],
                        UiStroke::new(1.3_f32, Color32::from_gray(64)),
                    );
                }
                p.line_segment(
                    [pos - axis * 11.0, pos + axis * 11.0],
                    UiStroke::new(1.0_f32, Color32::from_gray(64)),
                );
            }
            return;
        }
        if self.doc.layers[self.layer].brush == Tool::Fill {
            brush_icon(p, pos, Brush::Fill, Color32::from_gray(90));
            return;
        }
        if self.doc.layers[self.layer].brush == Tool::Marker {
            let angle = self.tablet.angle().map_or(0.0, |a| {
                if self.mirror {
                    std::f32::consts::PI - a
                } else {
                    a
                }
            });
            let corners = marker_corners(
                Point {
                    x: pos.x,
                    y: pos.y,
                    pressure: 1.0,
                    angle,
                },
                r * 2.0,
            )
            .map(|p| Pos2::new(p[0], p[1]))
            .to_vec();
            p.add(PathShape::closed_line(
                corners.clone(),
                UiStroke::new(2.5_f32, Color32::from_white_alpha(150)),
            ));
            p.add(PathShape::closed_line(
                corners,
                UiStroke::new(0.8_f32, Color32::from_gray(64)),
            ));
        } else {
            p.circle_stroke(
                pos,
                r.max(1.5),
                UiStroke::new(2.5_f32, Color32::from_white_alpha(150)),
            );
            p.circle_stroke(
                pos,
                r.max(1.5),
                UiStroke::new(0.8_f32, Color32::from_gray(64)),
            );
        }
        if self.doc.layers[self.layer].brush == Tool::Liquify {
            p.circle_filled(pos, 1.2, Color32::from_gray(90));
        }
    }
    pub(super) fn timeline_page(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let area = ui.max_rect().shrink2(Vec2::new(32.0, 0.0));
        let preview_height = (area.height() * 0.44)
            .clamp(170.0, 410.0)
            .min(area.height() * 0.5);
        let preview_area = Rect::from_min_size(area.min, Vec2::new(area.width(), preview_height));
        let width = preview_area
            .width()
            .min(preview_area.height() * self.doc.settings.aspect);
        let preview = Rect::from_center_size(
            preview_area.center(),
            Vec2::new(width, width / self.doc.settings.aspect),
        );
        ui.painter().add(render::callback(
            preview,
            self.renderer.clone(),
            self.view(false),
        ));
        let timeline = Rect::from_min_max(
            Pos2::new(area.left(), preview_area.bottom() + 34.0),
            Pos2::new(area.right(), area.bottom() - 24.0),
        );
        self.timeline_tracks(ui, ctx, timeline);
    }
    fn timeline_tracks(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, rect: Rect) {
        let label_width = 190.0;
        let tracks = Rect::from_min_max(rect.min + Vec2::new(label_width, 0.0), rect.max);
        let ruler = Rect::from_min_size(tracks.min, Vec2::new(tracks.width(), 26.0));
        let wave = Rect::from_min_max(
            Pos2::new(tracks.left(), ruler.bottom() + 12.0),
            Pos2::new(tracks.right(), ruler.bottom() + 72.0),
        );
        let tempo = Rect::from_min_max(
            Pos2::new(tracks.left(), wave.bottom() + 4.0),
            Pos2::new(tracks.right(), wave.bottom() + 34.0),
        );
        let rows = Rect::from_min_max(Pos2::new(rect.left(), tempo.bottom() + 12.0), rect.max);
        let response = ui.interact(
            Rect::from_min_max(tracks.min, rect.max),
            Id::new("timeline_surface"),
            Sense::click_and_drag(),
        );
        let duration = self.doc.timing.duration;
        let span = duration / self.timeline_zoom as f64;
        if ctx.input(|i| {
            i.pointer
                .hover_pos()
                .is_some_and(|p| Rect::from_min_max(tracks.min, rect.max).contains(p))
        }) {
            let delta = ctx.input(|i| i.smooth_scroll_delta);
            let pinch = ctx.input(|i| i.zoom_delta());
            let zoom = if ctx.input(|i| i.modifiers.command || i.modifiers.ctrl) {
                (delta.y * 0.008).exp()
            } else {
                1.0
            } * pinch;
            let anchor = ctx.input(|i| i.pointer.hover_pos()).map_or(0.5, |p| {
                ((p.x - tracks.left()) / tracks.width()).clamp(0.0, 1.0)
            }) as f64;
            let time_at_anchor = self.timeline_start + anchor * span;
            self.timeline_zoom = (self.timeline_zoom * zoom).clamp(1.0, 256.0);
            let new_span = duration / self.timeline_zoom as f64;
            self.timeline_start = time_at_anchor - anchor * new_span;
            if !ctx.input(|i| i.modifiers.command || i.modifiers.ctrl) {
                self.timeline_start -=
                    (delta.x + delta.y) as f64 / tracks.width() as f64 * new_span;
            }
            if ctx.input(|i| i.pointer.middle_down()) {
                self.timeline_start -=
                    ctx.input(|i| i.pointer.delta().x) as f64 / tracks.width() as f64 * new_span;
            }
        }
        let span = duration / self.timeline_zoom as f64;
        let playhead_time = if self.transport.playing {
            self.transport.position()
        } else {
            self.time
        };
        if self.preferences.timeline_follow && self.transport.playing {
            self.timeline_start = followed_start(self.timeline_start, span, playhead_time);
        }
        self.timeline_start = self.timeline_start.clamp(0.0, (duration - span).max(0.0));
        let start = self.timeline_start;
        let end = start + span;
        let x = |time: f64| tracks.left() + ((time - start) / span) as f32 * tracks.width();
        let at = |pos: Pos2| {
            start + ((pos.x - tracks.left()) / tracks.width()).clamp(0.0, 1.0) as f64 * span
        };
        let p = ui.painter().with_clip_rect(rect);
        // Restrained hierarchy: major musical divisions, waveform, tempo starts, exposures.
        for time in self.doc.timing.display_grid(start, end, tracks.width()) {
            let segment = self.doc.timing.segment(time);
            let beat = (time - self.doc.timing.tempos[segment].start)
                * self.doc.timing.tempos[segment].bpm
                / 60.0;
            let major = (beat - beat.round()).abs() < 0.0001;
            let meter = self.doc.timing.tempos[segment];
            let bar = major
                && (beat / meter.beats as f64 - (beat / meter.beats as f64).round()).abs() < 0.0001;
            let group = bar
                && (beat / (meter.beats * meter.bars) as f64
                    - (beat / (meter.beats * meter.bars) as f64).round())
                .abs()
                    < 0.0001;
            let px = x(time);
            if major {
                label(
                    &p,
                    Pos2::new(px + 1.0, ruler.top() + 3.0),
                    Align2::LEFT_TOP,
                    if bar {
                        format!("{}", (beat / meter.beats as f64).round() as u64 + 1)
                    } else {
                        format!("{}", beat.round() as u64 % meter.beats as u64 + 1)
                    },
                    10.0,
                    if group {
                        INK
                    } else if bar {
                        ACCENT
                    } else {
                        MUTED
                    },
                );
            }
            p.line_segment(
                [
                    Pos2::new(
                        px,
                        ruler.bottom()
                            - if group {
                                12.0
                            } else if bar {
                                9.0
                            } else if major {
                                6.0
                            } else {
                                3.0
                            },
                    ),
                    Pos2::new(px, ruler.bottom()),
                ],
                UiStroke::new(1.0_f32, LINE),
            );
            if major {
                p.vline(
                    px,
                    rows.y_range(),
                    UiStroke::new(
                        1.0_f32,
                        if group {
                            Color32::from_rgb(55, 62, 78)
                        } else if bar {
                            Color32::from_rgb(40, 46, 58)
                        } else {
                            Color32::from_rgb(26, 28, 32)
                        },
                    ),
                );
            }
        }
        let mut import = false;
        let mut remove_audio = false;
        if let Some(audio) = &self.transport.audio {
            let columns = wave.width().ceil() as usize;
            for i in 0..columns {
                let lo = ((start + i as f64 / columns as f64 * span) * 200.0).floor() as usize;
                let hi = ((start + (i + 1) as f64 / columns as f64 * span) * 200.0).ceil() as usize;
                let peak = audio
                    .peaks
                    .get(lo.min(audio.peaks.len())..hi.min(audio.peaks.len()))
                    .unwrap_or(&[])
                    .iter()
                    .fold(0.0_f32, |a, v| a.max(*v));
                let h = peak * wave.height() * 0.44;
                let px = wave.left() + i as f32;
                p.line_segment(
                    [
                        Pos2::new(px, wave.center().y - h),
                        Pos2::new(px, wave.center().y + h),
                    ],
                    UiStroke::new(1.0_f32, Color32::from_rgb(112, 121, 145)),
                );
            }
            if let Some(asset) = &self.doc.audio {
                label_raw(
                    &p,
                    Pos2::new(rect.left(), wave.center().y),
                    Align2::LEFT_CENTER,
                    truncate(&asset.name, 21),
                    11.0,
                    MUTED,
                );
            }
        } else {
            let hovered =
                ctx.input(|i| i.pointer.hover_pos().is_some_and(|pos| wave.contains(pos)));
            let dragging = ctx.input(|i| !i.raw.hovered_files.is_empty());
            p.rect_filled(
                wave,
                6.0,
                if hovered || dragging {
                    Color32::from_rgb(30, 36, 48)
                } else {
                    Color32::from_rgb(22, 25, 31)
                },
            );
            p.rect_stroke(
                wave,
                6.0,
                UiStroke::new(1.0_f32, if hovered || dragging { ACCENT } else { LINE }),
                egui::StrokeKind::Inside,
            );
            label(
                &p,
                wave.center(),
                Align2::CENTER_CENTER,
                "Import audio",
                12.0,
                if hovered || dragging { INK } else { MUTED },
            );
        }
        let wave_response = ui
            .interact(wave, Id::new("wave_context"), Sense::click_and_drag())
            .on_hover_cursor(egui::CursorIcon::PointingHand);
        if self.background.is_none() {
            let dropped = ctx.input(|i| {
                i.raw
                    .dropped_files
                    .iter()
                    .find_map(|file| file.path.clone())
            });
            if let Some(path) = dropped {
                self.import_audio_path(path);
            }
        }
        if wave_response.hovered() && self.doc.audio.is_some() {
            p.rect_stroke(
                wave,
                5.0,
                UiStroke::new(1.0_f32, ACCENT),
                egui::StrokeKind::Inside,
            );
        }
        wave_response.context_menu(|ui| {
            menu_surface(ui);
            if ui
                .button(tr_ui(
                    ui,
                    if self.doc.audio.is_some() {
                        "Replace audio…"
                    } else {
                        "Import audio…"
                    },
                ))
                .clicked()
            {
                import = true;
                ui.close_menu();
            }
            if self.doc.audio.is_some() && ui.button(tr_ui(ui, "Remove audio")).clicked() {
                remove_audio = true;
                ui.close_menu();
            }
        });
        if wave_response.clicked() {
            if self.doc.audio.is_some() {
                if let Some(pos) = wave_response.interact_pointer_pos() {
                    self.set_time(at(pos));
                }
            } else {
                import = true;
            }
        }
        if import {
            self.import_audio();
        }
        if remove_audio {
            self.record();
            self.doc.audio = None;
            self.transport.stop();
            self.audio_bytes = None;
            self.transport.audio = None;
        }
        let before = self.doc.clone();
        let mut changed = false;
        if wave_response.dragged_by(PointerButton::Primary) {
            if let Some(pos) = wave_response.interact_pointer_pos() {
                self.set_time(at(pos));
            }
        }
        let mut remove_tempo = None;
        for i in 0..self.doc.timing.tempos.len() {
            let t = self.doc.timing.tempos[i];
            if t.start < start || t.start > end {
                continue;
            }
            let px = x(t.start);
            let marker = Rect::from_min_size(Pos2::new(px, tempo.top()), Vec2::new(140.0, 26.0));
            diamond(&p, Pos2::new(px + 4.0, marker.center().y), 3.0, ACCENT);
            let marker_response =
                ui.interact(marker, Id::new(("tempo", i)), Sense::click_and_drag());
            if marker_response.hovered() || marker_response.dragged() {
                p.rect_filled(marker, 4.0, Color32::from_rgb(30, 34, 43));
                diamond(&p, Pos2::new(px + 4.0, marker.center().y), 4.0, INK);
                ctx.set_cursor_icon(if marker_response.dragged() {
                    egui::CursorIcon::Grabbing
                } else {
                    egui::CursorIcon::Grab
                });
            }
            label(
                &p,
                Pos2::new(px + 15.0, marker.center().y),
                Align2::LEFT_CENTER,
                format!("{:.0}   {} × {}", t.bpm, t.bars, t.beats),
                11.0,
                ACCENT,
            );
            if i > 0 && marker_response.dragged() {
                let delta = ctx.input(|input| input.pointer.delta().x) as f64
                    / tracks.width() as f64
                    * span;
                let lo = self.doc.timing.tempos[i - 1].start + 0.000001;
                let hi = self
                    .doc
                    .timing
                    .tempos
                    .get(i + 1)
                    .map_or(duration, |t| t.start - 0.000001);
                self.doc.timing.tempos[i].start = (t.start + delta).clamp(lo, hi);
                changed = true;
            }
            marker_response.context_menu(|ui| {
                menu_surface(ui);
                changed |= ui
                    .add(
                        egui::DragValue::new(&mut self.doc.timing.tempos[i].bpm)
                            .range(1.0..=999.0)
                            .speed(0.2)
                            .suffix(" BPM"),
                    )
                    .changed();
                menu_row(ui, "Division", |ui| {
                    changed |= ui
                        .add(
                            egui::DragValue::new(&mut self.doc.timing.tempos[i].division)
                                .range(0.125..=128.0)
                                .speed(1.0),
                        )
                        .changed();
                });
                menu_row(ui, "Bars / group", |ui| {
                    changed |= ui
                        .add(
                            egui::DragValue::new(&mut self.doc.timing.tempos[i].bars).range(1..=32),
                        )
                        .changed();
                });
                menu_row(ui, "Beats / bar", |ui| {
                    changed |= ui
                        .add(
                            egui::DragValue::new(&mut self.doc.timing.tempos[i].beats)
                                .range(1..=32),
                        )
                        .changed();
                });
                if i > 0 {
                    changed |= ui
                        .add(
                            egui::DragValue::new(&mut self.doc.timing.tempos[i].start)
                                .range(0.000001..=duration)
                                .speed(0.001)
                                .suffix(" s"),
                        )
                        .changed();
                    if ui.button(tr_ui(ui, "Remove start")).clicked() {
                        remove_tempo = Some(i);
                        ui.close_menu();
                    }
                }
            });
        }
        if let Some(i) = remove_tempo {
            self.doc.timing.tempos.remove(i);
            changed = true;
        }
        let mut add_tempo = None;
        if response.secondary_clicked() {
            if let Some(pos) = response.interact_pointer_pos() {
                self.set_time(at(pos));
            }
        }
        response.context_menu(|ui| {
            menu_surface(ui);
            if ui.button(tr_ui(ui, "Add tempo start")).clicked() {
                add_tempo = Some(self.time);
                ui.close_menu();
            }
        });
        if response.double_clicked() {
            if let Some(pos) = response.interact_pointer_pos() {
                if tempo.contains(pos) {
                    add_tempo = Some(self.doc.timing.snap(at(pos)));
                }
            }
        }
        if let Some(time) = add_tempo {
            changed |= self.doc.timing.add_start(time);
        }
        if changed {
            self.doc.timing.normalize();
            self.time = self.doc.timing.snap(self.time);
            self.transport.stop();
        }
        self.ui_change(&before, changed, ctx);
        self.layer_tracks(ui, ctx, &p, rect, rows, tracks, start, span);
        if self.frame_drag.is_none()
            && (response.clicked() || response.dragged_by(PointerButton::Primary))
        {
            if let Some(pos) = response.interact_pointer_pos() {
                self.set_time(at(pos));
            }
        }
        let playhead = x(playhead_time);
        let playhead_p = ui
            .painter()
            .with_clip_rect(Rect::from_min_max(tracks.min, rect.max));
        playhead_p.vline(playhead, rect.y_range(), UiStroke::new(1.0_f32, ACCENT));
        playhead_p.circle_filled(Pos2::new(playhead, ruler.top() + 1.0), 2.5, ACCENT);
    }
    #[allow(clippy::too_many_arguments)]
    fn layer_tracks(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        p: &egui::Painter,
        rect: Rect,
        rows: Rect,
        tracks: Rect,
        start: f64,
        span: f64,
    ) {
        const HEIGHT: f32 = 58.0;
        let max_scroll = (self.doc.layers.len() as f32 * HEIGHT - rows.height()).max(0.0);
        if ctx.input(|i| {
            i.pointer
                .hover_pos()
                .is_some_and(|pos| rows.contains(pos) && pos.x < tracks.left())
        }) {
            self.layer_scroll =
                (self.layer_scroll - ctx.input(|i| i.smooth_scroll_delta.y)).clamp(0.0, max_scroll);
        }
        self.layer_scroll = self.layer_scroll.clamp(0.0, max_scroll);
        let p = p.with_clip_rect(rows);
        let x = |time: f64| tracks.left() + ((time - start) / span) as f32 * tracks.width();
        let before = self.doc.clone();
        let mut changed = false;
        let mut reorder = None;
        let mut delete = None;
        let mut create = false;
        let background = ui.interact(
            Rect::from_min_max(rows.min, Pos2::new(tracks.left() - 12.0, rows.bottom())),
            Id::new("layers_background"),
            Sense::click(),
        );
        background.context_menu(|ui| {
            menu_surface(ui);
            if ui.button(tr_ui(ui, "New layer")).clicked() {
                create = true;
                ui.close_menu();
            }
        });
        for row_index in 0..self.doc.layers.len() {
            let i = self.doc.layers.len() - 1 - row_index;
            let y = rows.top() + row_index as f32 * HEIGHT - self.layer_scroll;
            let row_rect =
                Rect::from_min_size(Pos2::new(rect.left(), y), Vec2::new(rect.width(), HEIGHT));
            if !row_rect.intersects(rows) {
                continue;
            }
            let context = ui.interact(
                row_rect.intersect(Rect::from_min_max(
                    Pos2::new(tracks.left(), rows.top()),
                    rows.max,
                )),
                Id::new(("exposure_row", i)),
                Sense::click_and_drag(),
            );
            if context.hovered() {
                p.rect_filled(context.rect, 4.0, Color32::from_rgb(27, 30, 38));
            }
            let selected = i == self.layer;
            let layer = &self.doc.layers[i];
            if selected {
                p.rect_filled(
                    row_rect,
                    4.0,
                    if context.hovered() {
                        Color32::from_rgb(30, 34, 43)
                    } else {
                        Color32::from_rgb(25, 27, 32)
                    },
                );
                p.line_segment(
                    [
                        row_rect.left_top() + Vec2::new(0.0, 14.0),
                        row_rect.left_bottom() - Vec2::new(0.0, 14.0),
                    ],
                    UiStroke::new(2.0_f32, ACCENT),
                );
            }
            label_raw(
                &p,
                Pos2::new(rect.left() + 16.0, y + HEIGHT * 0.5),
                Align2::LEFT_CENTER,
                truncate(&layer.name, 21),
                13.0,
                if layer.visible { INK } else { MUTED },
            );
            let keys: Vec<Tick> = layer.cels.keys().copied().collect();
            for (key_index, time) in keys.iter().enumerate() {
                let t = seconds(*time);
                let next = keys
                    .get(key_index + 1)
                    .map_or(self.doc.timing.duration, |t| seconds(*t));
                if next < start || t > start + span {
                    continue;
                }
                let exposure = Rect::from_min_max(
                    Pos2::new(x(t).max(tracks.left()), y + 12.0),
                    Pos2::new(x(next).min(tracks.right()), y + HEIGHT - 12.0),
                );
                let cp = p.with_clip_rect(Rect::from_min_max(
                    Pos2::new(tracks.left(), rows.top()),
                    rows.max,
                ));
                cp.rect_filled(
                    exposure,
                    3.0,
                    if selected {
                        Color32::from_rgb(49, 55, 70)
                    } else {
                        Color32::from_rgb(33, 36, 43)
                    },
                );
                diamond(
                    &cp,
                    Pos2::new(x(t) + 6.0, y + HEIGHT * 0.5),
                    2.7,
                    if selected { ACCENT } else { MUTED },
                );
            }
            if let Some((layer, _, to)) = self.frame_drag {
                if layer == i {
                    let px = x(seconds(to));
                    let end = x(self.doc.timing.next(seconds(to))).max(px + 12.0);
                    p.rect_stroke(
                        Rect::from_min_max(
                            Pos2::new(px, y + 10.0),
                            Pos2::new(end, y + HEIGHT - 10.0),
                        ),
                        3.0,
                        UiStroke::new(1.5_f32, ACCENT),
                        egui::StrokeKind::Inside,
                    );
                }
            }
            let label_response = ui.interact(
                Rect::from_min_max(
                    Pos2::new(rect.left(), y),
                    Pos2::new(tracks.left() - 12.0, y + HEIGHT),
                )
                .intersect(rows),
                Id::new(("layer", i)),
                Sense::click_and_drag(),
            );
            if label_response.hovered() {
                let h = p.with_clip_rect(rows);
                h.line_segment(
                    [
                        Pos2::new(rect.left() + 5.0, y + 18.0),
                        Pos2::new(rect.left() + 5.0, y + HEIGHT - 18.0),
                    ],
                    UiStroke::new(1.5_f32, ACCENT),
                );
                ctx.set_cursor_icon(egui::CursorIcon::Grab);
            }
            if label_response.clicked() || label_response.drag_started() {
                self.layer = i;
            }
            if label_response.drag_stopped() {
                if let Some(pos) = label_response.interact_pointer_pos() {
                    let target = (self.doc.layers.len() as isize
                        - 1
                        - ((pos.y - rows.top() + self.layer_scroll) / HEIGHT).floor() as isize)
                        .clamp(0, self.doc.layers.len().saturating_sub(1) as isize)
                        as usize;
                    if target != i {
                        reorder = Some((i, target));
                    }
                }
            }
            let frame_head = |pos: Pos2| -> Option<Tick> {
                if !(y + 10.0..=y + HEIGHT - 10.0).contains(&pos.y) {
                    return None;
                }
                keys.iter().copied().find(|time| {
                    let px = x(seconds(*time));
                    px >= tracks.left() - 1.0
                        && px <= tracks.right()
                        && (pos.x - (px + 6.0)).abs() <= 9.0
                })
            };
            if context.hovered() {
                if let Some(pos) = ctx.input(|input| input.pointer.hover_pos()) {
                    if let Some(time) = frame_head(pos) {
                        diamond(
                            &p,
                            Pos2::new(x(seconds(time)) + 6.0, y + HEIGHT * 0.5),
                            4.0,
                            INK,
                        );
                        ctx.set_cursor_icon(egui::CursorIcon::Grab);
                    } else {
                        let time = self.doc.timing.snap(
                            start
                                + ((pos.x - tracks.left()) / tracks.width()).clamp(0.0, 1.0) as f64
                                    * span,
                        );
                        p.vline(
                            x(time),
                            rows.y_range(),
                            UiStroke::new(
                                1.0_f32,
                                Color32::from_rgba_unmultiplied(183, 193, 221, 75),
                            ),
                        );
                        ctx.set_cursor_icon(egui::CursorIcon::Crosshair);
                    }
                }
            }
            if context.drag_started() {
                if let Some(origin) = ctx.input(|input| input.pointer.press_origin()) {
                    self.layer = i;
                    if let Some(from) = frame_head(origin) {
                        self.frame_drag = Some((i, from, from));
                        self.transport.stop();
                    }
                }
            }
            if context.dragged() {
                if let Some((layer, from, _)) = self.frame_drag {
                    if layer == i {
                        if let Some(pos) = context.interact_pointer_pos() {
                            let origin = ctx
                                .input(|input| input.pointer.press_origin())
                                .unwrap_or(pos);
                            let to = tick(self.doc.timing.snap(
                                seconds(from)
                                    + (pos.x - origin.x) as f64 / tracks.width() as f64 * span,
                            ));
                            self.frame_drag = Some((i, from, to));
                            self.time = seconds(to);
                            ctx.set_cursor_icon(egui::CursorIcon::Grabbing);
                        }
                    }
                } else if let Some(pos) = context.interact_pointer_pos() {
                    self.layer = i;
                    self.set_time(
                        start
                            + ((pos.x - tracks.left()) / tracks.width()).clamp(0.0, 1.0) as f64
                                * span,
                    );
                }
            }
            if context.drag_stopped() {
                if let Some((layer, from, to)) = self.frame_drag.take() {
                    if layer == i && from != to {
                        changed |= self.doc.move_exposure(i, from, to);
                        self.time = seconds(to);
                    }
                }
            }
            if context.clicked() {
                self.layer = i;
                if let Some(pos) = context.interact_pointer_pos() {
                    let time = frame_head(pos).map(seconds).unwrap_or_else(|| {
                        start
                            + ((pos.x - tracks.left()) / tracks.width()).clamp(0.0, 1.0) as f64
                                * span
                    });
                    self.set_time(time);
                }
            }
            let frame_menu_id = Id::new(("frame_menu_target", i));
            if context.secondary_clicked() {
                let target = context.interact_pointer_pos().and_then(frame_head);
                ctx.data_mut(|data| data.insert_temp(frame_menu_id, target));
            }
            let frame_target = ctx
                .data(|data| data.get_temp::<Option<Tick>>(frame_menu_id))
                .flatten();
            for response in [&label_response, &context] {
                response.context_menu(|ui| {
                    menu_surface(ui);
                    if response.id == context.id {
                        if let Some(time) = frame_target {
                            if ui.button(tr_ui(ui, "Copy frame")).clicked() {
                                self.layer = i;
                                self.transport.stop();
                                if let Some(dest) = self.doc.copy_frame_forward(i, time) {
                                    self.time = seconds(dest);
                                    changed = true;
                                }
                                ui.close_menu();
                            }
                            if ui.button(tr_ui(ui, "Delete frame")).clicked() {
                                self.layer = i;
                                self.set_time(seconds(time));
                                changed |= self.doc.layers[i].cels.remove(&time).is_some();
                                ui.close_menu();
                            }
                            return;
                        }
                    }
                    let can_delete = self.doc.layers.len() > 1;
                    let layer = &mut self.doc.layers[i];
                    changed |= ui
                        .add(
                            egui::TextEdit::singleline(&mut layer.name)
                                .font(FontId::proportional(14.0))
                                .desired_width(224.0)
                                .frame(false),
                        )
                        .changed();
                    ui.add_space(5.0);
                    ui.separator();
                    ui.add_space(3.0);
                    {
                        menu_row(ui, "Blend", |ui| {
                            egui::ComboBox::from_id_salt(("blend", i))
                                .width(124.0)
                                .selected_text(tr_ui(ui, layer.blend.name()))
                                .show_ui(ui, |ui| {
                                    for blend in Blend::OVERLAYS {
                                        if ui
                                            .selectable_value(
                                                &mut layer.blend,
                                                blend,
                                                tr_ui(ui, blend.name()),
                                            )
                                            .changed()
                                        {
                                            changed = true;
                                            if blend == Blend::Move {
                                                for cel in layer.cels.values_mut() {
                                                    let cel = Arc::make_mut(cel);
                                                    if cel.strokes.len() > 1 {
                                                        let last = cel.strokes.pop().unwrap();
                                                        cel.strokes = vec![last];
                                                    }
                                                }
                                            }
                                        }
                                    }
                                });
                        });
                    }
                    if layer.blend.effect() {
                        menu_row(ui, "Style", |ui| {
                            egui::ComboBox::from_id_salt(("style", i))
                                .width(124.0)
                                .selected_text(tr_ui(ui, layer.style.name()))
                                .show_ui(ui, |ui| {
                                    for style in Style::ALL {
                                        changed |= ui
                                            .selectable_value(
                                                &mut layer.style,
                                                style,
                                                tr_ui(ui, style.name()),
                                            )
                                            .changed();
                                    }
                                });
                        });
                    }
                    ui.add_space(5.0);
                    ui.separator();
                    if ui.button(tr_ui(ui, "New layer")).clicked() {
                        create = true;
                        ui.close_menu();
                    }
                    if can_delete
                        && ui
                            .button(
                                RichText::new(tr_ui(ui, "Delete layer"))
                                    .color(Color32::from_rgb(211, 143, 151)),
                            )
                            .clicked()
                    {
                        delete = Some(i);
                        ui.close_menu();
                    }
                });
            }
        }
        if ctx.input(|i| i.key_pressed(Key::Escape) || !i.focused) {
            self.frame_drag = None;
        }
        if let Some((from, to)) = reorder {
            let layer = self.doc.layers.remove(from);
            self.doc.layers.insert(to, layer);
            self.layer = to;
            changed = true;
        }
        if let Some(i) = delete {
            self.doc.layers.remove(i);
            self.layer = self.layer.min(self.doc.layers.len() - 1);
            changed = true;
        }
        if create {
            let name = format!("{} {}", self.translated("Layer"), self.doc.layers.len() + 1);
            self.doc
                .layers
                .push(self.preferences.layer(&name, Blend::Xor));
            self.layer = self.doc.layers.len() - 1;
            changed = true;
        }
        self.ui_change(&before, changed, ctx);
    }
    fn ui_change(&mut self, before: &Document, changed: bool, ctx: &egui::Context) {
        if changed {
            if before.timing.tempos != self.doc.timing.tempos {
                let source = self.timing_origin.get_or_insert_with(|| before.clone());
                // A continuous BPM edit always regrids the original exposures, preventing
                // cumulative drift/collisions while dragging a value through many samples.
                for (layer, original) in self.doc.layers.iter_mut().zip(&source.layers) {
                    layer.cels = original.cels.clone();
                }
                self.doc.snap_exposures();
                self.time = self.doc.timing.snap(self.time);
                self.transport.stop();
            }
            if !self.ui_editing {
                self.history.record(before);
            }
            self.dirty = true;
            self.ui_editing = ctx.input(|i| i.pointer.any_down()) || ctx.wants_keyboard_input();
        }
    }
    pub(super) fn settings_page(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let rect = ui.max_rect();
        let sidebar = Rect::from_min_max(rect.min, Pos2::new(rect.left() + 215.0, rect.bottom()));
        ui.painter().vline(
            sidebar.right(),
            rect.y_range(),
            UiStroke::new(1.0_f32, LINE),
        );
        let tabs = [
            "Project",
            "Brushes",
            "Onion skin",
            "Playback",
            "Export",
            "Shortcuts",
            "General",
        ];
        let back = Rect::from_min_size(
            sidebar.min + Vec2::new(24.0, 38.0),
            Vec2::new(sidebar.width() - 48.0, 26.0),
        );
        let response = ui
            .interact(back, Id::new("settings_back"), Sense::click())
            .on_hover_cursor(egui::CursorIcon::PointingHand);
        let color = if response.hovered() { INK } else { MUTED };
        let c = Pos2::new(back.left() + 7.0, back.center().y);
        ui.painter().add(PathShape::line(
            vec![
                c + Vec2::new(3.0, -4.0),
                c - Vec2::new(1.0, 0.0),
                c + Vec2::new(3.0, 4.0),
            ],
            UiStroke::new(1.3_f32, color),
        ));
        label(
            ui.painter(),
            Pos2::new(back.left() + 24.0, back.center().y),
            Align2::LEFT_CENTER,
            "Back",
            13.0,
            color,
        );
        if response.clicked() {
            self.page = self.return_page;
            self.bind_capture = None;
            ctx.memory_mut(|m| m.surrender_focus(m.focused().unwrap_or(Id::NULL)));
        }
        for (i, name) in tabs.iter().enumerate() {
            let r = Rect::from_min_size(
                sidebar.min + Vec2::new(24.0, 104.0 + i as f32 * 48.0),
                Vec2::new(sidebar.width() - 48.0, 40.0),
            );
            let response = ui
                .interact(r, Id::new(("settings_tab", i)), Sense::click())
                .on_hover_cursor(egui::CursorIcon::PointingHand);
            if self.settings_tab == i || response.hovered() {
                ui.painter().rect_filled(
                    r,
                    5.0,
                    if self.settings_tab == i {
                        Color32::from_rgb(30, 34, 43)
                    } else {
                        Color32::from_rgb(24, 27, 33)
                    },
                );
            }
            label(
                ui.painter(),
                r.left_center() + Vec2::new(12.0, 0.0),
                Align2::LEFT_CENTER,
                name,
                13.0,
                if self.settings_tab == i { INK } else { MUTED },
            );
            if response.clicked() {
                self.settings_tab = i;
                self.bind_capture = None;
                ctx.memory_mut(|m| m.surrender_focus(m.focused().unwrap_or(Id::NULL)));
            }
        }
        let content = Rect::from_min_max(
            Pos2::new(sidebar.right() + 56.0, rect.top() + 40.0),
            Pos2::new(
                (sidebar.right() + 800.0).min(rect.right() - 48.0),
                rect.bottom() - 24.0,
            ),
        );
        let before = self.doc.clone();
        let mut local_changed = false;
        ui.scope_builder(egui::UiBuilder::new().max_rect(content), |ui| {
            egui::ScrollArea::vertical()
                .id_salt(("settings", self.settings_tab))
                .scroll_bar_visibility(ScrollBarVisibility::AlwaysHidden)
                .show(ui, |ui| {
                    text(ui, tabs[self.settings_tab], 27.0, INK);
                    ui.add_space(4.0);
                    text(
                        ui,
                        if self.settings_tab == 0 {
                            "This project"
                        } else {
                            "Preferences"
                        },
                        11.0,
                        MUTED,
                    );
                    ui.add_space(20.0);
                    match self.settings_tab {
                        0 => {
                            row(ui, "Name", |ui| {
                                local_changed |= ui
                                    .add(
                                        egui::TextEdit::singleline(&mut self.doc.name)
                                            .desired_width(220.0)
                                            .frame(false),
                                    )
                                    .changed();
                            });
                            row(ui, "Aspect ratio", |ui| {
                                local_changed |=
                                    number(ui, &mut self.doc.settings.aspect, ": 1", 0.2..=8.0);
                            });
                            row(ui, "Duration", |ui| {
                                local_changed |= ui
                                    .add(
                                        egui::DragValue::new(&mut self.doc.timing.duration)
                                            .range(0.1..=86400.0)
                                            .speed(0.1)
                                            .suffix(" s"),
                                    )
                                    .changed();
                            });
                            group(ui, "New projects");
                            row(ui, "Default aspect ratio", |ui| {
                                number(ui, &mut self.preferences.default_aspect, ": 1", 0.2..=8.0);
                            });
                            row(ui, "Default division", |ui| {
                                let mut division=1.0/self.preferences.default_step;
                                if ui.add(egui::DragValue::new(&mut division).range(0.125..=128.0).speed(1.0)).changed() {
                                    self.preferences.default_step=1.0/division;
                                }
                            });
                            group(ui,"Rhythm watermark");
                            row(ui,"This project",|ui|{local_changed|=toggle(ui,&mut self.doc.settings.watermark).changed();});
                            row(ui,"New projects",|ui|{toggle(ui,&mut self.preferences.default_watermark);});
                        }
                        1 => {
                            row(ui, "Pressure curve", |ui| {
                                number(ui, &mut self.preferences.pressure_gamma, "", 0.2..=3.0);
                            });
                            let (curve, response) = ui.allocate_exact_size(
                                Vec2::new(ui.available_width(), 112.0),
                                Sense::drag(),
                            );
                            let plot = curve.shrink2(Vec2::new(16.0, 16.0));
                            let p = ui.painter();
                            p.line_segment(
                                [plot.left_bottom(), plot.right_bottom()],
                                UiStroke::new(1.0_f32, LINE),
                            );
                            p.line_segment(
                                [plot.left_bottom(), plot.left_top()],
                                UiStroke::new(1.0_f32, LINE),
                            );
                            let points = (0..=100)
                                .map(|i| {
                                    let t = i as f32 / 100.0;
                                    Pos2::new(
                                        plot.left() + t * plot.width(),
                                        plot.bottom()
                                            - t.powf(self.preferences.pressure_gamma)
                                                .max(self.preferences.minimum_pressure)
                                                * plot.height(),
                                    )
                                })
                                .collect();
                            p.add(PathShape::line(points, UiStroke::new(1.5_f32, ACCENT)));
                            if response.dragged() {
                                if let Some(pos) = response.interact_pointer_pos() {
                                    let x =
                                        ((pos.x - plot.left()) / plot.width()).clamp(0.01, 0.99);
                                    let y =
                                        ((plot.bottom() - pos.y) / plot.height()).clamp(0.01, 0.99);
                                    self.preferences.pressure_gamma =
                                        (y.ln() / x.ln()).clamp(0.2, 3.0);
                                }
                            }
                            row(ui, "Minimum diameter", |ui| {
                                number(ui, &mut self.preferences.minimum_pressure, "", 0.0..=0.5);
                            });
                            row(ui, "Sample spacing", |ui| {
                                ui.add(egui::DragValue::new(&mut self.preferences.spacing).range(0.25..=8.0).speed(0.2).max_decimals(2).suffix(" px"))
                                    .on_hover_text(tr_ui(ui,"Maximum distance between generated curve points, in canvas pixels."));
                            });
                            row(ui, "Interpolation", |ui| {
                                egui::ComboBox::from_id_salt("interpolation")
                                    .selected_text(tr_ui(ui,match self.preferences.interpolation{Interpolation::Linear=>"Linear",Interpolation::Smooth=>"Smooth"}))
                                    .width(120.0)
                                    .show_ui(ui, |ui| {
                                        ui.selectable_value(
                                            &mut self.preferences.interpolation,
                                            Interpolation::Linear,
                                            tr_ui(ui,"Linear"),
                                        );
                                        ui.selectable_value(
                                            &mut self.preferences.interpolation,
                                            Interpolation::Smooth,
                                            tr_ui(ui,"Smooth"),
                                        );
                                    });
                            });
                            row(ui, "Liquify strength", |ui| {
                                number(ui, &mut self.preferences.liquify_strength, "", 0.05..=1.0);
                            });
                            group(ui, "New layer sizes");
                            row(ui, "", |ui| {
                                ui.add_sized([72.0,24.0],egui::Label::new(RichText::new(tr_ui(ui,"Eraser")).color(MUTED).size(11.0)));
                                ui.add_sized([72.0,24.0],egui::Label::new(RichText::new(tr_ui(ui,"Liquify")).color(MUTED).size(11.0)));
                                ui.add_sized([72.0,24.0],egui::Label::new(RichText::new(tr_ui(ui,"Marker")).color(MUTED).size(11.0)));
                                ui.add_sized([72.0,24.0],egui::Label::new(RichText::new(tr_ui(ui,"Round")).color(MUTED).size(11.0)));
                            });
                            for blend in [
                                Blend::Base,
                                Blend::Xor,
                                Blend::Darken,
                                Blend::Lighten,
                                Blend::Move,
                                Blend::Blur,
                            ] {
                                row(ui, if blend==Blend::Base{"First layer"}else{blend.name()}, |ui| {
                                    let defaults = &mut self.preferences.brush_defaults
                                        [blend.shader_id() as usize];
                                    ui.add_sized(
                                        [72.0, 30.0],
                                        egui::DragValue::new(&mut defaults.eraser)
                                            .range(1.0..=1920.0).speed(1.0).suffix(" px"),
                                    );
                                    ui.add_sized(
                                        [72.0, 30.0],
                                        egui::DragValue::new(&mut defaults.liquify)
                                            .range(1.0..=1920.0)
                                            .speed(1.0)
                                            .suffix(" px"),
                                    );
                                    ui.add_sized([72.0,30.0],egui::DragValue::new(&mut defaults.marker).range(1.0..=1920.0).speed(1.0).suffix(" px"));
                                    ui.add_sized(
                                        [72.0, 30.0],
                                        egui::DragValue::new(&mut defaults.ink)
                                            .range(1.0..=1920.0)
                                            .speed(1.0)
                                            .suffix(" px"),
                                    );
                                });
                            }
                        }
                        2 => {
                            let onion = &mut self.preferences.onion;
                            row(ui, "Enabled", |ui| {
                                toggle(ui, &mut onion.enabled);
                            });
                            row(ui, "Previous frames", |ui| {
                                ui.color_edit_button_srgb(&mut onion.before);
                                ui.add(egui::DragValue::new(&mut onion.previous).range(0..=12));
                            });
                            row(ui, "Following frames", |ui| {
                                ui.color_edit_button_srgb(&mut onion.after);
                                ui.add(egui::DragValue::new(&mut onion.next).range(0..=12));
                            });
                            row(ui, "Opacity", |ui| {
                                number(ui, &mut onion.opacity, "", 0.0..=1.0);
                            });
                            row(ui, "Distance falloff", |ui| {
                                number(ui, &mut onion.falloff, "", 0.0..=1.0);
                            });
                        }
                        3 => {
                            row(ui, "Picture offset", |ui| {
                                ui.add(
                                    egui::DragValue::new(&mut self.preferences.video_delay_ms)
                                        .speed(1.0)
                                        .suffix(" ms")
                                        .custom_formatter(|n, _| format!("{n:+.0}")),
                                );
                            });
                            ui.add_space(16.0);
                            text(
                                ui,
                                "Positive delays the picture. Negative advances it.",
                                12.0,
                                MUTED,
                            );
                            text(
                                ui,
                                "Playback only; exported timing stays unchanged.",
                                12.0,
                                MUTED,
                            );
                            ui.add_space(22.0);
                            row(ui, "On pause", |ui| {
                                let selected=match self.preferences.pause_behavior {
                                    PauseBehavior::ReturnToStart=>"Return to start",
                                    PauseBehavior::KeepPlayhead=>"Keep playhead",
                                };
                                egui::ComboBox::from_id_salt("pause_behavior")
                                    .width(172.0).selected_text(tr_ui(ui,selected)).show_ui(ui, |ui| {
                                        ui.selectable_value(&mut self.preferences.pause_behavior,PauseBehavior::ReturnToStart,tr_ui(ui,"Return to start"));
                                        ui.selectable_value(&mut self.preferences.pause_behavior,PauseBehavior::KeepPlayhead,tr_ui(ui,"Keep playhead"));
                                    });
                            });
                            row(ui, "Follow playback", |ui| {
                                toggle(ui, &mut self.preferences.timeline_follow);
                            });
                        }
                        4 => {
                            row(ui, "Width", |ui| {
                                ui.add(
                                    egui::DragValue::new(&mut self.preferences.export.width)
                                        .range(64..=7680)
                                        .speed(2)
                                        .suffix(" px"),
                                );
                            });
                            row(ui, "Frame rate", |ui| {
                                ui.add(
                                    egui::DragValue::new(&mut self.preferences.export.fps)
                                        .range(1..=240)
                                        .suffix(" fps"),
                                );
                            });
                            row(ui, "Quality · CRF", |ui| {
                                ui.add(
                                    egui::DragValue::new(&mut self.preferences.export.crf)
                                        .range(0..=30),
                                );
                            });
                            row(ui, "Audio", |ui| {
                                toggle(ui, &mut self.preferences.export.audio);
                            });
                            group(ui, "Project range");
                            row(ui, "From", |ui| {
                                local_changed |= ui
                                    .add(
                                        egui::DragValue::new(&mut self.doc.settings.export.start)
                                            .range(0.0..=86400.0)
                                            .speed(0.1)
                                            .suffix(" s"),
                                    )
                                    .changed();
                            });
                            row(ui, "To", |ui| {
                                local_changed |= ui
                                    .add(
                                        egui::DragValue::new(&mut self.doc.settings.export.end)
                                            .range(0.001..=86400.0)
                                            .speed(0.1)
                                            .suffix(" s"),
                                    )
                                    .changed();
                            });
                            ui.add_space(28.0);
                            if let Some(job) = &self.export {
                                ui.add(
                                    egui::ProgressBar::new(job.frame as f32 / job.frames as f32)
                                        .text(format!(
                                            "{:.0}%",
                                            job.frame as f32 / job.frames as f32 * 100.0
                                        )),
                                );
                                if ui.button(tr_ui(ui,"Cancel export")).clicked() {
                                    self.export = None;
                                    self.notify("Export cancelled");
                                }
                            } else if ui
                                .add_enabled(
                                    self.doc.settings.export.end > self.doc.settings.export.start,
                                    egui::Button::new(RichText::new(tr_ui(ui,"Export video")).color(Color32::from_rgb(228,234,247)))
                                        .fill(Color32::from_rgb(54,73,107))
                                        .min_size(Vec2::new(160.0, 36.0)),
                                )
                                .clicked()
                            {
                                self.start_export();
                            }
                        }
                        5 => {
                            let mut capture = None;
                            for (i, binding) in self.preferences.bindings.iter().enumerate() {
                                row(ui, &binding.action, |ui| {
                                    if ui
                                        .button(
                                            RichText::new(if self.bind_capture == Some(i) {
                                                tr_ui(ui,"Press keys…")
                                            } else {
                                                binding.label()
                                            })
                                            .color(
                                                if self.bind_capture == Some(i) {
                                                    ACCENT
                                                } else {
                                                    MUTED
                                                },
                                            ),
                                        )
                                        .clicked()
                                    {
                                        capture = Some(i);
                                    }
                                });
                            }
                            if let Some(index) = capture {
                                self.bind_capture = Some(index);
                            }
                            ui.add_space(24.0);
                            if ui.button(tr_ui(ui,"Restore shortcuts")).clicked() {
                                self.preferences.bindings = default_bindings();
                                self.bind_capture = None;
                            }
                        }
                        _=>{
                            row(ui,"Language",|ui|{
                                let selected=match self.preferences.language{Language::System=>"Follow system",Language::English=>"English",Language::Chinese=>"简体中文"};
                                egui::ComboBox::from_id_salt("language").width(160.0).selected_text(tr_ui(ui,selected)).show_ui(ui,|ui|{
                                    ui.selectable_value(&mut self.preferences.language,Language::System,tr_ui(ui,"Follow system"));
                                    ui.selectable_value(&mut self.preferences.language,Language::English,"English");
                                    ui.selectable_value(&mut self.preferences.language,Language::Chinese,"简体中文");
                                });
                            });
                            ui.add_space(26.0);
                            if ui.button(tr_ui(ui,"Export settings…")).clicked(){self.export_preferences();}
                            if ui.button(tr_ui(ui,"Import settings…")).clicked(){self.import_preferences();}
                            ui.add_space(22.0);
                            if ui.button(RichText::new(tr_ui(ui,"Restore all defaults…")).color(Color32::from_rgb(211,143,151))).clicked(){self.reset_preferences=true;}
                        }

                    }
                });
        });
        if self.reset_preferences {
            let response = egui::Modal::new(Id::new("restore_preferences"))
                .frame(egui::Frame::popup(ui.style()).inner_margin(24))
                .show(ctx, |ui| {
                    ui.set_width(360.0);
                    text(ui, "Restore all defaults?", 20.0, INK);
                    ui.add_space(12.0);
                    ui.label(tr_ui(ui, "All application preferences will be reset. This project will stay unchanged."));
                    ui.add_space(20.0);
                    ui.horizontal(|ui| {
                        if ui.button(tr_ui(ui, "Cancel")).clicked() {
                            self.reset_preferences = false;
                        }
                        if ui.button(tr_ui(ui, "Restore defaults")).clicked() {
                            self.preferences = Preferences::default();
                            self.reset_preferences = false;
                            self.bind_capture = None;
                            self.notify("Defaults restored");
                        }
                    });
                });
            if response.should_close() {
                self.reset_preferences = false;
            }
        }
        if local_changed {
            self.time = self.doc.timing.snap(self.time);
        }
        self.ui_change(&before, local_changed, ctx);
    }
}
fn followed_start(start: f64, span: f64, time: f64) -> f64 {
    if time < start {
        time
    } else {
        start.max(time - span * 0.8)
    }
}
fn truncate(value: &str, limit: usize) -> String {
    if value.chars().count() > limit {
        value
            .chars()
            .take(limit - 1)
            .chain(std::iter::once('…'))
            .collect()
    } else {
        value.into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn playback_follow_preserves_the_view_until_the_playhead_reaches_its_margin() {
        assert_eq!(followed_start(0.0, 2.0, 1.0), 0.0);
        assert!((followed_start(0.0, 2.0, 1.9) - 0.3).abs() < 1e-9);
        assert_eq!(followed_start(4.0, 2.0, 1.0), 1.0);
    }
    #[test]
    fn brush_wheel_has_five_equal_sectors() {
        let directions = wheel_directions();
        for (i, &(brush, angle)) in directions.iter().enumerate() {
            assert_eq!(
                wheel_choice(Pos2::ZERO, Pos2::ZERO + Vec2::angled(angle) * 55.0),
                Some(brush)
            );
            let next = directions[(i + 1) % 5].1;
            assert!(
                ((next - angle).rem_euclid(std::f32::consts::TAU) - std::f32::consts::TAU / 5.0)
                    .abs()
                    < 1e-6
            );
        }
        assert_eq!(wheel_choice(Pos2::ZERO, Pos2::new(0.0, 5.0)), None);
    }
    #[test]
    fn chinese_catalog_has_renderable_glyphs() {
        let ctx = egui::Context::default();
        configure(&ctx);
        let catalog: std::collections::HashMap<String, String> =
            serde_json::from_str(include_str!("../../assets/locales/zh-CN.json")).unwrap();
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            ctx.fonts(|fonts| {
                let font = FontId::proportional(13.0);
                for value in catalog.values() {
                    for ch in value.chars().filter(|ch| !ch.is_whitespace()) {
                        assert!(fonts.has_glyph(&font, ch), "Missing UI glyph: {ch}");
                    }
                }
            });
        });
    }
    #[test]
    fn native_light_theme_does_not_reset_popup_or_input_colors() {
        let ctx = egui::Context::default();
        configure(&ctx);
        for theme in [egui::Theme::Light, egui::Theme::Dark] {
            let input = egui::RawInput {
                system_theme: Some(theme),
                ..Default::default()
            };
            let _ = ctx.run(input, |ctx| {
                assert_eq!(ctx.theme(), egui::Theme::Dark);
                let style = ctx.style();
                let visuals = &style.visuals;
                for fill in [
                    visuals.window_fill,
                    visuals.extreme_bg_color,
                    visuals.widgets.noninteractive.bg_fill,
                    visuals.widgets.inactive.bg_fill,
                    visuals.widgets.active.bg_fill,
                    visuals.widgets.hovered.bg_fill,
                    visuals.widgets.open.bg_fill,
                ] {
                    assert!(
                        fill.r() < 100 && fill.g() < 100 && fill.b() < 100,
                        "A widget reverted to a light surface"
                    );
                }
                assert!(visuals.widgets.open.fg_stroke.color.r() > 180);
            });
        }
    }
}
