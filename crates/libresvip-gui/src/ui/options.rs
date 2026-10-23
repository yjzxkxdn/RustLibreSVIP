//! "Advanced Settings": input/output option forms and middleware sections.

use eframe::egui::{self, pos2, Align, Align2, FontId, Layout, Rect, Sense, Stroke, Ui, Vec2};

use crate::app::App;
use crate::i18n::Translator;
use crate::icons::{self, Icon};
use crate::state::{FieldKind, FieldState};
use crate::theme::Palette;
use crate::ui::padded;
use crate::widgets;

pub fn show(app: &mut App, ui: &mut Ui, rect: Rect) {
    let pal = app.pal;
    widgets::card(ui, rect, &pal);
    let inner = padded(rect, 24.0, 16.0, 12.0, 8.0);
    crate::ui::at_rect(ui, inner, |ui| {
        ui.label(
            egui::RichText::new(app.t("Advanced Settings"))
                .size(Palette::heading())
                .color(pal.text),
        );
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                // Input options
                let input_name = app
                    .input_entry()
                    .map(|entry| app.td(entry.file_format))
                    .unwrap_or_default();
                let input_title = app.t("Input Options");
                let input_subtitle = format!("[{} {}]", app.t("Import as"), input_name);
                let mut expanded = app.input_expanded;
                let visible = !app.input_fields.is_empty();
                if visible {
                    if section_header(ui, &pal, &input_title, &input_subtitle, &mut expanded) {
                        app.input_expanded = expanded;
                    }
                }
                if visible && app.input_expanded {
                    let count = app.input_fields.len();
                    for index in 0..count {
                        let mut field = app.input_fields[index].clone();
                        field_row(ui, &pal, &app.translator, &mut field, index + 1 < count);
                        app.input_fields[index] = field;
                    }
                }

                // Middlewares
                for index in 0..app.middlewares.len() {
                    let name = app.td(app.middlewares[index].name);
                    let description = app.td(app.middlewares[index].description);
                    let mut enabled = app.middlewares[index].enabled;
                    ui.horizontal(|ui| {
                        ui.add_space(8.0);
                        if widgets::switch(ui, &pal, &mut enabled).changed() {
                            app.middlewares[index].enabled = enabled;
                        }
                        ui.label(
                            egui::RichText::new(name)
                                .size(Palette::heading() + 1.0)
                                .color(pal.text),
                        );
                        if !description.is_empty() {
                            widgets::help_icon(ui, &pal, &description);
                        }
                    });
                    if app.middlewares[index].enabled {
                        let count = app.middlewares[index].fields.len();
                        for field_index in 0..count {
                            let mut field = app.middlewares[index].fields[field_index].clone();
                            ui.horizontal(|ui| {
                                ui.add_space(40.0);
                                ui.vertical(|ui| {
                                    ui.set_width(ui.available_width());
                                    field_row(
                                        ui,
                                        &pal,
                                        &app.translator,
                                        &mut field,
                                        field_index + 1 < count,
                                    );
                                });
                            });
                            app.middlewares[index].fields[field_index] = field;
                        }
                    }
                }

                // Output options
                let output_name = app
                    .output_entry()
                    .map(|entry| app.td(entry.file_format))
                    .unwrap_or_default();
                let output_title = app.t("Output Options");
                let output_subtitle = format!("[{} {}]", app.t("Export to"), output_name);
                let mut expanded = app.output_expanded;
                let visible = !app.output_fields.is_empty();
                if visible {
                    if section_header(ui, &pal, &output_title, &output_subtitle, &mut expanded) {
                        app.output_expanded = expanded;
                    }
                }
                if visible && app.output_expanded {
                    let count = app.output_fields.len();
                    for index in 0..count {
                        let mut field = app.output_fields[index].clone();
                        field_row(ui, &pal, &app.translator, &mut field, index + 1 < count);
                        app.output_fields[index] = field;
                    }
                }
                ui.add_space(8.0);
            });
    });
}

/// Expandable section header; returns `true` when the expansion state changed.
fn section_header(
    ui: &mut Ui,
    pal: &Palette,
    title: &str,
    subtitle: &str,
    expanded: &mut bool,
) -> bool {
    let (rect, response) = ui.allocate_exact_size(
        Vec2::new(ui.available_width(), 44.0),
        Sense::click(),
    );
    if response.hovered() {
        ui.painter()
            .rect_filled(rect, 0.0, pal.hover);
    }
    // Disclosure triangle (▾ / ▸), rotated when collapsed.
    let arrow_center = pos2(rect.left() + 18.0, rect.center().y);
    let color = pal.text;
    let stroke = Stroke::new(1.6, color);
    let size = 5.0;
    if *expanded {
        ui.painter().add(egui::Shape::line(
            vec![
                pos2(arrow_center.x - size, arrow_center.y - size * 0.5),
                pos2(arrow_center.x, arrow_center.y + size * 0.7),
                pos2(arrow_center.x + size, arrow_center.y - size * 0.5),
            ],
            stroke,
        ));
    } else {
        ui.painter().add(egui::Shape::line(
            vec![
                pos2(arrow_center.x - size * 0.5, arrow_center.y - size),
                pos2(arrow_center.x + size * 0.7, arrow_center.y),
                pos2(arrow_center.x - size * 0.5, arrow_center.y + size),
            ],
            stroke,
        ));
    }
    let title_galley = ui.fonts(|fonts| {
        fonts.layout_no_wrap(
            title.to_owned(),
            FontId::proportional(Palette::heading() + 1.0),
            pal.text,
        )
    });
    let title_pos = pos2(arrow_center.x + 22.0, rect.center().y - title_galley.size().y * 0.5);
    ui.painter()
        .galley(title_pos, title_galley.clone(), pal.text);
    if !subtitle.is_empty() {
        ui.painter().text(
            pos2(
                title_pos.x + title_galley.size().x + 10.0,
                rect.center().y,
            ),
            Align2::LEFT_CENTER,
            subtitle,
            FontId::proportional(Palette::body() + 2.0),
            pal.muted,
        );
    }
    if response.clicked() {
        *expanded = !*expanded;
        true
    } else {
        false
    }
}

fn field_row(
    ui: &mut Ui,
    pal: &Palette,
    translator: &Translator,
    field: &mut FieldState,
    show_separator: bool,
) {
    let height = 40.0;
    let label_width = 150.0;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing = Vec2::new(8.0, 4.0);
        let title = translator.t_dyn(field.title);
        ui.allocate_ui_with_layout(
            Vec2::new(label_width, height),
            Layout::left_to_right(Align::Center),
            |ui| {
                ui.add(
                    egui::Label::new(
                        egui::RichText::new(format!("{title}："))
                            .size(12.0)
                            .color(pal.text),
                    )
                    .wrap(),
                );
            },
        );
        let control_width = (ui.available_width() - 34.0).max(120.0);
        match field.kind {
            FieldKind::Bool => {
                ui.add_space(4.0);
                widgets::switch(ui, pal, &mut field.bool_value);
                let _ = control_width;
            }
            FieldKind::Enum => {
                let popup_id = ui.make_persistent_id(("enum", field.key));
                let selected = field
                    .choices
                    .get(field.enum_index)
                    .map(|(_, title)| translator.t_dyn(title))
                    .unwrap_or_default();
                let open = widgets::is_popup_open(ui, popup_id);
                let response = widgets::combo_field(
                    ui,
                    pal,
                    ui.id().with(("enum-field", field.key)),
                    "",
                    &selected,
                    Vec2::new(control_width, height),
                    true,
                    open,
                );
                if response.clicked() {
                    widgets::toggle_combo_popup(ui, popup_id);
                }
                let mut picked: Option<usize> = None;
                let choices = field.choices.clone();
                let current = field.enum_index;
                widgets::combo_popup(ui, pal, popup_id, &response, control_width, |ui| {
                    for (index, (_, title)) in choices.iter().enumerate() {
                        if widgets::combo_item(ui, pal, index == current, &translator.t_dyn(title))
                            .clicked()
                        {
                            picked = Some(index);
                        }
                    }
                });
                if let Some(index) = picked {
                    field.enum_index = index;
                    widgets::close_combo_popup(ui);
                }
            }
            FieldKind::Int | FieldKind::Float | FieldKind::Text => {
                widgets::text_field(
                    ui,
                    pal,
                    ui.id().with(("text-field", field.key)),
                    "",
                    &mut field.text,
                    Vec2::new(control_width, height),
                );
            }
        }
        if let Some(description) = field.description {
            widgets::help_icon(ui, pal, &translator.t_dyn(description));
        }
    });
    if show_separator {
        let (rect, _) = ui.allocate_exact_size(
            Vec2::new(ui.available_width(), 12.0),
            Sense::hover(),
        );
        let glyph = Rect::from_center_size(pos2(rect.left() + 12.0, rect.center().y), Vec2::splat(12.0));
        icons::paint(ui.painter(), glyph, Icon::Tune, pal.muted.gamma_multiply(0.8));
        ui.painter().line_segment(
            [pos2(rect.left() + 26.0, rect.center().y), rect.right_center()],
            Stroke::new(1.0, pal.border),
        );
    }
}
