//! Gate drawing, never input. macOS uses CoreVideo display callbacks. Other
//! platforms retain swap synchronization and schedule against a monotonic grid.
use super::winit_integration::UserEvent;
use crate::FramePacing;
use std::{
    sync::Arc,
    time::{Duration, Instant},
};
use winit::{event_loop::EventLoopProxy, window::Window};

pub(super) struct FrameClock {
    pub controls: Arc<FramePacing>,
    next: Option<Instant>,
    monitor_check: Option<Instant>,
    #[cfg(target_os = "macos")]
    display: Option<mac::DisplayLink>,
}
impl FrameClock {
    pub fn new(controls: Arc<FramePacing>, _proxy: EventLoopProxy<UserEvent>) -> Self {
        controls.set_refresh(60_000);
        Self {
            #[cfg(target_os = "macos")]
            display: mac::DisplayLink::new(controls.clone(), _proxy),
            controls,
            next: None,
            monitor_check: None,
        }
    }
    pub fn monitor_changed(&mut self) {
        self.monitor_check = None;
    }
    pub fn update_monitor(&mut self, window: &Window, now: Instant) {
        if self
            .monitor_check
            .is_some_and(|t| now.duration_since(t) < Duration::from_secs(1))
        {
            return;
        }
        self.monitor_check = Some(now);
        if let Some(monitor) = window.current_monitor() {
            if let Some(rate) = monitor.refresh_rate_millihertz() {
                self.controls.set_refresh(rate);
            }
            #[cfg(target_os = "macos")]
            if let Some(display) = &self.display {
                use winit::platform::macos::MonitorHandleExtMacOS;
                display.set_display(monitor.native_id());
            }
        }
    }
    pub fn display_driven(&self) -> bool {
        #[cfg(target_os = "macos")]
        {
            self.display.is_some()
        }
        #[cfg(not(target_os = "macos"))]
        {
            false
        }
    }
    /// None is a display-clock wait; Some is a native event-loop deadline.
    pub fn deadline(&self, now: Instant) -> Option<Instant> {
        #[cfg(target_os = "macos")]
        if let Some(display) = &self.display {
            if display.ready() {
                return Some(now);
            }
            display.request();
            return None;
        }
        Some(self.next.unwrap_or(now))
    }
    pub fn consume(&mut self, now: Instant) {
        #[cfg(target_os = "macos")]
        if let Some(display) = &self.display {
            display.consume();
            return;
        }
        let interval =
            Duration::from_secs_f64(1000.0 / f64::from(self.controls.target_millihertz()));
        self.next = Some(next_slot(self.next.unwrap_or(now), now, interval));
    }
}
#[cfg(any(target_os = "macos", test))]
fn display_slot(phase: u64, target: u32, refresh: u32) -> (u64, bool) {
    let refresh = u64::from(refresh.max(1));
    let phase = phase + u64::from(target).min(refresh);
    (phase % refresh, phase >= refresh)
}
fn next_slot(previous: Instant, now: Instant, interval: Duration) -> Instant {
    if previous > now {
        return previous;
    }
    let slots = now.duration_since(previous).as_nanos() / interval.as_nanos() + 1;
    previous + interval.mul_f64(slots as f64)
}

#[cfg(target_os = "macos")]
#[allow(unsafe_code)]
mod mac {
    use super::*;
    use std::{
        ffi::c_void,
        ptr,
        sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering},
    };
    #[repr(C)]
    struct SmpteTime {
        subframes: i16,
        divisor: i16,
        counter: u32,
        kind: u32,
        flags: u32,
        hours: i16,
        minutes: i16,
        seconds: i16,
        frames: i16,
    }
    #[repr(C)]
    struct TimeStamp {
        version: u32,
        scale: i32,
        time: i64,
        host: u64,
        scalar: f64,
        period: i64,
        smpte: SmpteTime,
        flags: u64,
        reserved: u64,
    }
    type Callback = unsafe extern "C" fn(
        *mut c_void,
        *const TimeStamp,
        *const TimeStamp,
        u64,
        *mut u64,
        *mut c_void,
    ) -> i32;
    #[link(name = "CoreVideo", kind = "framework")]
    unsafe extern "C" {
        fn CVDisplayLinkCreateWithCGDisplay(display: u32, link: *mut *mut c_void) -> i32;
        fn CVDisplayLinkSetOutputCallback(
            link: *mut c_void,
            callback: Callback,
            context: *mut c_void,
        ) -> i32;
        fn CVDisplayLinkSetCurrentCGDisplay(link: *mut c_void, display: u32) -> i32;
        fn CVDisplayLinkStart(link: *mut c_void) -> i32;
        fn CVDisplayLinkStop(link: *mut c_void) -> i32;
        fn CVDisplayLinkRelease(link: *mut c_void);
    }
    #[link(name = "CoreGraphics", kind = "framework")]
    unsafe extern "C" {
        fn CGMainDisplayID() -> u32;
    }
    struct State {
        controls: Arc<FramePacing>,
        proxy: EventLoopProxy<UserEvent>,
        pending: AtomicBool,
        ready: AtomicBool,
        phase: AtomicU64,
    }
    pub(super) struct DisplayLink {
        link: *mut c_void,
        state: Box<State>,
        display: AtomicU32,
    }
    unsafe extern "C" fn tick(
        _: *mut c_void,
        _: *const TimeStamp,
        output: *const TimeStamp,
        _: u64,
        _: *mut u64,
        context: *mut c_void,
    ) -> i32 {
        // CoreVideo owns the callback thread. No UI, GL or explicit waits here.
        let state = unsafe { &*context.cast::<State>() };
        if !output.is_null() {
            let stamp = unsafe { &*output };
            if stamp.flags & (1 << 3) != 0 && stamp.scale > 0 && stamp.period > 0 {
                state.controls.set_refresh(
                    (f64::from(stamp.scale) * 1000.0 / stamp.period as f64).round() as u32,
                );
            }
        }
        let (phase, selected) = display_slot(
            state.phase.load(Ordering::Relaxed),
            state.controls.target_millihertz(),
            state.controls.refresh_millihertz.load(Ordering::Relaxed),
        );
        state.phase.store(phase, Ordering::Relaxed);
        if selected
            && state.pending.swap(false, Ordering::AcqRel)
            && !state.ready.swap(true, Ordering::AcqRel)
        {
            let _ = state.proxy.send_event(UserEvent::DisplayTick);
        }
        0
    }
    impl DisplayLink {
        pub fn new(controls: Arc<FramePacing>, proxy: EventLoopProxy<UserEvent>) -> Option<Self> {
            let display = unsafe { CGMainDisplayID() };
            let mut result = Self {
                link: ptr::null_mut(),
                state: Box::new(State {
                    controls,
                    proxy,
                    pending: AtomicBool::new(false),
                    ready: AtomicBool::new(false),
                    phase: AtomicU64::new(0),
                }),
                display: AtomicU32::new(display),
            };
            let created = unsafe { CVDisplayLinkCreateWithCGDisplay(display, &mut result.link) };
            if created != 0 || result.link.is_null() {
                log::error!("Cannot create macOS display clock ({created}); using native swap synchronization");
                return None;
            }
            let callback = unsafe {
                CVDisplayLinkSetOutputCallback(
                    result.link,
                    tick,
                    (&mut *result.state as *mut State).cast(),
                )
            };
            let started = if callback == 0 {
                unsafe { CVDisplayLinkStart(result.link) }
            } else {
                callback
            };
            if started != 0 {
                log::error!("Cannot start macOS display clock ({started}); using native swap synchronization");
                return None;
            }
            result
                .state
                .controls
                .display_clock
                .store(true, Ordering::Release);
            Some(result)
        }
        pub fn request(&self) {
            self.state.pending.store(true, Ordering::Release);
        }
        pub fn ready(&self) -> bool {
            self.state.ready.load(Ordering::Acquire)
        }
        pub fn consume(&self) {
            self.state.ready.store(false, Ordering::Release);
        }
        pub fn set_display(&self, id: u32) {
            if self.display.load(Ordering::Relaxed) != id {
                let status = unsafe { CVDisplayLinkSetCurrentCGDisplay(self.link, id) };
                if status == 0 {
                    self.display.store(id, Ordering::Relaxed);
                } else {
                    log::error!("Cannot move display clock to display {id}: {status}");
                }
            }
        }
    }
    impl Drop for DisplayLink {
        fn drop(&mut self) {
            self.state
                .controls
                .display_clock
                .store(false, Ordering::Release);
            if !self.link.is_null() {
                // Stop the callback before releasing its context.
                unsafe {
                    CVDisplayLinkStop(self.link);
                    CVDisplayLinkRelease(self.link);
                }
            }
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn lower_rates_select_display_ticks_without_dropping_or_bursting() {
        for (refresh, target, expected) in [
            (120_000, 60_000, 600),
            (60_000, 60_000, 1200),
            (144_000, 60_000, 500),
            (120_000, 90_000, 900),
        ] {
            let mut phase = 0;
            let mut frames = 0;
            for _ in 0..1200 {
                let (next, selected) = display_slot(phase, target, refresh);
                phase = next;
                frames += usize::from(selected);
            }
            assert_eq!(frames, expected);
        }
    }
    #[test]
    fn grid_does_not_drift_or_replay_missed_frames() {
        let origin = Instant::now();
        let interval = Duration::from_nanos(16_666_667);
        let mut next = origin;
        for i in 0..1000 {
            let actual = origin + interval * i + Duration::from_micros(150);
            next = next_slot(next, actual, interval);
            assert_eq!(next, origin + interval * (i + 1));
        }
        let late = origin + interval * 2000;
        assert_eq!(next_slot(next, late, interval), late + interval);
    }
    #[test]
    fn requested_rate_never_exceeds_current_display() {
        let settings = FramePacing::default();
        settings.set_refresh(60_000);
        assert_eq!(settings.target_fps(), 60.0);
        settings.set_max_fps(120);
        assert_eq!(settings.target_fps(), 60.0);
        settings.set_refresh(120_000);
        assert_eq!(settings.target_fps(), 120.0);
        settings.set_max_fps(30);
        assert_eq!(settings.target_fps(), 30.0);
        settings.set_refresh(59_940);
        settings.set_max_fps(0);
        assert_eq!(settings.target_fps(), 59.94);
    }
}
