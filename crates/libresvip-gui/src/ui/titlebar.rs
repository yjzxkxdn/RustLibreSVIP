//! Custom title bar: application icon, menu bar, caption and window buttons.
//!
//! The Python GUI is frameless and draws its own title bar
//! (`top_toolbar.qml`); this port does the same and asks winit to start the
//! system move/resize loops.

use eframe::egui::{
    self, pos2, Align, Align2, Color32, CornerRadius, CursorIcon, FontId, Layout, Rect, Response,
    Sense, Stroke, Ui, Vec2, ViewportCommand,
};

use crate::app::{open_url, App, SettingsTab};
use crate::icons::{self, Icon};
use crate::theme::{Palette, TITLEBAR_HEIGHT};
use libresvip_core::core::config::DarkMode;
use crate::i18n::Lang;
use crate::widgets;

pub const REPO_URL: &str = "https://github.com/yjzxkxdn/RustLibreSVIP";
pub const DOCS_URL: &str = "https://libresvip.soulmelody.net/";
pub const AUTHOR_URL: &str = "https://space.bilibili.com/175862486";

pub fn show(app: &mut App, ctx: &egui::Context) {
    let pal = app.pal;
    egui::TopBottomPanel::top("title-bar")
        .exact_height(TITLEBAR_HEIGHT)
        .frame(egui::Frame::NONE.fill(pal.surface))
        .show(ctx, |ui| {
            let rect = ui.max_rect();
            ui.painter().line_segment(
                [rect.left_bottom(), rect.right_bottom()],
                Stroke::new(1.0_f32, pal.border),
            );
            egui::Ui::scope_builder(
                ui,
                egui::UiBuilder::new()
                    .max_rect(rect)
                    .layout(Layout::left_to_right(Align::Center)),
                |ui| {
                    ui.spacing_mut().item_spacing = Vec2::ZERO;
                    // Menu bar buttons are flat (Qt `MenuBarItem` has no
                    // background until hovered).
                    {
                        let widgets = &mut ui.style_mut().visuals.widgets;
                        widgets.inactive.bg_fill = Color32::TRANSPARENT;
                        widgets.inactive.weak_bg_fill = Color32::TRANSPARENT;
                        widgets.inactive.bg_stroke = Stroke::NONE;
                        widgets.hovered.bg_fill = pal.hover;
                        widgets.hovered.weak_bg_fill = pal.hover;
                        widgets.hovered.bg_stroke = Stroke::NONE;
                        widgets.active.bg_fill = pal.hover;
                        widgets.active.weak_bg_fill = pal.hover;
                        widgets.active.bg_stroke = Stroke::NONE;
                        widgets.open.bg_fill = pal.hover;
                        widgets.open.weak_bg_fill = pal.hover;
                        widgets.open.bg_stroke = Stroke::NONE;
                    }

                    // Application icon (`configItems.icon_data`, 16x16 in Qt).
                    let (icon_rect, _) =
                        ui.allocate_exact_size(Vec2::splat(TITLEBAR_HEIGHT), Sense::hover());
                    if let Some(texture) = &app.app_icon {
                        let target = Rect::from_center_size(icon_rect.center(), Vec2::splat(18.0));
                        ui.painter().image(
                            texture.id(),
                            target,
                            Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                            Color32::WHITE,
                        );
                    }

                    menus(app, ui);

                    ui.add_space(12.0);
                    widgets::vertical_separator(ui, &pal, 16.0);
                    ui.add_space(16.0);

                    let title = format!("RustLibreSVIP - {}", app.t("SVS Projects Converter"));
                    let title_response = ui.label(
                        egui::RichText::new(title)
                            .size(Palette::body() * 1.2)
                            .color(pal.text),
                    );
                    // The caption plus the free space next to it drags the
                    // frameless window (`DragHandler` on `captionLabel`).
                    let drag_rect = Rect::from_min_max(
                        pos2(title_response.rect.left() - 4.0, rect.top()),
                        pos2(rect.right() - 3.0 * 46.0, rect.bottom()),
                    );
                    let drag = ui.interact(
                        drag_rect,
                        ui.id().with("title-drag"),
                        Sense::click_and_drag(),
                    );
                    if drag.double_clicked() {
                        let maximized =
                            ui.ctx().input(|input| input.viewport().maximized.unwrap_or(false));
                        ui.ctx()
                            .send_viewport_cmd(ViewportCommand::Maximized(!maximized));
                    } else if drag.drag_started() {
                        ui.ctx().send_viewport_cmd(ViewportCommand::StartDrag);
                    }

                    let spacer = (ui.available_width() - 3.0 * 46.0).max(0.0);
                    ui.allocate_space(Vec2::new(spacer, 0.0));

                    let maximized = ui
                        .ctx()
                        .input(|input| input.viewport().maximized.unwrap_or(false));
                    if window_button(ui, &pal, Icon::Minimize, false)
                        .on_hover_text(app.t("Minimize"))
                        .clicked()
                    {
                        ui.ctx()
                            .send_viewport_cmd(ViewportCommand::Minimized(true));
                    }
                    let restore_icon = if maximized {
                        Icon::Restore
                    } else {
                        Icon::Maximize
                    };
                    if window_button(ui, &pal, restore_icon, false)
                        .on_hover_text(app.t("Maximize or Restore"))
                        .clicked()
                    {
                        ui.ctx()
                            .send_viewport_cmd(ViewportCommand::Maximized(!maximized));
                    }
                    if window_button(ui, &pal, Icon::Close, true)
                        .on_hover_text(app.t("Close"))
                        .clicked()
                    {
                        ui.ctx().send_viewport_cmd(ViewportCommand::Close);
                    }
                    let _ = &pal;
                },
            );
        });
}

fn menus(app: &mut App, ui: &mut Ui) {
    let pal = app.pal;
    let convert = app.t("Convert (&C)");
    ui.menu_button(convert, |ui| {
        ui.set_min_width(300.0);
        let label = app.t("Import Projects (Ctrl+O)");
        if menu_row(ui, &pal, &label, Some(Icon::Folder), true).clicked() {
            app.open_files();
            ui.close_menu();
        }
        let label = app.t("Perform All Tasks (Ctrl+Enter)");
        if menu_row(ui, &pal, &label, Some(Icon::ArrowRight), app.can_start()).clicked() {
            app.start_conversion();
            ui.close_menu();
        }
        let label = app.t("Clear Tasks (Ctrl+R)");
        if menu_row(ui, &pal, &label, Some(Icon::Refresh), !app.tasks.is_empty()).clicked() {
            app.clear_tasks();
            ui.close_menu();
        }
        ui.separator();
        let label = app.t("Swap Input and Output (Ctrl+Tab)");
        if menu_row(ui, &pal, &label, Some(Icon::SwapVertical), true).clicked() {
            app.swap_formats();
            ui.close_menu();
        }
    });

    let formats = app.t("Formats (&F)");
    ui.menu_button(formats, |ui| {
        ui.set_min_width(220.0);
        let input_title = app.t("Input Format (&I)");
        ui.menu_button(input_title, |ui| {
            ui.set_min_width(320.0);
            let mut picked = None;
            for entry in &app.input_formats {
                let label = format!("{} (*.{})", app.td(entry.file_format), entry.suffix);
                let selected = entry.suffix == app.input_format;
                if menu_row_checked(ui, &pal, &label, selected, true).clicked() {
                    picked = Some(entry.suffix.to_owned());
                }
            }
            if let Some(value) = picked {
                app.set_input_format(value);
                ui.close_menu();
            }
        });
        let output_title = app.t("Output Format (&E)");
        ui.menu_button(output_title, |ui| {
            ui.set_min_width(320.0);
            let mut picked = None;
            for entry in &app.output_formats {
                let label = format!("{} (*.{})", app.td(entry.file_format), entry.suffix);
                let selected = entry.suffix == app.output_format;
                if menu_row_checked(ui, &pal, &label, selected, true).clicked() {
                    picked = Some(entry.suffix.to_owned());
                }
            }
            if let Some(value) = picked {
                app.set_output_format(value);
                ui.close_menu();
            }
        });
    });

    let settings = app.t("Settings (&S)");
    ui.menu_button(settings, |ui| {
        ui.set_min_width(240.0);
        // Use egui's own button here so the label lines up with the submenu
        // entries below it (both place their text at `button_padding`).
        let label = app.t("Options (&O)");
        if ui.button(label).clicked() {
            app.show_settings = true;
            ui.close_menu();
        }
        let themes = app.t("Themes (&T)");
        ui.menu_button(themes, |ui| {
            ui.set_min_width(180.0);
            for (mode, label) in [
                (DarkMode::Light, app.t("Light")),
                (DarkMode::Dark, app.t("Dark")),
                (DarkMode::System, app.t("System")),
            ] {
                let checked = app.settings.dark_mode == mode;
                if menu_row_checked(ui, &pal, &label, checked, true).clicked() {
                    let ctx = ui.ctx().clone();
                    app.set_theme(mode, &ctx);
                    ui.close_menu();
                }
            }
        });
        let languages = app.t("Language (&L)");
        ui.menu_button(languages, |ui| {
            ui.set_min_width(180.0);
            let mut picked = None;
            for lang in Lang::all() {
                let checked = app.lang == lang;
                if menu_row_checked(ui, &pal, lang.native_name(), checked, true).clicked() {
                    picked = Some(lang);
                }
            }
            if let Some(lang) = picked {
                app.set_language(lang);
                ui.close_menu();
            }
        });
    });

    let help = app.t("Help (&H)");
    ui.menu_button(help, |ui| {
        ui.set_min_width(260.0);
        let label = app.t("About (Ctrl+A)");
        if menu_row(ui, &pal, &label, Some(Icon::Info), true).clicked() {
            app.show_about = true;
            ui.close_menu();
        }
        let label = app.t("Check for Updates");
        if menu_row(ui, &pal, &label, Some(Icon::Refresh), true).clicked() {
            app.update_notice = Some(
                app.t("This build of RustLibreSVIP cannot check for updates online."),
            );
            ui.close_menu();
        }
        let label = app.t("Documentation (F1)");
        if menu_row(ui, &pal, &label, Some(Icon::Search), true).clicked() {
            open_url(DOCS_URL);
            ui.close_menu();
        }
    });
}

fn menu_row(
    ui: &mut Ui,
    pal: &Palette,
    label: &str,
    icon: Option<Icon>,
    enabled: bool,
) -> Response {
    let width = ui.available_width().max(220.0);
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, 30.0), Sense::click());
    if response.hovered() && enabled {
        ui.painter()
            .rect_filled(rect, CornerRadius::same(2), pal.hover);
    }
    let color = if enabled {
        pal.text
    } else {
        pal.muted.gamma_multiply(0.6)
    };
    if let Some(icon) = icon {
        let glyph = Rect::from_center_size(
            pos2(rect.left() + 18.0, rect.center().y),
            Vec2::splat(16.0),
        );
        icons::paint(ui.painter(), glyph, icon, color);
    }
    let text_x = if icon.is_some() {
        rect.left() + 38.0
    } else {
        rect.left() + 14.0
    };
    ui.painter().text(
        pos2(text_x, rect.center().y),
        Align2::LEFT_CENTER,
        label,
        FontId::proportional(Palette::body()),
        color,
    );
    if enabled {
        response.on_hover_cursor(CursorIcon::PointingHand)
    } else {
        response
    }
}

fn menu_row_checked(
    ui: &mut Ui,
    pal: &Palette,
    label: &str,
    checked: bool,
    enabled: bool,
) -> Response {
    let width = ui.available_width().max(180.0);
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, 30.0), Sense::click());
    if response.hovered() && enabled {
        ui.painter()
            .rect_filled(rect, CornerRadius::same(2), pal.hover);
    }
    let color = if enabled {
        pal.text
    } else {
        pal.muted.gamma_multiply(0.6)
    };
    if checked {
        let glyph = Rect::from_center_size(
            pos2(rect.left() + 18.0, rect.center().y),
            Vec2::splat(14.0),
        );
        icons::paint(ui.painter(), glyph, Icon::Check, pal.primary);
    }
    ui.painter().text(
        pos2(rect.left() + 36.0, rect.center().y),
        Align2::LEFT_CENTER,
        label,
        FontId::proportional(Palette::body()),
        color,
    );
    if enabled {
        response.on_hover_cursor(CursorIcon::PointingHand)
    } else {
        response
    }
}

fn window_button(ui: &mut Ui, pal: &Palette, icon: Icon, close: bool) -> Response {
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(46.0, TITLEBAR_HEIGHT), Sense::click());
    let hover_bg = if close {
        Color32::from_rgb(0xD3, 0x2F, 0x2F)
    } else {
        if pal.dark {
            Color32::from_rgba_premultiplied(255, 255, 255, 22)
        } else {
            Color32::from_rgba_premultiplied(0, 0, 0, 16)
        }
    };
    if response.hovered() {
        ui.painter().rect_filled(rect, 0.0, hover_bg);
    }
    let color = if close && response.hovered() {
        Color32::WHITE
    } else {
        pal.text
    };
    icons::paint(
        ui.painter(),
        Rect::from_center_size(rect.center(), Vec2::splat(12.0)),
        icon,
        color,
    );
    response
}

pub fn settings_tab_name(app: &App, tab: SettingsTab) -> String {
    match tab {
        SettingsTab::Basic => app.t("Basic Settings"),
        SettingsTab::SavePath => app.t("Save Path Settings"),
        SettingsTab::Plugins => app.t("Format Provider Plugins"),
        SettingsTab::Updates => app.t("Updates Settings"),
    }
}

