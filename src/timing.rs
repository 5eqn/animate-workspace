//! Opt-in frame tracing for the native input/presentation path. It performs no
//! GPU synchronization or readback, and is absent from the normal frame path.
use std::{
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
}
pub fn initialize() {
    if std::env::args().any(|arg| arg == "--trace-frames") {
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
        }));
        eprintln!("MORI frame tracing enabled: move the pen or mouse continuously while reproducing the issue. Gaps include idle time; CPU timings exclude swap/vsync waiting.");
    }
}
fn mean(values: &[f64]) -> f64 {
    values.iter().sum::<f64>() / values.len().max(1) as f64
}
fn maximum(values: &[f64]) -> f64 {
    values.iter().copied().fold(0.0, f64::max)
}
pub fn frame(ctx: &eframe::egui::Context, frame: &eframe::Frame) {
    let Some(trace) = TRACE.get() else {
        return;
    };
    let mut trace = trace.lock().unwrap();
    let now = Instant::now();
    if let Some(last) = trace.last {
        trace
            .gaps
            .push(now.duration_since(last).as_secs_f64() * 1000.0);
    }
    trace.last = Some(now);
    if let Some(cpu) = frame.info().cpu_usage {
        trace.frame_cpu.push(cpu as f64 * 1000.0);
    }
    trace.events += ctx.input(|i| {
        i.events
            .iter()
            .filter(|e| matches!(e, eframe::egui::Event::PointerMoved(_)))
            .count()
    });
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
            trace
                .lock()
                .unwrap()
                .canvas_cpu
                .push(self.0.elapsed().as_secs_f64() * 1000.0);
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
        eprintln!(
            "MORI GPU vendor={} renderer={} GL={}",
            gl.get_parameter_string(glow::VENDOR),
            gl.get_parameter_string(glow::RENDERER),
            gl.get_parameter_string(glow::VERSION)
        );
        trace.gpu_reported = true;
    }
}
