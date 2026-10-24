//! RustLibreSVIP desktop GUI (based on LibreSVIP 2.9.2 and UtaFormatix3).
//!
//! A native egui/eframe rebuild of the Python project's Qt Quick interface:
//! same layout, same Material look, same conversion behaviour.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod convert;
mod i18n;
mod icons;
mod platform;
mod state;
mod theme;
mod ui;
mod widgets;

fn window_icon() -> Option<eframe::egui::IconData> {
    eframe::icon_data::from_png_bytes(include_bytes!("../assets/icon.png")).ok()
}

fn main() -> eframe::Result<()> {
    // Files given on the command line are added to the task list, so the GUI
    // can be used as the target of "Open with" / drag-and-drop from a shell.
    let initial_files: Vec<std::path::PathBuf> = std::env::args_os()
        .skip(1)
        .map(std::path::PathBuf::from)
        .filter(|path| path.is_file())
        .collect();
    let mut viewport = eframe::egui::ViewportBuilder::default()
        .with_inner_size([1200.0, 800.0])
        .with_min_inner_size([700.0, 600.0])
        .with_decorations(false)
        .with_title("RustLibreSVIP")
        .with_app_id("libresvip-gui");
    if let Some(icon) = window_icon() {
        viewport = viewport.with_icon(icon);
    }
    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };
    eframe::run_native(
        "LibreSVIP",
        options,
        Box::new(move |cc| Ok(Box::new(app::App::new(cc, initial_files)))),
    )
}
