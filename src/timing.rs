//! Opt-in frame tracing for the native input/presentation path. It performs no
//! GPU synchronization or readback, and is absent from the normal frame path.
use std::{
    collections::VecDeque,
    sync::{Mutex, OnceLock},
    time::{Duration, Instant},
};

static TRACE: OnceLock<Mutex<Trace>> = OnceLock::new();
struct Trace {
    start: Instant,
    last: Option<Instant>,
    gaps: Vec<f64>,
    frame_cpu: Vec<f64>,
    canvas_cpu: Vec<f64>,
    events: usize,
    pixels: [u32; 2],
    scale: f32,
    gpu_reported: bool,
    console: bool,
    overlay: bool,
    meter: Meter,
    last_input_time: Option<u64>,
    last_app_ms: f64,
    last_canvas_ms: f64,
    renderer: String,
    refresh_hz: f64,
    target_fps: f64,
    swap_ms: f64,
    display_clock: bool,
}
const WINDOW: Duration = Duration::from_secs(1);
struct Meter {
    start: Instant,
    updates: VecDeque<Instant>,
    paints: VecDeque<Instant>,
    moves: VecDeque<Instant>,
    samples: VecDeque<Instant>,
}
impl Meter {
    fn new(now: Instant) -> Self {
        Self {
            start: now,
            updates: VecDeque::new(),
            paints: VecDeque::new(),
            moves: VecDeque::new(),
            samples: VecDeque::new(),
        }
    }
    fn prune(&mut self, now: Instant) {
        for samples in [
            &mut self.updates,
            &mut self.paints,
            &mut self.moves,
            &mut self.samples,
        ] {
            while samples
                .front()
                .is_some_and(|time| now.duration_since(*time) > WINDOW)
            {
                samples.pop_front();
            }
        }
    }
    fn rates(&mut self, now: Instant) -> [f64; 4] {
        self.prune(now);
        let seconds = now
            .duration_since(self.start)
            .min(WINDOW)
            .as_secs_f64()
            .max(0.001);
        [
            self.paints.len() as f64 / seconds,
            self.updates.len() as f64 / seconds,
            self.moves.len() as f64 / seconds,
            self.samples.len() as f64 / seconds,
        ]
    }
    fn gaps(&self) -> (f64, f64) {
        let mut gaps: Vec<_> = self
            .paints
            .iter()
            .zip(self.paints.iter().skip(1))
            .map(|(a, b)| b.duration_since(*a).as_secs_f64() * 1000.0)
            .collect();
        if gaps.is_empty() {
            return (0.0, 0.0);
        }
        gaps.sort_by(f64::total_cmp);
        (
            gaps[(gaps.len() * 95 / 100).min(gaps.len() - 1)],
            *gaps.last().unwrap(),
        )
    }
}
pub fn initialize() {
    let console = std::env::args().any(|arg| arg == "--trace-frames");
    let overlay = std::env::args().any(|arg| arg == "--debug-fps");
    if console || overlay {
        let _ = TRACE.set(Mutex::new(Trace {
            start: Instant::now(),
            last: None,
            gaps: Vec::new(),
            frame_cpu: Vec::new(),
            canvas_cpu: Vec::new(),
            events: 0,
            pixels: [0; 2],
            scale: 1.0,
            gpu_reported: false,
            console,
            overlay,
            meter: Meter::new(Instant::now()),
            last_input_time: None,
            last_app_ms: 0.0,
            last_canvas_ms: 0.0,
            renderer: String::new(),
            refresh_hz: 0.0,
            target_fps: 0.0,
            swap_ms: 0.0,
            display_clock: false,
        }));
        if console {
            eprintln!("MORI frame tracing enabled: move the pen or mouse continuously while reproducing the issue. Gaps include idle time; CPU timings exclude swap/vsync waiting.");
        }
    }
}
fn mean(values: &[f64]) -> f64 {
    values.iter().sum::<f64>() / values.len().max(1) as f64
}
fn maximum(values: &[f64]) -> f64 {
    values.iter().copied().fold(0.0, f64::max)
}
pub fn frame(ctx: &eframe::egui::Context, frame: &eframe::Frame, pacing: &eframe::FramePacing) {
    let Some(trace) = TRACE.get() else {
        return;
    };
    let mut trace = trace.lock().unwrap();
    let now = Instant::now();
    trace.refresh_hz = pacing.refresh_rate();
    trace.target_fps = pacing.target_fps();
    trace.swap_ms = pacing.swap_ms();
    trace.display_clock = pacing.display_clock_active();
    trace.meter.updates.push_back(now);
    trace.meter.prune(now);
    let (input_time, moves) = ctx.input(|i| {
        (
            i.time.to_bits(),
            i.events
                .iter()
                .filter(|e| matches!(e, eframe::egui::Event::PointerMoved(_)))
                .count(),
        )
    });
    if trace.last_input_time != Some(input_time) {
        trace.last_input_time = Some(input_time);
        trace.events += moves;
        trace.meter.moves.extend(std::iter::repeat_n(now, moves));
    }
    if let Some(cpu) = frame.info().cpu_usage {
        trace.last_app_ms = cpu as f64 * 1000.0;
    }
    if !trace.console {
        return;
    }
    if let Some(last) = trace.last {
        trace
            .gaps
            .push(now.duration_since(last).as_secs_f64() * 1000.0);
    }
    trace.last = Some(now);
    let app_ms = trace.last_app_ms;
    trace.frame_cpu.push(app_ms);
    if now.duration_since(trace.start) >= Duration::from_secs(2) && !trace.gaps.is_empty() {
        trace.gaps.sort_by(f64::total_cmp);
        let median = trace.gaps[trace.gaps.len() / 2];
        let p95 = trace.gaps[(trace.gaps.len() * 95 / 100).min(trace.gaps.len() - 1)];
        eprintln!("MORI frames={} moves={} gap median/p95/max={:.2}/{:.2}/{:.2}ms app+draw mean/max={:.2}/{:.2}ms canvas mean/max={:.2}/{:.2}ms pixels={}x{} scale={:.2}",
            trace.gaps.len(),trace.events,median,p95,maximum(&trace.gaps),mean(&trace.frame_cpu),maximum(&trace.frame_cpu),mean(&trace.canvas_cpu),maximum(&trace.canvas_cpu),trace.pixels[0],trace.pixels[1],trace.scale);
        trace.gaps.clear();
        trace.frame_cpu.clear();
        trace.canvas_cpu.clear();
        trace.events = 0;
        trace.start = now;
    }
}
/// Count samples consumed by the drawing path separately from UI pointer events.
pub fn input_samples(count: usize) {
    if let Some(trace) = TRACE.get() {
        let mut trace = trace.lock().unwrap();
        let now = Instant::now();
        trace.meter.samples.extend(std::iter::repeat_n(now, count));
        trace.meter.prune(now);
    }
}
pub struct CanvasTimer(Instant);
pub fn canvas(pixels: [u32; 2], scale: f32) -> Option<CanvasTimer> {
    let trace = TRACE.get()?;
    let mut trace = trace.lock().unwrap();
    trace.pixels = pixels;
    trace.scale = scale;
    Some(CanvasTimer(Instant::now()))
}
impl Drop for CanvasTimer {
    fn drop(&mut self) {
        if let Some(trace) = TRACE.get() {
            let mut trace = trace.lock().unwrap();
            let now = Instant::now();
            trace.last_canvas_ms = self.0.elapsed().as_secs_f64() * 1000.0;
            trace.meter.paints.push_back(now);
            trace.meter.prune(now);
            if trace.console {
                let elapsed = trace.last_canvas_ms;
                trace.canvas_cpu.push(elapsed);
            }
        }
    }
}
pub unsafe fn gpu(gl: &eframe::glow::Context) {
    use eframe::glow::{self, HasContext};
    let Some(trace) = TRACE.get() else {
        return;
    };
    let mut trace = trace.lock().unwrap();
    if !trace.gpu_reported {
        trace.renderer = gl.get_parameter_string(glow::RENDERER);
        if trace.console {
            eprintln!(
                "MORI GPU vendor={} renderer={} GL={}",
                gl.get_parameter_string(glow::VENDOR),
                gl.get_parameter_string(glow::RENDERER),
                gl.get_parameter_string(glow::VERSION)
            );
        }
        trace.gpu_reported = true;
    }
}

/// Observation only: the overlay never schedules extra redraws or changes vsync.
pub fn overlay(ctx: &eframe::egui::Context) {
    use eframe::egui;
    let Some(trace) = TRACE.get() else {
        return;
    };
    let mut trace = trace.lock().unwrap();
    if !trace.overlay {
        return;
    }
    let rates = trace.meter.rates(Instant::now());
    let (p95, max) = trace.meter.gaps();
    let clock = if trace.display_clock {
        "display"
    } else {
        "timer + vsync"
    };
    let text = format!(
        concat!(
            "Canvas FPS {:5.1}   target {:.2} / display {:.2} Hz\n",
            "Clock {}   Swap {:.2} ms\n",
            "Update {:5.1} Hz   UI pointer {:5.1} Hz\n",
            "Canvas samples {:5.1} Hz\n",
            "Paint gap p95/max {:5.2}/{:5.2} ms\n",
            "CPU app/draw {:5.2} ms   canvas {:5.2} ms\n",
            "{} × {} px  @ {:.2}×\n{}"
        ),
        rates[0],
        trace.target_fps,
        trace.refresh_hz,
        clock,
        trace.swap_ms,
        rates[1],
        rates[2],
        rates[3],
        p95,
        max,
        trace.last_app_ms,
        trace.last_canvas_ms,
        trace.pixels[0],
        trace.pixels[1],
        trace.scale,
        trace.renderer,
    );
    drop(trace);
    egui::Area::new(egui::Id::new("debug_fps"))
        .order(egui::Order::Debug)
        .fixed_pos(egui::pos2(12.0, 12.0))
        .interactable(false)
        .show(ctx, |ui| {
            egui::Frame::new()
                .fill(egui::Color32::from_black_alpha(225))
                .corner_radius(6.0)
                .inner_margin(10)
                .show(ui, |ui| {
                    ui.label(
                        egui::RichText::new(text)
                            .monospace()
                            .size(12.0)
                            .color(egui::Color32::from_rgb(214, 230, 197)),
                    );
                });
        });
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rates_count_paints_separately_from_updates_and_pointer_events() {
        let start = Instant::now();
        let mut meter = Meter::new(start);
        for i in 0..60 {
            meter
                .paints
                .push_back(start + Duration::from_millis(i * 16));
        }
        for i in 0..90 {
            meter
                .updates
                .push_back(start + Duration::from_millis(i * 11));
        }
        for i in 0..48 {
            meter.moves.push_back(start + Duration::from_millis(i * 20));
        }
        assert_eq!(meter.rates(start + WINDOW), [60.0, 90.0, 48.0, 0.0]);
        assert_eq!(meter.gaps(), (16.0, 16.0));
        assert_eq!(meter.rates(start + Duration::from_secs(3)), [0.0; 4]);
    }
}
