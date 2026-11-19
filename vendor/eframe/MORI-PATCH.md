# Local eframe integration

Upstream: eframe 0.31.1, egui commit `1669e52a7ccfc3489c1b0999b9ed48894a0b3887`.
The crate is retained under its upstream MIT / Apache-2.0 licenses.

MORI needs a single scheduling gate before native rendering. Upstream 0.31.1
exposes repaint requests but no display-clock hook, so application-level sleeps
would block input while application-level repaint timers could not cap redraws
requested by the window integration.

Local additions:

- `NativeOptions::frame_pacing` and shared `FramePacing` controls/observations.
- A native redraw gate. Window/input events are always dispatched normally;
  pending redraw requests coalesce until the next rendering slot.
- macOS CoreVideo display callbacks, retargeted to the window's current monitor.
  Lower requested rates select display ticks; idle clocks do not wake the UI.
- A monotonic, drift-free event-loop deadline for other platforms, retaining
  the existing native swap synchronization. No sleeps in input or paint.
- Nonblocking GL swaps when the macOS display clock is active, avoiding a
  second vblank wait on the UI/input thread. Other platforms retain vsync.
- Measurement of the native swap wait, which upstream excludes from CPU usage.

Other eframe behavior and the OpenGL backend are retained. The added scheduling
logic and controls have unit tests in `src/native/frame_clock.rs`.
