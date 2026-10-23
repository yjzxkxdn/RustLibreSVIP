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
        Stroke::new(1.0, self.border)
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
    visuals.window_stroke = Stroke::new(1.0, pal.border);
    visuals.window_corner_radius = CornerRadius::same(8);
    visuals.menu_corner_radius = CornerRadius::same(4);
    visuals.selection = egui::style::Selection {
        bg_fill: pal.primary.gamma_multiply(0.35),
        stroke: Stroke::new(1.0, pal.primary),
    };

    let widgets = &mut visuals.widgets;
    widgets.noninteractive = egui::style::WidgetVisuals {
        bg_fill: pal.surface,
        weak_bg_fill: pal.surface,
        bg_stroke: Stroke::new(1.0, pal.border),
        corner_radius: radius,
        fg_stroke: Stroke::new(1.0, pal.text),
        expansion: 0.0,
    };
    widgets.inactive = egui::style::WidgetVisuals {
        bg_fill: Color32::TRANSPARENT,
        weak_bg_fill: Color32::TRANSPARENT,
        bg_stroke: Stroke::new(1.0, pal.border),
        corner_radius: radius,
        fg_stroke: Stroke::new(1.0, pal.text),
        expansion: 0.0,
    };
    widgets.hovered = egui::style::WidgetVisuals {
        bg_fill: pal.hover,
        weak_bg_fill: pal.hover,
        bg_stroke: Stroke::new(1.0, pal.primary),
        corner_radius: radius,
        fg_stroke: Stroke::new(1.0, pal.text),
        expansion: 0.0,
    };
    widgets.active = egui::style::WidgetVisuals {
        bg_fill: pal.hover,
        weak_bg_fill: pal.hover,
        bg_stroke: Stroke::new(2.0, pal.primary),
        corner_radius: radius,
        fg_stroke: Stroke::new(1.0, pal.text),
        expansion: 0.0,
    };
    widgets.open = egui::style::WidgetVisuals {
        bg_fill: pal.surface,
        weak_bg_fill: pal.surface,
        bg_stroke: Stroke::new(1.0, pal.border),
        corner_radius: radius,
        fg_stroke: Stroke::new(1.0, pal.text),
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
const FONT_CANDIDATES: &[&str] = &[
    r"C:\Windows\Fonts\msyh.ttc",
    r"C:\Windows\Fonts\Deng.ttf",
    r"C:\Windows\Fonts\simhei.ttf",
    r"C:\Windows\Fonts\NotoSansSC-VF.ttf",
    r"C:\Windows\Fonts\simsun.ttc",
    r"/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
    r"/System/Library/Fonts/PingFang.ttc",
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
        eprintln!("libresvip-gui: no CJK font found, Chinese text will not render");
    }
    ctx.set_fonts(fonts);
}

fn load_cjk_font() -> Option<(std::path::PathBuf, Vec<u8>)> {
    let mut candidates: Vec<std::path::PathBuf> = Vec::new();
    if let Some(path) = std::env::var_os("LIBRESVIP_GUI_FONT") {
        candidates.push(std::path::PathBuf::from(path));
    }
    candidates.extend(FONT_CANDIDATES.iter().map(std::path::PathBuf::from));
    for path in candidates {
        let Ok(bytes) = std::fs::read(&path) else {
            continue;
        };
        if ab_glyph::FontVec::try_from_vec(bytes.clone()).is_ok() {
            return Some((path, bytes));
        }
    }
    None
}
