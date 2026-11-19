//! MORI's display-clock integration. The native event loop continues receiving
//! all input while a repaint waits for its display slot; it never sleeps in paint.
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

/// Shared controls and observations for display-paced native rendering.
/// Zero means the current display's refresh rate. A positive value selects a
/// lower maximum; it never schedules more frames than the display can present.
#[derive(Debug, Default)]
pub struct FramePacing {
    max_fps: AtomicU32,
    pub(crate) display_clock: AtomicBool,
    pub(crate) refresh_millihertz: AtomicU32,
    swap_microseconds: AtomicU32,
}
impl FramePacing {
    /// Select the maximum rendering rate; zero follows the display.
    pub fn set_max_fps(&self, fps: u32) {
        self.max_fps.store(fps, Ordering::Relaxed);
    }
    /// The detected refresh rate of the current display, in Hz.
    pub fn refresh_rate(&self) -> f64 {
        f64::from(self.refresh_millihertz.load(Ordering::Relaxed)) / 1000.0
    }
    /// Requested rendering rate, capped by the current display.
    pub fn target_fps(&self) -> f64 {
        f64::from(self.target_millihertz()) / 1000.0
    }
    /// Whether a native display callback currently owns rendering cadence.
    pub fn display_clock_active(&self) -> bool {
        self.display_clock.load(Ordering::Acquire)
    }
    /// Time spent in the last native buffer swap, in milliseconds.
    pub fn swap_ms(&self) -> f64 {
        f64::from(self.swap_microseconds.load(Ordering::Relaxed)) / 1000.0
    }
    pub(crate) fn record_swap(&self, seconds: f64) {
        self.swap_microseconds
            .store((seconds * 1e6) as u32, Ordering::Relaxed);
    }
    pub(crate) fn set_refresh(&self, millihertz: u32) {
        self.refresh_millihertz.store(millihertz, Ordering::Relaxed);
    }
    pub(crate) fn target_millihertz(&self) -> u32 {
        let display = self.refresh_millihertz.load(Ordering::Relaxed).max(1);
        match self.max_fps.load(Ordering::Relaxed) {
            0 => display,
            fps => fps.saturating_mul(1000).min(display),
        }
    }
}
