mod app;
mod audio;
mod curve;
mod erase;
mod export;
mod i18n;
mod model;
mod native_menu;
mod preferences;
mod project;
mod render;
mod tablet;
mod timing;

fn main() -> eframe::Result {
    timing::initialize();
    let pacing = std::sync::Arc::new(eframe::FramePacing::default());
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title("MORI")
            .with_decorations(true)
            .with_icon(
                eframe::icon_data::from_png_bytes(include_bytes!("../assets/icon.png"))
                    .expect("Bundled app icon"),
            )
            .with_inner_size([1440.0, 960.0])
            .with_min_inner_size([900.0, 620.0]),
        renderer: eframe::Renderer::Glow,
        multisampling: 0,
        vsync: true,
        frame_pacing: Some(pacing.clone()),
        ..Default::default()
    };
    eframe::run_native(
        "MORI",
        options,
        Box::new(move |cc| Ok(Box::new(app::Studio::new(cc, pacing)))),
    )
}
