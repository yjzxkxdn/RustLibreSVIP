//! "Output Settings" card: destination folder, conflict policy, start button.

use eframe::egui::{self, Align, Layout, Rect, Ui, Vec2};
use libresvip_core::core::config::ConflictPolicy;

use crate::app::App;
use crate::icons::Icon;
use crate::theme::Palette;
use crate::ui::card_at;
use crate::widgets;

pub fn show(app: &mut App, ui: &mut Ui, rect: Rect) {
    let pal = app.pal;
    card_at(ui, rect, &pal, (24.0, 14.0, 24.0, 14.0), |ui| {
        ui.spacing_mut().item_spacing = Vec2::new(8.0, 6.0);
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new(app.t("Output Settings"))
                    .size(Palette::heading())
                    .color(pal.text),
            );
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let mut value = app.settings.open_save_folder_on_completion;
                let label = app.t("Open Output Folder When Done");
                if widgets::labelled_switch(ui, &pal, &label, &mut value, Palette::body())
                    .changed()
                {
                    app.settings.open_save_folder_on_completion = value;
                    app.persist_settings();
                }
            });
        });

        let start_height = 50.0;
        let start_width = 150.0;
        let remaining = (ui.available_width() - start_width - 16.0).max(180.0);
        ui.horizontal_top(|ui| {
            ui.allocate_ui_with_layout(
                Vec2::new(remaining, start_height + 60.0),
                Layout::top_down(Align::Min),
                |ui| {
                    ui.spacing_mut().item_spacing = Vec2::new(8.0, 6.0);
                    ui.horizontal(|ui| {
                        if widgets::icon_button(
                            ui,
                            &pal,
                            Icon::Folder,
                            40.0,
                            Some(&app.t("Choose Output Folder")),
                            true,
                        )
                        .clicked()
                        {
                            app.choose_folder();
                        }
                        let width = (ui.available_width()).max(120.0);
                        let label = app.t("Output Folder");
                        let response = widgets::text_field(
                            ui,
                            &pal,
                            ui.id().with("save-folder"),
                            &label,
                            &mut app.save_folder,
                            Vec2::new(width, 40.0),
                        );
                        if response.lost_focus() {
                            app.apply_save_folder();
                        }
                    });
                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new(app.t("Deal with Conflicts"))
                                .size(Palette::body())
                                .color(pal.text),
                        );
                        let popup_id = ui.make_persistent_id("conflict-policy");
                        let open = widgets::is_popup_open(ui, popup_id);
                        let current = match app.settings.conflict_policy {
                            ConflictPolicy::Prompt => app.t("Prompt"),
                            ConflictPolicy::Overwrite => app.t("Overwrite"),
                            ConflictPolicy::Skip => app.t("Skip"),
                        };
                        let response = widgets::combo_field(
                            ui,
                            &pal,
                            ui.id().with("conflict-combo"),
                            "",
                            &current,
                            Vec2::new(ui.available_width().max(120.0), 38.0),
                            true,
                            open,
                        );
                        if response.clicked() {
                            widgets::toggle_combo_popup(ui, popup_id);
                        }
                        let mut picked: Option<ConflictPolicy> = None;
                        let policies = [
                            (ConflictPolicy::Prompt, app.t("Prompt")),
                            (ConflictPolicy::Overwrite, app.t("Overwrite")),
                            (ConflictPolicy::Skip, app.t("Skip")),
                        ];
                        widgets::combo_popup(ui, &pal, popup_id, &response, 180.0, |ui| {
                            for (policy, label) in &policies {
                                if widgets::combo_item(
                                    ui,
                                    &pal,
                                    app.settings.conflict_policy == *policy,
                                    label,
                                )
                                .clicked()
                                {
                                    picked = Some(*policy);
                                }
                            }
                        });
                        if let Some(policy) = picked {
                            app.settings.conflict_policy = policy;
                            app.persist_settings();
                            widgets::close_combo_popup(ui);
                        }
                    });
                },
            );
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let label = if app.busy {
                    app.t("Converting")
                } else {
                    app.t("Start Conversion")
                };
                let enabled = app.can_start();
                if widgets::accent_button(
                    ui,
                    &pal,
                    &label,
                    Vec2::new(start_width, start_height),
                    enabled,
                )
                .clicked()
                    && enabled
                {
                    app.start_conversion();
                }
            });
        });
    });
}
