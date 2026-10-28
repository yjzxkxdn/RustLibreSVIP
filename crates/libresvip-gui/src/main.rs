//! RustLibreSVIP desktop GUI (based on LibreSVIP 2.9.2 and UtaFormatix3).
//!
//! A native egui/eframe rebuild of the Python project's Qt Quick interface:
//! same layout, same Material look, same conversion behaviour.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod convert;
mod deviations;
mod i18n;
mod icons;
mod platform;
mod state;
mod theme;
mod ui;
mod widgets;

use std::path::PathBuf;

fn window_icon() -> Option<eframe::egui::IconData> {
    eframe::icon_data::from_png_bytes(include_bytes!("../assets/icon.png")).ok()
}

/// Which renderer to try first.
///
/// `wgpu` is the default on Linux: the OpenGL path can draw the window but
/// leave every glyph invisible on some Mesa/driver combinations, and nothing in
/// the app can detect that before the first frame is on screen. Elsewhere
/// `glow` stays the default. `--renderer glow|wgpu` and the `LIBRESVIP_RENDERER`
/// environment variable override the choice.
fn preferred_renderer(requested: Option<&str>) -> eframe::Renderer {
    match requested.map(|value| value.trim().to_ascii_lowercase()).as_deref() {
        Some("glow") => eframe::Renderer::Glow,
        Some("wgpu") => eframe::Renderer::Wgpu,
        _ => {
            if cfg!(target_os = "linux") {
                eframe::Renderer::Wgpu
            } else {
                eframe::Renderer::Glow
            }
        }
    }
}

fn alternate(renderer: eframe::Renderer) -> eframe::Renderer {
    match renderer {
        eframe::Renderer::Glow => eframe::Renderer::Wgpu,
        eframe::Renderer::Wgpu => eframe::Renderer::Glow,
    }
}

fn launch(renderer: eframe::Renderer, files: Vec<PathBuf>) -> eframe::Result<()> {
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
        renderer,
        ..Default::default()
    };
    eprintln!("libresvip-gui: starting with the {renderer:?} renderer");
    eframe::run_native(
        "LibreSVIP",
        options,
        Box::new(move |cc| Ok(Box::new(app::App::new(cc, files)))),
    )
}

fn main() -> eframe::Result<()> {
    // Options can sit before or between file names, so walk the arguments
    // instead of filtering them: `--renderer wgpu song.ustx` must keep the file.
    let mut requested: Option<String> = std::env::var("LIBRESVIP_RENDERER").ok();
    let mut want_x11 = false;
    let mut files: Vec<PathBuf> = Vec::new();
    let mut args = std::env::args_os().skip(1);
    while let Some(arg) = args.next() {
        let text = arg.to_string_lossy().to_string();
        if let Some(value) = text.strip_prefix("--renderer=") {
            requested = Some(value.to_owned());
            continue;
        }
        if text == "--renderer" {
            requested = args.next().map(|value| value.to_string_lossy().to_string());
            continue;
        }
        if text == "--x11" {
            want_x11 = true;
            continue;
        }
        let path = PathBuf::from(&arg);
        if path.is_file() {
            files.push(path);
        }
    }

    // Wayland never delivers file drops to winit (emilk/egui#1563), so `--x11`
    // relaunches the window on X11/XWayland where drag and drop works.
    if want_x11 || std::env::var_os("LIBRESVIP_GUI_X11").is_some() {
        if crate::platform::prefer_x11() {
            eprintln!(
                "libresvip-gui: WAYLAND_DISPLAY cleared; running through X11 so file \
                 drag and drop works"
            );
        }
    }

    let renderer = preferred_renderer(requested.as_deref());
    match launch(renderer, files.clone()) {
        Ok(()) => Ok(()),
        Err(error) => {
            // Adapter creation fails outright when a backend is missing, so this
            // retry covers "no Vulkan on this box" rather than the blank-text
            // case the default above is about.
            let fallback = alternate(renderer);
            eprintln!("libresvip-gui: {renderer:?} failed ({error}); retrying with {fallback:?}");
            launch(fallback, files)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_override_beats_the_platform_default() {
        assert!(matches!(
            preferred_renderer(Some("wgpu")),
            eframe::Renderer::Wgpu
        ));
        assert!(matches!(
            preferred_renderer(Some("GLOW")),
            eframe::Renderer::Glow
        ));
        assert!(matches!(
            preferred_renderer(Some(" glow ")),
            eframe::Renderer::Glow
        ));
        // Junk falls back to the platform default instead of failing.
        let fallback = preferred_renderer(Some("vulkan"));
        assert!(matches!(
            fallback,
            eframe::Renderer::Glow | eframe::Renderer::Wgpu
        ));
        assert!(matches!(
            alternate(eframe::Renderer::Glow),
            eframe::Renderer::Wgpu
        ));
        assert!(matches!(
            alternate(eframe::Renderer::Wgpu),
            eframe::Renderer::Glow
        ));
    }
}
