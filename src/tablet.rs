//! Native tablet event decoding. Each NSEvent accessor is used only on the
//! event types for which AppKit defines it; proximity has no subtype/pressure.
#[cfg(any(target_os = "macos", test))]
#[derive(Clone, Copy, Debug, PartialEq)]
struct PenState {
    active: bool,
    pressure: f32,
    angle: f32,
}
#[cfg(any(target_os = "macos", test))]
impl Default for PenState {
    fn default() -> Self {
        Self {
            active: false,
            pressure: 1.0,
            angle: 0.0,
        }
    }
}
#[cfg(any(target_os = "macos", test))]
trait TabletEvent {
    fn kind(&self) -> usize;
    fn subtype(&self) -> i16;
    fn pressure(&self) -> f32;
    fn entering(&self) -> bool;
    fn tilt(&self) -> [f32; 2];
}
#[cfg(any(target_os = "macos", test))]
fn tilt_angle([x, y]: [f32; 2]) -> f32 {
    if x * x + y * y > 1e-8 {
        (-y).atan2(x)
    } else {
        0.0
    }
}
#[cfg(any(target_os = "macos", test))]
fn decode(event: &impl TabletEvent) -> Option<PenState> {
    match event.kind() {
        24 => Some(if event.entering() {
            PenState {
                active: true,
                pressure: 0.0,
                angle: 0.0,
            }
        } else {
            PenState::default()
        }),
        23 => Some(PenState {
            active: true,
            pressure: event.pressure().clamp(0.0, 1.0),
            angle: tilt_angle(event.tilt()),
        }),
        1 | 2 | 5 | 6 => match event.subtype() {
            1 => Some(PenState {
                active: true,
                pressure: event.pressure().clamp(0.0, 1.0),
                angle: tilt_angle(event.tilt()),
            }),
            2 => Some(if event.entering() {
                PenState {
                    active: true,
                    pressure: 0.0,
                    angle: 0.0,
                }
            } else {
                PenState::default()
            }),
            _ => Some(PenState::default()),
        },
        _ => None,
    }
}
#[cfg(target_os = "macos")]
mod mac {
    use super::*;
    use block2::RcBlock;
    use objc2::{rc::Retained, runtime::AnyObject};
    use objc2_app_kit::{NSEvent, NSEventMask};
    use std::{cell::Cell, ptr::NonNull, rc::Rc};
    impl TabletEvent for NSEvent {
        fn kind(&self) -> usize {
            self.r#type().0
        }
        fn subtype(&self) -> i16 {
            self.subtype().0
        }
        fn pressure(&self) -> f32 {
            self.pressure()
        }
        fn entering(&self) -> bool {
            self.isEnteringProximity()
        }
        fn tilt(&self) -> [f32; 2] {
            let tilt = self.tilt();
            [tilt.x as f32, tilt.y as f32]
        }
    }
    pub struct Tablet {
        monitor: Option<Retained<AnyObject>>,
        state: Rc<Cell<PenState>>,
        _handler: RcBlock<dyn Fn(NonNull<NSEvent>) -> *mut NSEvent>,
    }
    impl Tablet {
        pub fn new(ctx: eframe::egui::Context) -> Self {
            let state = Rc::new(Cell::new(PenState::default()));
            let observed = state.clone();
            let handler: RcBlock<dyn Fn(NonNull<NSEvent>) -> *mut NSEvent> =
                RcBlock::new(move |event: NonNull<NSEvent>| {
                    // AppKit supplies a live NSEvent and runs this local monitor on the UI thread.
                    let event_ref = unsafe { event.as_ref() };
                    if let Some(next) = decode(event_ref) {
                        observed.set(next);
                        ctx.request_repaint();
                    }
                    event.as_ptr()
                });
            let mask = NSEventMask::LeftMouseDown
                | NSEventMask::LeftMouseUp
                | NSEventMask::MouseMoved
                | NSEventMask::LeftMouseDragged
                | NSEventMask::TabletPoint
                | NSEventMask::TabletProximity;
            // Returning the original pointer leaves normal winit event delivery intact.
            let monitor =
                unsafe { NSEvent::addLocalMonitorForEventsMatchingMask_handler(mask, &handler) };
            Self {
                monitor,
                state,
                _handler: handler,
            }
        }
        pub fn angle(&self) -> Option<f32> {
            let state = self.state.get();
            state.active.then_some(state.angle)
        }
        pub fn pressure(&self) -> Option<f32> {
            let state = self.state.get();
            state.active.then_some(state.pressure)
        }
    }
    impl Drop for Tablet {
        fn drop(&mut self) {
            if let Some(monitor) = &self.monitor {
                unsafe {
                    NSEvent::removeMonitor(monitor);
                }
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
    pub fn angle(&self) -> Option<f32> {
        None
    }
    pub fn new(_ctx: eframe::egui::Context) -> Self {
        Self
    }
    pub fn pressure(&self) -> Option<f32> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Event {
        kind: usize,
        subtype: Option<i16>,
        pressure: Option<f32>,
        entering: Option<bool>,
    }
    impl TabletEvent for Event {
        fn kind(&self) -> usize {
            self.kind
        }
        fn subtype(&self) -> i16 {
            self.subtype.expect("Invalid NSEvent subtype accessor")
        }
        fn pressure(&self) -> f32 {
            self.pressure.expect("Invalid NSEvent pressure accessor")
        }
        fn entering(&self) -> bool {
            self.entering.expect("Invalid NSEvent proximity accessor")
        }
        fn tilt(&self) -> [f32; 2] {
            assert!(
                self.kind == 23 || (matches!(self.kind, 1 | 2 | 5 | 6) && self.subtype == Some(1)),
                "Invalid tilt accessor"
            );
            [0.3, 0.4]
        }
    }
    #[test]
    fn huion_proximity_enter_and_exit_never_read_subtype_or_pressure() {
        for entering in [true, false] {
            let state = decode(&Event {
                kind: 24,
                subtype: None,
                pressure: None,
                entering: Some(entering),
            })
            .unwrap();
            assert_eq!(state.active, entering);
        }
    }
    #[test]
    fn tablet_points_and_stationary_pressure_do_not_read_mouse_subtype() {
        for pressure in [0.0, 0.1, 0.5, 1.0, 0.0] {
            let state = decode(&Event {
                kind: 23,
                subtype: None,
                pressure: Some(pressure),
                entering: None,
            })
            .unwrap();
            assert_eq!(state.pressure, pressure);
        }
    }
    #[test]
    fn real_mouse_does_not_read_tablet_only_properties() {
        let state = decode(&Event {
            kind: 1,
            subtype: Some(0),
            pressure: None,
            entering: None,
        })
        .unwrap();
        assert_eq!(state, PenState::default());
        let state = decode(&Event {
            kind: 6,
            subtype: Some(1),
            pressure: Some(0.3),
            entering: None,
        })
        .unwrap();
        assert_eq!(state.pressure, 0.3);
    }
    #[test]
    fn unrelated_events_do_not_read_any_specialized_properties() {
        assert!(decode(&Event {
            kind: 10,
            subtype: None,
            pressure: None,
            entering: None
        })
        .is_none());
    }
}

#[cfg(test)]
mod tilt_tests {
    use super::*;
    #[test]
    fn tablet_tilt_uses_screen_coordinates_and_stays_continuous_at_zero() {
        assert_eq!(tilt_angle([1.0, 0.0]), 0.0);
        assert_eq!(tilt_angle([0.0, 1.0]), -std::f32::consts::FRAC_PI_2);
        assert_eq!(tilt_angle([0.0, 0.0]), 0.0);
    }
}
