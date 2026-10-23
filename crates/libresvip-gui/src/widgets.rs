//! Material-looking widgets: switches, outlined fields, combo boxes, buttons.

use eframe::egui::{
    self, Align, Align2, Color32, CornerRadius, FontId, Id, Layout, Rect, Response, Sense, Stroke,
    StrokeKind, Ui, Vec2,
};

use crate::icons::{self, Icon};
use crate::theme::{Palette, BORDER_RADIUS};

/// Places a box of `size` inside `rect` according to `anchor` (egui's
/// `Align2::anchor_size` takes a position, this helper takes the container).
pub fn place(anchor: Align2, rect: Rect, size: Vec2) -> egui::Pos2 {
    let x = match anchor.x() {
        Align::Min => rect.left(),
        Align::Center => rect.center().x - size.x * 0.5,
        Align::Max => rect.right() - size.x,
    };
    let y = match anchor.y() {
        Align::Min => rect.top(),
        Align::Center => rect.center().y - size.y * 0.5,
        Align::Max => rect.bottom() - size.y,
    };
    egui::pos2(x, y)
}

pub fn text(
    ui: &Ui,
    pos: egui::Pos2,
    anchor: Align2,
    value: &str,
    size: f32,
    color: Color32,
) -> Rect {
    ui.painter()
        .text(pos, anchor, value, FontId::proportional(size), color)
}

/// Draws single-line text clipped with an ellipsis inside `rect`.
pub fn elided(ui: &Ui, rect: Rect, anchor: Align2, value: &str, size: f32, color: Color32) {
    let mut job = egui::text::LayoutJob::single_section(
        value.to_owned(),
        egui::TextFormat {
            font_id: FontId::proportional(size),
            color,
            ..Default::default()
        },
    );
    job.wrap = egui::text::TextWrapping {
        max_width: rect.width().max(1.0),
        max_rows: 1,
        break_anywhere: true,
        overflow_character: Some('…'),
    };
    let galley = ui.fonts(|fonts| fonts.layout_job(job));
    let pos = place(anchor, rect, galley.size());
    ui.painter().galley(pos, galley, color);
}

/// Draws wrapped text inside `rect` and returns the used height.
#[allow(dead_code)]
pub fn wrapped_text(
    ui: &Ui,
    rect: Rect,
    anchor: Align2,
    value: &str,
    size: f32,
    color: Color32,
) -> f32 {
    let galley = ui.fonts(|fonts| {
        fonts.layout(
            value.to_owned(),
            FontId::proportional(size),
            color,
            rect.width().max(1.0),
        )
    });
    let height = galley.size().y;
    let pos = place(anchor, rect, galley.size());
    ui.painter().galley(pos, galley, color);
    height
}

fn sense(enabled: bool) -> Sense {
    if enabled {
        Sense::click()
    } else {
        Sense::hover()
    }
}

/// Material switch. Allocates 40x24 logical pixels.
pub fn switch(ui: &mut Ui, pal: &Palette, value: &mut bool) -> Response {
    let size = Vec2::new(40.0, 24.0);
    let (rect, mut response) = ui.allocate_exact_size(size, Sense::click());
    if response.clicked() {
        *value = !*value;
        response.mark_changed();
    }
    let t = ui
        .ctx()
        .animate_bool_with_time(response.id.with("anim"), *value, 0.12);
    let track = Rect::from_center_size(rect.center(), Vec2::new(36.0, 20.0));
    let radius = track.height() * 0.5;
    let mut track_color = if *value {
        pal.primary
    } else {
        pal.switch_off
    };
    if response.hovered() && !*value {
        track_color = pal.switch_off.gamma_multiply(1.4);
    }
    ui.painter()
        .rect_filled(track, CornerRadius::same(radius as u8), track_color);
    if !*value {
        ui.painter().rect_stroke(
            track,
            CornerRadius::same(radius as u8),
            Stroke::new(1.0, pal.muted.gamma_multiply(0.6)),
            StrokeKind::Inside,
        );
    }
    let thumb_x = egui::lerp(
        (track.left() + 10.0)..=(track.right() - 10.0),
        t.clamp(0.0, 1.0),
    );
    let thumb_center = egui::pos2(thumb_x, track.center().y);
    let thumb_color = if *value {
        pal.on_primary
    } else {
        pal.switch_thumb
    };
    ui.painter().circle_filled(thumb_center, 8.0, thumb_color);
    if !*value {
        ui.painter().circle_stroke(
            thumb_center,
            8.0,
            Stroke::new(1.0, pal.muted.gamma_multiply(0.5)),
        );
    }
    response
}

/// Switch followed by a label, clickable as a whole.
pub fn labelled_switch(
    ui: &mut Ui,
    pal: &Palette,
    label: &str,
    value: &mut bool,
    font_size: f32,
) -> Response {
    ui.horizontal(|ui| {
        let response = switch(ui, pal, value);
        let label_response = ui.add(
            egui::Label::new(egui::RichText::new(label).size(font_size))
                .selectable(false)
                .sense(Sense::click()),
        );
        if label_response.clicked() {
            *value = !*value;
        }
        response | label_response
    })
    .inner
}

/// Filled button (Material `Button` with an accent background).
pub fn accent_button(
    ui: &mut Ui,
    pal: &Palette,
    label: &str,
    size: Vec2,
    enabled: bool,
) -> Response {
    let (rect, response) = ui.allocate_exact_size(size, sense(enabled));
    let hovered = response.hovered() && enabled;
    let color = if !enabled {
        pal.primary.gamma_multiply(0.55)
    } else if hovered {
        pal.primary_hover
    } else {
        pal.primary
    };
    ui.painter()
        .rect_filled(rect, CornerRadius::same(10), color);
    text(
        ui,
        rect.center(),
        Align2::CENTER_CENTER,
        label,
        Palette::body(),
        pal.on_primary,
    );
    if enabled {
        response.on_hover_cursor(egui::CursorIcon::PointingHand)
    } else {
        response
    }
}

/// Outlined (tonal) button: used for "Browse Files" and dialog buttons.
pub fn outlined_button(
    ui: &mut Ui,
    pal: &Palette,
    label: &str,
    size: Vec2,
    enabled: bool,
    primary: bool,
) -> Response {
    let (rect, response) = ui.allocate_exact_size(size, sense(enabled));
    let hovered = response.hovered() && enabled;
    let fill = if hovered {
        if primary {
            pal.primary.gamma_multiply(0.12)
        } else {
            pal.hover
        }
    } else {
        Color32::TRANSPARENT
    };
    ui.painter()
        .rect_filled(rect, CornerRadius::same(BORDER_RADIUS), fill);
    let stroke_color = if !enabled {
        pal.border
    } else if primary {
        pal.primary
    } else {
        pal.border
    };
    ui.painter().rect_stroke(
        rect,
        CornerRadius::same(BORDER_RADIUS),
        Stroke::new(1.0, stroke_color),
        StrokeKind::Inside,
    );
    let color = if !enabled {
        pal.muted.gamma_multiply(0.7)
    } else if primary {
        pal.primary
    } else {
        pal.text
    };
    text(
        ui,
        rect.center(),
        Align2::CENTER_CENTER,
        label,
        Palette::body(),
        color,
    );
    if enabled {
        response.on_hover_cursor(egui::CursorIcon::PointingHand)
    } else {
        response
    }
}

/// Flat text button (menu bar entries, dialog text buttons).
#[allow(dead_code)]
pub fn flat_button(ui: &mut Ui, pal: &Palette, label: &str) -> Response {
    let galley = ui.fonts(|fonts| {
        fonts.layout_no_wrap(
            label.to_owned(),
            FontId::proportional(Palette::body()),
            pal.text,
        )
    });
    let size = Vec2::new(galley.size().x + 16.0, 24.0);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    if response.hovered() {
        ui.painter()
            .rect_filled(rect, CornerRadius::same(BORDER_RADIUS), pal.hover);
    }
    ui.painter().galley(
        place(Align2::CENTER_CENTER, rect, galley.size()),
        galley,
        pal.text,
    );
    response
}

/// Circular icon button; `diameter` is the hit area.
pub fn icon_button(
    ui: &mut Ui,
    pal: &Palette,
    icon: Icon,
    diameter: f32,
    tooltip: Option<&str>,
    enabled: bool,
) -> Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(diameter), sense(enabled));
    if response.hovered() && enabled {
        ui.painter().circle_filled(
            rect.center(),
            diameter * 0.5,
            if pal.dark {
                pal.hover
            } else {
                Color32::from_rgba_premultiplied(0, 0, 0, 14)
            },
        );
    }
    let color = if enabled {
        pal.muted
    } else {
        pal.muted.gamma_multiply(0.4)
    };
    let glyph = Rect::from_center_size(rect.center(), Vec2::splat(diameter * 0.5));
    icons::paint(ui.painter(), glyph, icon, color);
    if let Some(tooltip) = tooltip {
        tooltip_on_hover(ui, &response, tooltip);
    }
    response
}

/// Small filled circle with an icon inside (task status indicators).
pub fn status_badge(ui: &mut Ui, icon: Icon, diameter: f32, color: Color32) -> Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(diameter), Sense::click());
    ui.painter()
        .circle_filled(rect.center(), diameter * 0.5, color);
    let glyph = Rect::from_center_size(rect.center(), Vec2::splat(diameter * 0.5));
    icons::paint(ui.painter(), glyph, icon, Color32::WHITE);
    response
}

/// Outlined single-line text field with a floating label.
pub fn text_field(
    ui: &mut Ui,
    pal: &Palette,
    id: Id,
    label: &str,
    value: &mut String,
    size: Vec2,
) -> Response {
    let (rect, mut response) = ui.allocate_exact_size(size, Sense::click());
    let focused = ui.memory(|memory| memory.has_focus(id));
    let hovered = response.hovered();
    let (stroke, label_color) = if focused {
        (Stroke::new(2.0, pal.primary), pal.primary)
    } else if hovered {
        (Stroke::new(1.0, pal.text), pal.muted)
    } else {
        (Stroke::new(1.0, pal.border), pal.muted)
    };
    let radius = CornerRadius::same(BORDER_RADIUS);
    ui.painter()
        .rect_filled(rect, radius, Color32::TRANSPARENT);
    ui.painter()
        .rect_stroke(rect, radius, stroke, StrokeKind::Inside);

    if !label.is_empty() {
        let galley = ui.fonts(|fonts| {
            fonts.layout_no_wrap(
                label.to_owned(),
                FontId::proportional(Palette::small()),
                label_color,
            )
        });
        let label_pos = egui::pos2(rect.left() + 10.0, rect.top());
        let patch = Rect::from_min_max(
            egui::pos2(label_pos.x - 4.0, label_pos.y - galley.size().y * 0.5),
            egui::pos2(
                label_pos.x + galley.size().x + 4.0,
                label_pos.y + galley.size().y * 0.5,
            ),
        );
        ui.painter().rect_filled(patch, 0.0, pal.surface);
        ui.painter().galley(
            egui::pos2(label_pos.x, label_pos.y - galley.size().y * 0.5),
            galley,
            label_color,
        );
    }

    let inner = Rect::from_min_max(
        egui::pos2(rect.left() + 12.0, rect.top() + 6.0),
        egui::pos2(rect.right() - 12.0, rect.bottom() - 6.0),
    );
    let edit_response = ui
        .scope_builder(
            egui::UiBuilder::new()
                .max_rect(inner)
                .layout(Layout::left_to_right(Align::Center)),
            |ui| {
                ui.add(
                    egui::TextEdit::singleline(value)
                        .id(id)
                        .frame(false)
                        .desired_width(f32::INFINITY)
                        .font(FontId::proportional(Palette::body())),
                )
            },
        )
        .inner;
    if edit_response.hovered() || ui.rect_contains_pointer(rect) {
        response = response.on_hover_cursor(egui::CursorIcon::Text);
    }
    if edit_response.changed() {
        response.mark_changed();
    }
    response
}

/// Read-only outlined combo box; the caller opens the popup when the returned
/// response is clicked.
pub fn combo_field(
    ui: &mut Ui,
    pal: &Palette,
    id: Id,
    label: &str,
    value: &str,
    size: Vec2,
    enabled: bool,
    open: bool,
) -> Response {
    let (rect, response) = ui.allocate_exact_size(size, sense(enabled));
    let hovered = response.hovered();
    let (stroke, label_color) = if open || response.has_focus() {
        (Stroke::new(2.0, pal.primary), pal.primary)
    } else if hovered && enabled {
        (Stroke::new(1.0, pal.text), pal.muted)
    } else {
        (Stroke::new(1.0, pal.border), pal.muted)
    };
    let radius = CornerRadius::same(BORDER_RADIUS);
    ui.painter()
        .rect_filled(rect, radius, Color32::TRANSPARENT);
    ui.painter()
        .rect_stroke(rect, radius, stroke, StrokeKind::Inside);
    if !label.is_empty() {
        let galley = ui.fonts(|fonts| {
            fonts.layout_no_wrap(
                label.to_owned(),
                FontId::proportional(Palette::small()),
                label_color,
            )
        });
        let label_pos = egui::pos2(rect.left() + 10.0, rect.top());
        let patch = Rect::from_min_max(
            egui::pos2(label_pos.x - 4.0, label_pos.y - galley.size().y * 0.5),
            egui::pos2(
                label_pos.x + galley.size().x + 4.0,
                label_pos.y + galley.size().y * 0.5,
            ),
        );
        ui.painter().rect_filled(patch, 0.0, pal.surface);
        ui.painter().galley(
            egui::pos2(label_pos.x, label_pos.y - galley.size().y * 0.5),
            galley,
            label_color,
        );
    }
    let text_color = if enabled { pal.text } else { pal.muted };
    let text_rect = Rect::from_min_max(
        egui::pos2(rect.left() + 12.0, rect.top() + 6.0),
        egui::pos2(rect.right() - 36.0, rect.bottom() - 4.0),
    );
    let galley = ui.fonts(|fonts| {
        fonts.layout(
            value.to_owned(),
            FontId::proportional(Palette::body()),
            text_color,
            text_rect.width(),
        )
    });
    let pos = place(Align2::LEFT_CENTER, text_rect, galley.size());
    ui.painter().galley(pos, galley, text_color);
    let chevron = Rect::from_center_size(
        egui::pos2(rect.right() - 20.0, rect.center().y),
        Vec2::splat(18.0),
    );
    icons::paint(
        ui.painter(),
        chevron,
        Icon::ChevronDown,
        if enabled {
            pal.muted
        } else {
            pal.muted.gamma_multiply(0.5)
        },
    );
    let _ = id;
    if enabled {
        response.on_hover_cursor(egui::CursorIcon::PointingHand)
    } else {
        response
    }
}

/// Toggles a popup anchored below `anchor`.
pub fn toggle_combo_popup(ui: &Ui, popup_id: Id) {
    ui.memory_mut(|memory| memory.toggle_popup(popup_id));
}

pub fn close_combo_popup(ui: &Ui) {
    ui.memory_mut(|memory| memory.close_popup());
}

pub fn is_popup_open(ui: &Ui, popup_id: Id) -> bool {
    ui.memory(|memory| memory.is_popup_open(popup_id))
}

/// Renders the contents of a combo popup.
pub fn combo_popup(
    ui: &mut Ui,
    pal: &Palette,
    popup_id: Id,
    anchor: &Response,
    width: f32,
    mut items: impl FnMut(&mut Ui),
) {
    egui::popup_below_widget(
        ui,
        popup_id,
        anchor,
        egui::PopupCloseBehavior::CloseOnClick,
        |ui| {
            ui.set_min_width(width);
            ui.set_max_width((width * 1.8).max(width));
            egui::Frame::NONE
                .fill(pal.surface)
                .stroke(Stroke::new(1.0, pal.border))
                .corner_radius(CornerRadius::same(BORDER_RADIUS))
                .inner_margin(egui::Margin::same(4))
                .show(ui, |ui| {
                    egui::ScrollArea::vertical()
                        .max_height(420.0)
                        .auto_shrink([false, true])
                        .show(ui, |ui| {
                            items(ui);
                        });
                });
        },
    );
}

/// One row in a combo popup.
pub fn combo_item(ui: &mut Ui, pal: &Palette, selected: bool, label: &str) -> Response {
    let width = ui.available_width();
    let galley = ui.fonts(|fonts| {
        fonts.layout_no_wrap(
            label.to_owned(),
            FontId::proportional(Palette::body()),
            pal.text,
        )
    });
    let height = galley.size().y.max(20.0) + 10.0;
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, height), Sense::click());
    if selected {
        ui.painter().rect_filled(
            rect,
            CornerRadius::same(2),
            pal.primary.gamma_multiply(0.16),
        );
    } else if response.hovered() {
        ui.painter()
            .rect_filled(rect, CornerRadius::same(2), pal.hover);
    }
    ui.painter().galley(
        place(
            Align2::LEFT_CENTER,
            rect.shrink2(Vec2::new(8.0, 0.0)),
            galley.size(),
        ),
        galley,
        pal.text,
    );
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// Shows `text` next to the pointer while `response` is hovered.
///
/// Qt shows tooltips as soon as the pointer enters the item; egui's
/// `Response::on_hover_text` waits for `style.interaction.tooltip_delay` and
/// needs a later frame (which a static window does not always schedule), which
/// made the "?" help icons look dead. This shows the tooltip immediately and
/// keeps the window repainting so it stays visible.
pub fn tooltip_on_hover(ui: &Ui, response: &Response, text: &str) {
    if text.is_empty() || !response.hovered() {
        return;
    }
    egui::show_tooltip_at_pointer(
        ui.ctx(),
        ui.layer_id(),
        response.id.with("tooltip"),
        |ui| {
            ui.set_max_width(400.0);
            ui.label(egui::RichText::new(text).size(Palette::body()));
        },
    );
    ui.ctx().request_repaint();
}

/// Selectable chip used for theme/language/conflict pickers.
pub fn chip(ui: &mut Ui, pal: &Palette, label: &str, selected: bool) -> Response {
    let galley = ui.fonts(|fonts| {
        fonts.layout_no_wrap(
            label.to_owned(),
            FontId::proportional(Palette::body()),
            pal.text,
        )
    });
    let size = Vec2::new(galley.size().x + 20.0, 28.0);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    if selected {
        ui.painter().rect_filled(
            rect,
            CornerRadius::same(BORDER_RADIUS),
            pal.primary.gamma_multiply(0.18),
        );
        ui.painter().rect_stroke(
            rect,
            CornerRadius::same(BORDER_RADIUS),
            Stroke::new(1.0, pal.primary),
            StrokeKind::Inside,
        );
    } else if response.hovered() {
        ui.painter()
            .rect_filled(rect, CornerRadius::same(BORDER_RADIUS), pal.hover);
    }
    let color = if selected { pal.primary } else { pal.text };
    ui.painter().galley(
        place(Align2::CENTER_CENTER, rect, galley.size()),
        galley,
        color,
    );
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// Horizontal separator line.
#[allow(dead_code)]
pub fn separator(ui: &mut Ui, pal: &Palette) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 1.0), Sense::hover());
    ui.painter().line_segment(
        [rect.left_center(), rect.right_center()],
        Stroke::new(1.0, pal.border),
    );
}

/// Vertical separator line of the given height.
pub fn vertical_separator(ui: &mut Ui, pal: &Palette, height: f32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(1.0, height), Sense::hover());
    ui.painter().line_segment(
        [rect.center_top(), rect.center_bottom()],
        Stroke::new(1.0, pal.border),
    );
}

/// Small help icon with a tooltip.
pub fn help_icon(ui: &mut Ui, pal: &Palette, tooltip: &str) -> Response {
    if tooltip.is_empty() {
        let (_, response) = ui.allocate_exact_size(Vec2::splat(24.0), Sense::hover());
        return response;
    }
    let response = icon_button(ui, pal, Icon::Help, 26.0, None, true);
    tooltip_on_hover(ui, &response, tooltip);
    response.on_hover_cursor(egui::CursorIcon::Help)
}

/// Paints a card background (surface fill + border) and returns the inner rect.
pub fn card(ui: &mut Ui, rect: Rect, pal: &Palette) -> Rect {
    ui.painter().rect_filled(rect, 0.0, pal.surface);
    ui.painter()
        .rect_stroke(rect, 0.0, Stroke::new(1.0, pal.border), StrokeKind::Inside);
    rect
}

/// Clickable overlay used to implement drag regions (title bar).
#[allow(dead_code)]
pub fn drag_region(ui: &mut Ui, rect: Rect, id: Id) -> Response {
    ui.interact(rect, id, Sense::click_and_drag())
}
