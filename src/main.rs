mod app;
mod audio;
mod curve;
mod export;
mod i18n;
mod model;
mod native_menu;
mod preferences;
mod project;
mod render;
mod tablet;

fn main() -> eframe::Result {
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
        ..Default::default()
    };
    eframe::run_native(
        "MORI",
        options,
        Box::new(|cc| Ok(Box::new(app::Studio::new(cc)))),
    )
}
