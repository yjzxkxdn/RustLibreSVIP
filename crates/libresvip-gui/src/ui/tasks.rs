//! Task list: conversion mode tabs, drop area and per-file rows.

use eframe::egui::{
    self, pos2, Align, Align2, Color32, CursorIcon, FontId, Layout, Rect, Sense, Stroke, Ui, Vec2,
};

use libresvip_core::core::config::ConversionMode;

use crate::app::App;
use crate::icons::{self, Icon};
use crate::theme::Palette;
use crate::ui::{dashed_rect, padded};
use crate::widgets;

pub fn show(app: &mut App, ui: &mut Ui, rect: Rect) {
    let pal = app.pal;
    widgets::card(ui, rect, &pal);
    if app.tasks.is_empty() {
        empty_state(app, ui, rect, &pal);
    } else {
        list(app, ui, rect, &pal);
    }
}

fn empty_state(app: &mut App, ui: &mut Ui, rect: Rect, pal: &Palette) {
    let inner = padded(rect, 12.0, 12.0, 12.0, 12.0);
    let hovering = ui.ctx().input(|input| !input.raw.hovered_files.is_empty());
    dashed_rect(ui, inner, pal, hovering);
    let response = ui.interact(inner, ui.id().with("drop-area"), Sense::click());
    if response.hovered() {
        ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
    }
    if response.clicked() {
        app.open_files();
    }
    let center = inner.center();
    let icon_rect = Rect::from_center_size(pos2(center.x, center.y - 78.0), Vec2::splat(84.0));
    icons::paint(ui.painter(), icon_rect, Icon::ArrowUp, pal.text);
    let title = if hovering {
        app.t("Release to add files")
    } else {
        app.t("Drag and drop files here")
    };
    ui.painter().text(
        pos2(center.x, center.y + 6.0),
        Align2::CENTER_CENTER,
        title,
        FontId::proportional(28.0),
        pal.text,
    );
    ui.painter().text(
        pos2(center.x, center.y + 42.0),
        Align2::CENTER_CENTER,
        app.t("or"),
        FontId::proportional(14.0),
        pal.muted,
    );
    let button_rect = Rect::from_center_size(pos2(center.x, center.y + 80.0), Vec2::new(132.0, 36.0));
    let button = ui.scope_builder(
        egui::UiBuilder::new()
            .max_rect(button_rect)
            .layout(Layout::left_to_right(Align::Center)),
        |ui| {
            widgets::outlined_button(
                ui,
                pal,
                &app.t("Browse Files"),
                Vec2::new(132.0, 36.0),
                true,
                false,
            )
        },
    );
    if button.inner.clicked() {
        app.open_files();
    }
}

fn list(app: &mut App, ui: &mut Ui, rect: Rect, pal: &Palette) {
    let inner = padded(rect, 16.0, 10.0, 16.0, 6.0);
    crate::ui::at_rect(ui, inner, |ui| {
        ui.spacing_mut().item_spacing = Vec2::new(8.0, 6.0);
        header(app, ui, pal);
        let (divider, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 1.0), Sense::hover());
        ui.painter()
            .line_segment([divider.left_center(), divider.right_center()], Stroke::new(1.0, pal.border));

        let footer_height = 46.0;
        let list_height = (ui.available_height() - footer_height).max(40.0);
        let list_rect = Rect::from_min_size(ui.cursor().min, Vec2::new(ui.available_width(), list_height));
        ui.allocate_rect(list_rect, Sense::hover());
        rows(app, ui, list_rect, pal);
        footer(app, ui, pal);
    });
}

fn header(app: &mut App, ui: &mut Ui, pal: &Palette) {
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new(app.t("Conversion Mode:"))
                .size(Palette::heading())
                .color(pal.text),
        );
        for mode in [
            ConversionMode::Direct,
            ConversionMode::Merge,
            ConversionMode::Split,
        ] {
            let tooltip = match mode {
                ConversionMode::Direct => app.t("Direct Mode"),
                ConversionMode::Merge => app.t("Singing Track Merging Mode"),
                ConversionMode::Split => app.t("Singing Track Grouping Mode"),
            };
            if mode_tab(ui, pal, mode, app.mode == mode, &tooltip) {
                app.mode = mode;
                for task in &mut app.tasks {
                    task.reset_status();
                }
            }
        }
        ui.add_space(4.0);
        widgets::vertical_separator(ui, pal, 24.0);
        let remaining = ui.available_width();
        ui.allocate_ui_with_layout(
            Vec2::new(remaining, 30.0),
            Layout::left_to_right(Align::Center),
            |ui| {
                ui.centered_and_justified(|ui| {
                    ui.label(
                        egui::RichText::new(app.t("Task List"))
                            .size(Palette::heading())
                            .color(pal.text),
                    );
                });
            },
        );
    });
}

fn mode_tab(
    ui: &mut Ui,
    pal: &Palette,
    mode: ConversionMode,
    selected: bool,
    tooltip: &str,
) -> bool {
    let (rect, response) = ui.allocate_exact_size(Vec2::new(50.0, 30.0), Sense::click());
    if selected {
        ui.painter()
            .rect_filled(rect, 0.0, pal.primary.gamma_multiply(0.16));
        ui.painter().line_segment(
            [rect.left_bottom(), rect.right_bottom()],
            Stroke::new(2.0, pal.primary),
        );
    } else if response.hovered() {
        ui.painter().rect_filled(rect, 0.0, pal.hover);
    }
    let color = if selected { pal.primary } else { pal.muted };
    mode_icon(ui, Rect::from_center_size(rect.center(), Vec2::splat(20.0)), mode, color);
    widgets::tooltip_on_hover(ui, &response, tooltip);
    response.on_hover_cursor(egui::CursorIcon::PointingHand).clicked()
}

fn mode_icon(ui: &mut Ui, rect: Rect, mode: ConversionMode, color: Color32) {
    let stroke = Stroke::new(1.6, color);
    let painter = ui.painter();
    let center = rect.center();
    let h = rect.width() * 0.5;
    let p = |x: f32, y: f32| pos2(center.x + x * h, center.y + y * h);
    match mode {
        ConversionMode::Direct => {
            painter.line_segment([p(-0.75, -0.3), p(0.45, -0.3)], stroke);
            painter.add(egui::Shape::line(
                vec![p(0.2, -0.55), p(0.5, -0.3), p(0.2, -0.05)],
                stroke,
            ));
            painter.line_segment([p(0.75, 0.3), p(-0.45, 0.3)], stroke);
            painter.add(egui::Shape::line(
                vec![p(-0.2, 0.05), p(-0.5, 0.3), p(-0.2, 0.55)],
                stroke,
            ));
        }
        ConversionMode::Merge => {
            painter.line_segment([p(-0.75, -0.55), p(0.0, -0.1)], stroke);
            painter.line_segment([p(-0.75, 0.55), p(0.0, 0.1)], stroke);
            painter.line_segment([p(0.0, 0.0), p(0.55, 0.0)], stroke);
            painter.add(egui::Shape::line(
                vec![p(0.3, -0.25), p(0.6, 0.0), p(0.3, 0.25)],
                stroke,
            ));
        }
        ConversionMode::Split => {
            painter.line_segment([p(-0.6, 0.0), p(0.05, 0.0)], stroke);
            painter.line_segment([p(0.05, 0.0), p(0.55, -0.4)], stroke);
            painter.line_segment([p(0.05, 0.0), p(0.55, 0.4)], stroke);
            painter.add(egui::Shape::line(
                vec![p(0.3, -0.58), p(0.6, -0.42), p(0.42, -0.15)],
                stroke,
            ));
            painter.add(egui::Shape::line(
                vec![p(0.3, 0.58), p(0.6, 0.42), p(0.42, 0.15)],
                stroke,
            ));
        }
    }
}

fn rows(app: &mut App, ui: &mut Ui, rect: Rect, pal: &Palette) {
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect)
            .layout(Layout::top_down(Align::Min)),
    );
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(&mut child, |ui| {
            let count = app.tasks.len();
            let mut remove: Option<usize> = None;
            for index in 0..count {
                if task_row(app, ui, index, pal) {
                    remove = Some(index);
                }
            }
            if let Some(index) = remove {
                app.remove_task(index);
            }
        });
}

/// Returns `true` when the row asked to be removed.
fn task_row(app: &mut App, ui: &mut Ui, index: usize, pal: &Palette) -> bool {
    let width = ui.available_width();
    let height = 52.0;
    let (row, _) = ui.allocate_exact_size(Vec2::new(width, height), Sense::hover());
    ui.painter().line_segment(
        [row.left_bottom(), row.right_bottom()],
        Stroke::new(1.0, pal.border),
    );
    let task = &app.tasks[index];
    let name = task.name.clone();
    let path = task.path.to_string_lossy().into_owned();
    let stem = task.stem.clone();
    let ext = task.ext.clone();
    let running = task.running;
    let success = task.success;
    let skipped = task.skipped;
    let error = !task.error.is_empty();
    let warning = !task.warning.is_empty();

    // Name and path
    let name_width = (row.width() - 360.0).max(80.0);
    widgets::elided(
        ui,
        Rect::from_min_size(
            pos2(row.left() + 2.0, row.top() + 4.0),
            Vec2::new(name_width, 24.0),
        ),
        Align2::LEFT_CENTER,
        &name,
        Palette::body() * 1.2,
        pal.text,
    );
    widgets::elided(
        ui,
        Rect::from_min_size(
            pos2(row.left() + 2.0, row.top() + 27.0),
            Vec2::new(name_width, 20.0),
        ),
        Align2::LEFT_CENTER,
        &path,
        Palette::small(),
        pal.muted,
    );

    // Direction indicator
    let arrow_rect = Rect::from_center_size(
        pos2(row.right() - 330.0, row.center().y),
        Vec2::splat(20.0),
    );
    let arrow_icon = if app.mode == ConversionMode::Merge && index != 0 {
        Icon::ArrowUp
    } else {
        Icon::ArrowRight
    };
    icons::paint(ui.painter(), arrow_rect, arrow_icon, pal.text);

    // Stem text field
    let mut stem_value = stem;
    let stem_rect = Rect::from_center_size(
        pos2(row.right() - 220.0, row.center().y),
        Vec2::new(150.0, 32.0),
    );
    let field = ui.scope_builder(
        egui::UiBuilder::new()
            .max_rect(stem_rect)
            .layout(Layout::left_to_right(Align::Center)),
        |ui| widgets::text_field(ui, pal, ui.id().with(("stem", index)), "", &mut stem_value, Vec2::new(150.0, 32.0)),
    );
    if field.inner.changed() {
        app.tasks[index].stem = stem_value;
    }

    // Extension
    ui.painter().text(
        pos2(row.right() - 124.0, row.center().y),
        Align2::LEFT_CENTER,
        &ext,
        FontId::proportional(Palette::body()),
        pal.muted,
    );

    // Remove button
    let trash_rect =
        Rect::from_center_size(pos2(row.right() - 76.0, row.center().y), Vec2::splat(32.0));
    let trash = ui.scope_builder(
        egui::UiBuilder::new()
            .max_rect(trash_rect)
            .layout(Layout::left_to_right(Align::Center)),
        |ui| {
            widgets::icon_button(
                ui,
                pal,
                Icon::Trash,
                30.0,
                Some(&app.t("Remove")),
                !app.busy,
            )
        },
    );
    let mut remove = false;
    if trash.inner.clicked() {
        remove = true;
    }

    // Status indicator
    let status_rect =
        Rect::from_center_size(pos2(row.right() - 28.0, row.center().y), Vec2::splat(36.0));
    if running {
        ui.scope_builder(
            egui::UiBuilder::new()
                .max_rect(status_rect)
                .layout(Layout::left_to_right(Align::Center)),
            |ui| {
                ui.add(egui::Spinner::new().size(22.0).color(pal.primary));
            },
        );
    } else if error {
        let detail_heading = app.t("File failed to convert, below is the error message:");
        let body = app.tasks[index].error.clone();
        let response = ui.scope_builder(
            egui::UiBuilder::new()
                .max_rect(status_rect)
                .layout(Layout::left_to_right(Align::Center)),
            |ui| widgets::status_badge(ui, Icon::Alert, 26.0, pal.error),
        );
        if response.inner.clicked() {
            app.detail = Some((detail_heading, body));
        }
    } else if warning {
        let detail_heading = app.t("This project file may contain abnormal or illegal data");
        let body = app.tasks[index].warning.clone();
        let response = ui.scope_builder(
            egui::UiBuilder::new()
                .max_rect(status_rect)
                .layout(Layout::left_to_right(Align::Center)),
            |ui| widgets::status_badge(ui, Icon::Alert, 26.0, pal.warning),
        );
        if response.inner.clicked() {
            app.detail = Some((detail_heading, body));
        }
    } else if skipped {
        let response = ui.scope_builder(
            egui::UiBuilder::new()
                .max_rect(status_rect)
                .layout(Layout::left_to_right(Align::Center)),
            |ui| widgets::status_badge(ui, Icon::Minimize, 26.0, pal.info),
        );
        response.inner.on_hover_text(app.t("File skipped due to conflict"));
    } else if success == Some(true) {
        let popup_id = ui.id().with(("task-status", index));
        let response = ui.scope_builder(
            egui::UiBuilder::new()
                .max_rect(status_rect)
                .layout(Layout::left_to_right(Align::Center)),
            |ui| widgets::status_badge(ui, Icon::Check, 26.0, pal.success),
        );
        let response = response.inner;
        if response.clicked() {
            widgets::toggle_combo_popup(ui, popup_id);
        }
        let mut action = None;
        widgets::combo_popup(ui, pal, popup_id, &response, 180.0, |ui| {
            if widgets::combo_item(ui, pal, false, &app.t("Open")).clicked() {
                action = Some(false);
            }
            if widgets::combo_item(ui, pal, false, &app.t("Open folder")).clicked() {
                action = Some(true);
            }
        });
        if let Some(folder) = action {
            app.open_output(index, folder);
            widgets::close_combo_popup(ui);
        }
    }
    remove
}

fn footer(app: &mut App, ui: &mut Ui, pal: &Palette) {
    ui.horizontal(|ui| {
        let tools = app.task_toolbar_open;
        let toggle = widgets::icon_button(
            ui,
            pal,
            if tools { Icon::Cross } else { Icon::Tune },
            30.0,
            Some(&app.t("Expand or Collapse Task Toolbar")),
            true,
        );
        if toggle.clicked() {
            app.task_toolbar_open = !app.task_toolbar_open;
        }
        if app.task_toolbar_open {
            if widgets::icon_button(
                ui,
                pal,
                Icon::Plus,
                30.0,
                Some(&app.t("Continue Adding files")),
                true,
            )
            .clicked()
            {
                app.open_files();
            }
            if widgets::icon_button(
                ui,
                pal,
                Icon::Refresh,
                30.0,
                Some(&app.t("Clear Task List")),
                true,
            )
            .clicked()
            {
                app.clear_tasks();
            }
            if widgets::icon_button(
                ui,
                pal,
                Icon::Folder,
                30.0,
                Some(&app.t("Reset Default Extension")),
                true,
            )
            .clicked()
            {
                let ext = app.output_ext();
                for task in &mut app.tasks {
                    task.ext = ext.clone();
                }
                app.reset_stems();
            }
            if widgets::icon_button(
                ui,
                pal,
                Icon::Trash,
                30.0,
                Some(&app.t("Remove Tasks With Other Extensions")),
                true,
            )
            .clicked()
            {
                app.remove_other_extensions();
            }
        }
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if app.mode == ConversionMode::Split {
                let mut value = app.settings.max_track_count;
                let response = ui.add(
                    egui::DragValue::new(&mut value)
                        .range(1..=64)
                        .speed(0.1),
                );
                if response.changed() {
                    app.settings.max_track_count = value.max(1);
                }
                ui.label(
                    egui::RichText::new(app.t("Max Track count:"))
                        .size(Palette::body())
                        .color(pal.text),
                );
            }
        });
    });
}
