//! egui supplies touch force where available. AppKit's local event monitor adds
//! native tablet pressure on macOS without changing event delivery to winit.
#[cfg(target_os = "macos")]
#[allow(unexpected_cfgs)]
mod mac {
    use objc::{class, msg_send, runtime::Object, sel, sel_impl};
    use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
    static ACTIVE: AtomicBool = AtomicBool::new(false);
    static PRESSURE: AtomicU32 = AtomicU32::new(1.0f32.to_bits());
    pub struct Tablet {
        monitor: *mut Object,
    }
    impl Tablet {
        pub fn new(ctx: eframe::egui::Context) -> Self {
            unsafe {
                let handler = block::ConcreteBlock::new(move |event: *mut Object| -> *mut Object {
                    let kind: usize = msg_send![event, type];
                    let subtype: i16 = msg_send![event, subtype];
                    if kind == 23 || subtype == 1 {
                        let value: f32 = msg_send![event, pressure];
                        PRESSURE.store(value.clamp(0.0, 1.0).to_bits(), Ordering::Relaxed);
                        ACTIVE.store(true, Ordering::Relaxed);
                    } else if kind == 1 || kind == 5 || kind == 6 {
                        ACTIVE.store(false, Ordering::Relaxed);
                    }
                    ctx.request_repaint();
                    event
                })
                .copy();
                let mask: usize = (1 << 1) | (1 << 2) | (1 << 5) | (1 << 6) | (1 << 23) | (1 << 24);
                let monitor: *mut Object = msg_send![class!(NSEvent),addLocalMonitorForEventsMatchingMask:mask handler:&*handler];
                // NSEvent retains the copied block until removeMonitor.
                Self { monitor }
            }
        }
        pub fn pressure(&self) -> Option<f32> {
            ACTIVE
                .load(Ordering::Relaxed)
                .then(|| f32::from_bits(PRESSURE.load(Ordering::Relaxed)))
        }
    }
    impl Drop for Tablet {
        fn drop(&mut self) {
            unsafe {
                let _: () = msg_send![class!(NSEvent),removeMonitor:self.monitor];
            }
        }
    }
}
#[cfg(target_os = "macos")]
pub use mac::Tablet;
#[cfg(not(target_os = "macos"))]
pub struct Tablet;
#[cfg(not(target_os = "macos"))]
impl Tablet {
    pub fn new(_ctx: eframe::egui::Context) -> Self {
        Self
    }
    pub fn pressure(&self) -> Option<f32> {
        None
    }
}
