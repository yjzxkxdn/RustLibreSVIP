//! "Select File Formats" card (left column, top).

use eframe::egui::{self, Align, Layout, Rect, Ui, Vec2};

use crate::app::App;
use crate::icons::Icon;
use crate::theme::Palette;
use crate::ui::card_at;
use crate::widgets;

const COMBO_HEIGHT: f32 = 44.0;
const ROW_HEIGHT: f32 = 32.0;

pub fn show(app: &mut App, ui: &mut Ui, rect: Rect) {
    let pal = app.pal;
    card_at(ui, rect, &pal, (24.0, 18.0, 24.0, 14.0), |ui| {
        ui.spacing_mut().item_spacing = Vec2::new(8.0, 6.0);

        // Title + "Reset Tasks When Changing Input"
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new(app.t("Select File Formats"))
                    .size(Palette::heading())
                    .color(pal.text),
            );
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let mut value = app.settings.reset_tasks_on_input_change;
                let label = app.t("Reset Tasks When Changing Input");
                if widgets::labelled_switch(ui, &pal, &label, &mut value, Palette::body())
                    .changed()
                {
                    app.settings.reset_tasks_on_input_change = value;
                    app.persist_settings();
                }
            });
        });

        // Input format combo + info button
        let info_width = 40.0;
        let combo_width = (ui.available_width() - info_width - 8.0).max(160.0);
        ui.horizontal(|ui| {
            let popup_id = ui.make_persistent_id("input-format-popup");
            let display = app
                .input_entry()
                .map(|entry| format!("{} (*.{})", app.td(entry.file_format), entry.suffix))
                .unwrap_or_default();
            let open = widgets::is_popup_open(ui, popup_id);
            let label = app.t("Input Format: ");
            let response = widgets::combo_field(
                ui,
                &pal,
                ui.id().with("input-format"),
                &label,
                &display,
                Vec2::new(combo_width, COMBO_HEIGHT),
                !app.busy,
                open,
            );
            if response.clicked() {
                widgets::toggle_combo_popup(ui, popup_id);
            }
            let mut picked: Option<String> = None;
            {
                let formats = &app.input_formats;
                let current = app.input_format.clone();
                let translator = &app.translator;
                widgets::combo_popup(ui, &pal, popup_id, &response, combo_width, |ui| {
                    for entry in formats {
                        let label =
                            format!("{} (*.{})", translator.t_dyn(entry.file_format), entry.suffix);
                        if widgets::combo_item(ui, &pal, entry.suffix == current, &label).clicked() {
                            picked = Some(entry.suffix.to_owned());
                        }
                    }
                });
            }
            if let Some(value) = picked {
                app.set_input_format(value);
                widgets::close_combo_popup(ui);
            }
            let info = widgets::icon_button(
                ui,
                &pal,
                Icon::Info,
                38.0,
                Some(&app.t("View Detail Information")),
                true,
            );
            if info.clicked() {
                app.input_info_open = true;
            }
        });

        // Auto-detect + swap button
        ui.horizontal(|ui| {
            let mut value = app.settings.auto_detect_input_format;
            let label = app.t("Auto-Detect Input File Type");
            if widgets::labelled_switch(ui, &pal, &label, &mut value, Palette::body()).changed() {
                app.settings.auto_detect_input_format = value;
                app.persist_settings();
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let response = widgets::icon_button(
                    ui,
                    &pal,
                    Icon::SwapVertical,
                    38.0,
                    Some(&app.t("Swap Input and Output")),
                    !app.busy,
                );
                if response.clicked() {
                    app.swap_formats();
                }
            });
        });

        // Output format combo + info button
        ui.horizontal(|ui| {
            let popup_id = ui.make_persistent_id("output-format-popup");
            let display = app
                .output_entry()
                .map(|entry| format!("{} (*.{})", app.td(entry.file_format), entry.suffix))
                .unwrap_or_default();
            let open = widgets::is_popup_open(ui, popup_id);
            let label = app.t("Output Format: ");
            let response = widgets::combo_field(
                ui,
                &pal,
                ui.id().with("output-format"),
                &label,
                &display,
                Vec2::new(combo_width, COMBO_HEIGHT),
                !app.busy,
                open,
            );
            if response.clicked() {
                widgets::toggle_combo_popup(ui, popup_id);
            }
            let mut picked: Option<String> = None;
            {
                let formats = &app.output_formats;
                let current = app.output_format.clone();
                let translator = &app.translator;
                widgets::combo_popup(ui, &pal, popup_id, &response, combo_width, |ui| {
                    for entry in formats {
                        let label =
                            format!("{} (*.{})", translator.t_dyn(entry.file_format), entry.suffix);
                        if widgets::combo_item(ui, &pal, entry.suffix == current, &label).clicked() {
                            picked = Some(entry.suffix.to_owned());
                        }
                    }
                });
            }
            if let Some(value) = picked {
                app.set_output_format(value);
                widgets::close_combo_popup(ui);
            }
            let info = widgets::icon_button(
                ui,
                &pal,
                Icon::Info,
                38.0,
                Some(&app.t("View Detail Information")),
                true,
            );
            if info.clicked() {
                app.output_info_open = true;
            }
        });

        // Auto extension
        ui.horizontal(|ui| {
            let mut value = app.settings.auto_set_output_extension;
            let label = app.t("Set Output File Extension Automatically");
            if widgets::labelled_switch(ui, &pal, &label, &mut value, Palette::body()).changed() {
                app.settings.auto_set_output_extension = value;
                let ext = app.output_ext();
                for task in &mut app.tasks {
                    task.ext = ext.clone();
                }
                app.persist_settings();
            }
        });
    });
}
