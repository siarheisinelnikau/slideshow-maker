#![cfg_attr(windows, windows_subsystem = "windows")]
mod ui;

fn main() -> eframe::Result {
    eframe::run_native(
        "SlideshowMaker",
        eframe::NativeOptions {
            viewport: eframe::egui::ViewportBuilder::default()
                .with_inner_size([1180.0, 780.0])
                .with_min_inner_size([900.0, 640.0]),
            ..Default::default()
        },
        Box::new(|cc| Ok(Box::new(ui::Editor::new(cc)))),
    )
}
