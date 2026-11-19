# Rendering and performance validation

MORI keeps input collection independent of its rendering cadence. The normal
path performs no GPU timer-query readback, synchronization or screenshot capture.

## Display scheduling

Rendering defaults to the current monitor's refresh rate. General settings can
select a lower maximum; project timing and video export FPS are independent.
The preference is persisted, imported, exported and restored with other settings.

The local eframe 0.31.1 integration gates native redraws before app updates and
painting, while continuing to dispatch every window/input event. It coalesces
repaint requests, not input samples. The OpenGL backend is retained. On macOS, an active display clock pairs with
nonblocking GL swaps so the UI/input thread does not wait for a second vblank.
Other platforms retain native swap synchronization.

On macOS, CoreVideo display callbacks release drawing slots. The clock follows
the window's monitor and selects display ticks for lower requested rates. Rates
that do not divide the display rate use distributed ticks. A callback wakes the
UI only for pending work. On other platforms, native swap synchronization is
retained with monotonic event-loop deadlines that do not accumulate timer drift.
A missed slot skips ahead rather than queuing old frames. There is no sleep in
the input or paint path. Interaction and playback continue requesting frames;
static views leave the UI event loop asleep.

macOS drawing samples are collected at the AppKit event monitor with their own
position, pressure and tilt. They are consumed in order at drawing time, including
stationary pressure changes. The brush cursor uses the latest observed position.
Normal winit event delivery remains available for UI controls. Other platforms
consume the full egui input batch.

Upstream provenance, licenses and exact integration points are documented in
[vendor/eframe/MORI-PATCH.md](../vendor/eframe/MORI-PATCH.md).

## GPU work

- Ordinary black/white ink masks use R16F, including their four-sample render
  buffer. This retains the same coverage precision as RGBA16F with one quarter
  the channels. Colored composition and blur direction/coverage stay RGBA16F.
- Multisample buffers are allocated only when their drawing path is used.
- Completed scene textures are retained directly, avoiding a full-canvas copy
  after every edited frame.
- Layer damage tracking, retained scenes and composed onion images remain in
  use. Hovering over unchanged content does not rebuild stroke geometry.
- Brush geometry, anti-aliasing, canvas resolution and the 32-sample directional
  blur kernel are retained. Effects use the ordinary page rendering bounds.

The GPU smoke suite compares R16F masks against RGBA16F masks pixel for pixel
through pressure, marker orientation, blending, local blur and every blur style.
It also checks cached rendering against fresh renders and verifies that unchanged
six-onion views redraw no strokes. Export and UI workflows have separate checks.

## Reproduce

```sh
cargo test --locked
cargo clippy --locked --all-targets -- -D warnings
cargo build --release --locked
cargo test --manifest-path vendor/eframe/Cargo.toml --no-default-features --features glow,x11 --lib
MORI_BINARY=target/release/mori xvfb-run -a -s '-screen 0 1440x960x24' python3 scripts/ui-pacing-smoke.py
MORI_PREFERENCES_PATH=/tmp/mori-smoke-preferences.json xvfb-run -a -s '-screen 0 1440x960x24' target/release/mori --smoke artifacts/gpu
```

The pacing check exercises default 60 Hz and selected 30 FPS, verifies that
pointer-event throughput exceeds rendering throughput, and checks idle CPU use.
Other UI smoke scripts cover drawing, editing frames, erasing and playback.

On Linux with an EGL-capable GPU, measure renderer work without window/vsync:

```sh
python3 scripts/render-benchmark.py --verify --size 2880x1206 --output artifacts/renderer.json
```

The script uses the repository's Rust sources in a temporary measurement library,
selects a real NVIDIA EGL device by default (`--vendor` selects another driver),
and writes JSON with CPU and GPU p95/max times. Each workload has 15 warmup frames
and 60 recorded frames. GPU times use driver timer queries; waiting for their
results happens only in this offline script. Workloads cover blank and retained
views, 1200-unit round/marker brushes, liquify, dense liquify, movement, directional
and chromatic blur, and six distinct onion drawings.

For native interaction testing, use `./scripts/run.sh --debug-fps --trace-frames`.
Canvas FPS counts drawing callbacks; it does not measure physical scanout.
UI pointer counts egui movement events, while Canvas samples counts movement /
pressure samples consumed by the actual drawing path. CPU
submission time excludes swap waiting, which is displayed separately. Native
macOS frame pacing and input-to-screen latency require a Mac test; headless GPU
throughput and Apple Silicon compilation do not establish those properties.
