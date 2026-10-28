//! Material Design palette and egui style used by the desktop GUI.
//!
//! Colours and spacing constants mirror `libresvip/res/qml/components/theme.qml`
//! and the Qt Quick Material style the Python GUI uses (`Material.accent` is
//! indigo, `QT_QUICK_CONTROLS_MATERIAL_VARIANT=Dense`).

use std::sync::Arc;

use eframe::egui::{
    self, Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Margin, Stroke,
    TextStyle, Visuals,
};

/// `Theme.spacingXS`
pub const SPACING_XS: f32 = 8.0;
/// `Theme.spacingS`
pub const SPACING_S: f32 = 12.0;
/// `Theme.spacingM`
pub const SPACING_M: f32 = 16.0;
/// `Theme.spacingL`
pub const SPACING_L: f32 = 24.0;

pub const TITLEBAR_HEIGHT: f32 = 32.0;
pub const BORDER_RADIUS: u8 = 4;

/// Resolved colour scheme for one frame.
#[derive(Clone, Copy, Debug)]
pub struct Palette {
    pub dark: bool,
    /// Window background (`Material.background`).
    pub bg: Color32,
    /// Card / panel background.
    pub surface: Color32,
    /// `Theme.colorBorder`
    pub border: Color32,
    /// Primary text (`Material.foreground`).
    pub text: Color32,
    /// `Theme.colorMutedText`
    pub muted: Color32,
    /// `Material.accent` (indigo).
    pub primary: Color32,
    /// Lighter primary used for hover.
    pub primary_hover: Color32,
    pub on_primary: Color32,
    /// `Theme.colorSuccess`
    pub success: Color32,
    /// `Theme.colorWarning`
    pub warning: Color32,
    /// `Theme.colorError`
    pub error: Color32,
    /// `Theme.colorInfo`
    pub info: Color32,
    /// Hover overlay for flat buttons.
    pub hover: Color32,
    /// Switch track when off.
    pub switch_off: Color32,
    /// Switch thumb when off.
    pub switch_thumb: Color32,
    /// Row hover in lists.
    pub row_hover: Color32,
    /// Text on top of the primary colour.
    pub shadow: Color32,
}

impl Palette {
    pub fn light() -> Self {
        Self {
            dark: false,
            bg: Color32::from_rgb(0xFA, 0xFA, 0xFA),
            surface: Color32::from_rgb(0xFF, 0xFF, 0xFF),
            border: Color32::from_rgb(0xE0, 0xE0, 0xE0),
            text: Color32::from_rgb(0x1F, 0x1F, 0x1F),
            muted: Color32::from_rgb(0x75, 0x75, 0x75),
            primary: Color32::from_rgb(0x3F, 0x51, 0xB5),
            primary_hover: Color32::from_rgb(0x5C, 0x6B, 0xC0),
            on_primary: Color32::from_rgb(0xFF, 0xFF, 0xFF),
            success: Color32::from_rgb(0x4C, 0xAF, 0x50),
            warning: Color32::from_rgb(0xFF, 0x98, 0x00),
            error: Color32::from_rgb(0xF4, 0x43, 0x36),
            info: Color32::from_rgb(0x21, 0x96, 0xF3),
            hover: Color32::from_rgba_premultiplied(0, 0, 0, 12),
            switch_off: Color32::from_rgba_premultiplied(0, 0, 0, 90),
            switch_thumb: Color32::from_rgb(0xFA, 0xFA, 0xFA),
            row_hover: Color32::from_rgba_premultiplied(0, 0, 0, 10),
            shadow: Color32::from_rgba_premultiplied(0, 0, 0, 40),
        }
    }

    pub fn dark() -> Self {
        Self {
            dark: true,
            bg: Color32::from_rgb(0x12, 0x12, 0x12),
            surface: Color32::from_rgb(0x1E, 0x1E, 0x1E),
            border: Color32::from_rgb(0x42, 0x42, 0x42),
            text: Color32::from_rgb(0xE6, 0xE6, 0xE6),
            muted: Color32::from_rgb(0xBD, 0xBD, 0xBD),
            primary: Color32::from_rgb(0x79, 0x86, 0xCB),
            primary_hover: Color32::from_rgb(0x8F, 0x9A, 0xD6),
            on_primary: Color32::from_rgb(0x12, 0x12, 0x12),
            success: Color32::from_rgb(0x81, 0xC7, 0x84),
            warning: Color32::from_rgb(0xFF, 0xB7, 0x4D),
            error: Color32::from_rgb(0xE5, 0x73, 0x73),
            info: Color32::from_rgb(0x64, 0xB5, 0xF6),
            hover: Color32::from_rgba_premultiplied(255, 255, 255, 16),
            switch_off: Color32::from_rgba_premultiplied(255, 255, 255, 70),
            switch_thumb: Color32::from_rgb(0xE0, 0xE0, 0xE0),
            row_hover: Color32::from_rgba_premultiplied(255, 255, 255, 14),
            shadow: Color32::from_rgba_premultiplied(0, 0, 0, 90),
        }
    }

    /// Body font size (Qt default font is 12-13 px).
    pub fn body() -> f32 {
        13.0
    }

    pub fn small() -> f32 {
        11.5
    }

    pub fn heading() -> f32 {
        20.0
    }

    pub fn border(&self) -> Stroke {
        Stroke::new(1.0_f32, self.border)
    }
}

fn text_styles() -> std::collections::BTreeMap<TextStyle, FontId> {
    use TextStyle::*;
    let mut styles = std::collections::BTreeMap::new();
    styles.insert(Body, FontId::proportional(13.0));
    styles.insert(Button, FontId::proportional(13.0));
    styles.insert(Small, FontId::proportional(11.5));
    styles.insert(Heading, FontId::proportional(20.0));
    styles.insert(Monospace, FontId::monospace(12.0));
    styles
}

fn visuals(pal: &Palette) -> Visuals {
    let mut visuals = if pal.dark {
        Visuals::dark()
    } else {
        Visuals::light()
    };
    let radius = CornerRadius::same(BORDER_RADIUS);
    visuals.dark_mode = pal.dark;
    visuals.override_text_color = Some(pal.text);
    visuals.panel_fill = pal.bg;
    visuals.window_fill = pal.surface;
    visuals.extreme_bg_color = if pal.dark {
        Color32::from_rgb(0x26, 0x26, 0x26)
    } else {
        Color32::from_rgb(0xF5, 0xF5, 0xF5)
    };
    visuals.faint_bg_color = pal.row_hover;
    visuals.hyperlink_color = pal.primary;
    visuals.warn_fg_color = pal.warning;
    visuals.error_fg_color = pal.error;
    visuals.window_stroke = Stroke::new(1.0_f32, pal.border);
    visuals.window_corner_radius = CornerRadius::same(8);
    visuals.menu_corner_radius = CornerRadius::same(4);
    visuals.selection = egui::style::Selection {
        bg_fill: pal.primary.gamma_multiply(0.35),
        stroke: Stroke::new(1.0_f32, pal.primary),
    };

    let widgets = &mut visuals.widgets;
    widgets.noninteractive = egui::style::WidgetVisuals {
        bg_fill: pal.surface,
        weak_bg_fill: pal.surface,
        bg_stroke: Stroke::new(1.0_f32, pal.border),
        corner_radius: radius,
        fg_stroke: Stroke::new(1.0_f32, pal.text),
        expansion: 0.0,
    };
    widgets.inactive = egui::style::WidgetVisuals {
        bg_fill: Color32::TRANSPARENT,
        weak_bg_fill: Color32::TRANSPARENT,
        bg_stroke: Stroke::new(1.0_f32, pal.border),
        corner_radius: radius,
        fg_stroke: Stroke::new(1.0_f32, pal.text),
        expansion: 0.0,
    };
    widgets.hovered = egui::style::WidgetVisuals {
        bg_fill: pal.hover,
        weak_bg_fill: pal.hover,
        bg_stroke: Stroke::new(1.0_f32, pal.primary),
        corner_radius: radius,
        fg_stroke: Stroke::new(1.0_f32, pal.text),
        expansion: 0.0,
    };
    widgets.active = egui::style::WidgetVisuals {
        bg_fill: pal.hover,
        weak_bg_fill: pal.hover,
        bg_stroke: Stroke::new(2.0_f32, pal.primary),
        corner_radius: radius,
        fg_stroke: Stroke::new(1.0_f32, pal.text),
        expansion: 0.0,
    };
    widgets.open = egui::style::WidgetVisuals {
        bg_fill: pal.surface,
        weak_bg_fill: pal.surface,
        bg_stroke: Stroke::new(1.0_f32, pal.border),
        corner_radius: radius,
        fg_stroke: Stroke::new(1.0_f32, pal.text),
        expansion: 0.0,
    };
    visuals
}

/// Applies fonts + widget style to the context.
pub fn apply(ctx: &egui::Context, pal: &Palette) {
    let mut style = egui::Style::default();
    style.visuals = visuals(pal);
    style.text_styles = text_styles();
    style.spacing.item_spacing = egui::vec2(SPACING_XS, SPACING_XS);
    style.spacing.button_padding = egui::vec2(12.0, 6.0);
    style.spacing.interact_size = egui::vec2(40.0, 32.0);
    style.spacing.window_margin = Margin::same(12);
    style.spacing.menu_margin = Margin::same(6);
    style.spacing.indent = 16.0;
    style.spacing.scroll.bar_width = 8.0;
    style.spacing.scroll.floating = true;
    style.animation_time = 0.12;
    let style = std::sync::Arc::new(style);
    // Pin the theme *and* install the same style for both variants: egui
    // otherwise keeps a default style for the inactive theme, which makes
    // windows created after a theme switch use the wrong colours.
    let preference = if pal.dark {
        egui::ThemePreference::Dark
    } else {
        egui::ThemePreference::Light
    };
    ctx.set_theme(preference);
    ctx.set_style_of(egui::Theme::Dark, style.clone());
    ctx.set_style_of(egui::Theme::Light, style);
}

// ---------------------------------------------------------------------------
// Fonts
// ---------------------------------------------------------------------------

/// System fonts that contain CJK glyphs, in order of preference. The Python GUI
/// renders with the system UI font (Microsoft YaHei on Windows).
///
/// The Linux entries start with the single-face SC fonts on purpose: inside
/// `NotoSansCJK-Regular.ttc` face 0 is the Japanese one, whose Han glyphs are
/// correct but shaped for Japanese typography.
const FONT_CANDIDATES: &[&str] = &[
    // Windows
    r"C:\Windows\Fonts\msyh.ttc",
    r"C:\Windows\Fonts\msyh.ttf",
    r"C:\Windows\Fonts\Deng.ttf",
    r"C:\Windows\Fonts\simhei.ttf",
    r"C:\Windows\Fonts\NotoSansSC-VF.ttf",
    r"C:\Windows\Fonts\simsun.ttc",
    // macOS
    r"/System/Library/Fonts/PingFang.ttc",
    r"/System/Library/Fonts/STHeiti Light.ttc",
    r"/Library/Fonts/Arial Unicode.ttf",
    // Linux - Debian/Ubuntu
    r"/usr/share/fonts/opentype/noto/NotoSansCJKsc-Regular.otf",
    r"/usr/share/fonts/opentype/noto/NotoSansSC-Regular.otf",
    r"/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
    r"/usr/share/fonts/truetype/wqy/wqy-microhei.ttc",
    r"/usr/share/fonts/truetype/wqy/wqy-zenhei.ttc",
    r"/usr/share/fonts/truetype/droid/DroidSansFallbackFull.ttf",
    // Linux - Arch
    r"/usr/share/fonts/noto-cjk/NotoSansCJK-Regular.ttc",
    r"/usr/share/fonts/noto-cjk/NotoSansCJKsc-Regular.otf",
    r"/usr/share/fonts/wenquanyi/wqy-microhei/wqy-microhei.ttc",
    r"/usr/share/fonts/adobe-source-han-sans/SourceHanSansSC-Regular.otf",
    // Linux - Fedora / openSUSE / Alpine
    r"/usr/share/fonts/google-noto-cjk/NotoSansCJK-Regular.ttc",
    r"/usr/share/fonts/truetype/noto-cjk/NotoSansCJK-Regular.ttc",
    r"/usr/share/fonts/opentype/noto-cjk/NotoSansCJK-Regular.ttc",
    r"/usr/share/fonts/noto/NotoSansCJK-Regular.ttc",
    r"/usr/share/fonts/truetype/noto/NotoSansCJK-Regular.ttc",
    // Linux - NixOS
    r"/run/current-system/sw/share/X11/fonts/NotoSansCJK-Regular.ttc",
    r"/nix/var/nix/profiles/default/share/fonts/NotoSansCJK-Regular.ttc",
];

/// Directories walked when none of the fixed paths exist. Fontconfig layouts
/// differ between distributions and a portable binary cannot assume any single
/// package is installed, so the last resort is to look for the file itself.
const FONT_DIRS: &[&str] = &[
    "/usr/share/fonts",
    "/usr/local/share/fonts",
    "/usr/share/X11/fonts",
    "/Library/Fonts",
    "/System/Library/Fonts",
];

/// Filename fragments that identify a font with Han coverage (lowercase).
const FONT_HINTS: &[&str] = &[
    "notosanscjk",
    "notosanssc",
    "notoserifcjk",
    "sourcehansans",
    "sourcehanserif",
    "wqy",
    "droidsansfallback",
    "sarasa",
    "cjk",
    "msyh",
    "simhei",
    "simsun",
    "pingfang",
];

/// Loads a CJK-capable font, validates it with ab_glyph and installs it as the
/// primary proportional family (egui's built-in fonts stay as fallbacks).
pub fn install_fonts(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    if let Some((path, bytes)) = load_cjk_font() {
        eprintln!("libresvip-gui: using font {}", path.display());
        fonts
            .font_data
            .insert("ui".to_owned(), Arc::new(FontData::from_owned(bytes)));
        if let Some(family) = fonts.families.get_mut(&FontFamily::Proportional) {
            family.insert(0, "ui".to_owned());
        }
        if let Some(family) = fonts.families.get_mut(&FontFamily::Monospace) {
            family.push("ui".to_owned());
        }
    } else {
        eprintln!(
            "libresvip-gui: no CJK font found, Chinese text will not render.\n\
             Install one (e.g. `fonts-noto-cjk` on Debian/Ubuntu, `noto-fonts-cjk` on Arch) \
             or point LIBRESVIP_GUI_FONT at a .ttf/.otf/.ttc file."
        );
    }
    ctx.set_fonts(fonts);
}

fn load_cjk_font() -> Option<(std::path::PathBuf, Vec<u8>)> {
    for path in candidate_paths() {
        if let Some(bytes) = read_font(&path) {
            return Some((path, bytes));
        }
    }
    None
}

/// Reads `path` and returns its bytes only when ab_glyph agrees it is a font.
fn read_font(path: &std::path::Path) -> Option<Vec<u8>> {
    let bytes = std::fs::read(path).ok()?;
    // A directory or a truncated file is not worth handing to the parser.
    if bytes.len() < 4 * 1024 {
        return None;
    }
    ab_glyph::FontVec::try_from_vec(bytes.clone()).ok()?;
    Some(bytes)
}

/// Every path worth trying, cheapest first: the environment override, the known
/// locations, whatever fontconfig reports, then a directory walk.
fn candidate_paths() -> Vec<std::path::PathBuf> {
    let mut candidates: Vec<std::path::PathBuf> = Vec::new();
    if let Some(path) = std::env::var_os("LIBRESVIP_GUI_FONT") {
        candidates.push(std::path::PathBuf::from(path));
    }
    candidates.extend(FONT_CANDIDATES.iter().map(std::path::PathBuf::from));
    candidates.extend(fc_match_candidates());
    candidates.extend(scan_font_dirs());
    candidates
}

/// Asks fontconfig which file would actually be used for Chinese text. This is
/// the only query that resolves user-installed fonts and fontconfig aliases.
fn fc_match_candidates() -> Vec<std::path::PathBuf> {
    let mut found = Vec::new();
    for query in ["sans-serif:lang=zh-cn", "Noto Sans CJK SC", "sans-serif"] {
        let Ok(output) = std::process::Command::new("fc-match")
            .args(["-f", "%{file}", query])
            .output()
        else {
            // No fontconfig CLI on this system: the fixed list and the walk
            // still cover the common cases.
            break;
        };
        if !output.status.success() {
            continue;
        }
        let path = String::from_utf8_lossy(&output.stdout).trim().to_owned();
        if !path.is_empty() {
            found.push(std::path::PathBuf::from(path));
        }
    }
    found
}

/// Walks the usual font roots and collects filenames that look CJK-capable.
fn scan_font_dirs() -> Vec<std::path::PathBuf> {
    let mut roots: Vec<std::path::PathBuf> = FONT_DIRS.iter().map(std::path::PathBuf::from).collect();
    for key in ["HOME", "USERPROFILE"] {
        if let Some(home) = std::env::var_os(key) {
            let home = std::path::PathBuf::from(home);
            roots.push(home.join(".fonts"));
            roots.push(home.join(".local/share/fonts"));
            roots.push(home.join("AppData/Local/Microsoft/Windows/Fonts"));
        }
    }
    let mut found = Vec::new();
    for root in roots {
        walk_fonts(&root, 0, &mut found);
        if found.len() >= 32 {
            break;
        }
    }
    found
}

fn walk_fonts(dir: &std::path::Path, depth: usize, found: &mut Vec<std::path::PathBuf>) {
    if depth > 5 || found.len() >= 32 {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        match entry.file_type() {
            Ok(kind) if kind.is_dir() => walk_fonts(&path, depth + 1, found),
            Ok(kind) if kind.is_file() => {
                let name = path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or_default()
                    .to_ascii_lowercase();
                let font = name.ends_with(".ttf") || name.ends_with(".ttc") || name.ends_with(".otf");
                if font && FONT_HINTS.iter().any(|hint| name.contains(hint)) {
                    found.push(path);
                }
            }
            _ => {}
        }
        if found.len() >= 32 {
            return;
        }
    }
}

#[cfg(test)]
mod font_tests {
    use super::*;

    #[test]
    fn candidates_are_non_empty_and_start_with_the_override() {
        // SAFETY: this test owns the variable; no other test reads it.
        unsafe { std::env::set_var("LIBRESVIP_GUI_FONT", "/nonexistent/font.ttf") };
        let paths = candidate_paths();
        assert!(!paths.is_empty());
        assert_eq!(paths[0], std::path::PathBuf::from("/nonexistent/font.ttf"));
        unsafe { std::env::remove_var("LIBRESVIP_GUI_FONT") };
    }

    #[test]
    fn missing_or_tiny_files_are_rejected() {
        assert!(read_font(std::path::Path::new("/nonexistent/font.ttf")).is_none());
        let tiny = std::env::temp_dir().join("libresvip-tiny-font.bin");
        std::fs::write(&tiny, b"not a font").expect("write");
        assert!(read_font(&tiny).is_none());
        let _ = std::fs::remove_file(&tiny);
    }
}
