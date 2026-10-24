//! Widget layout of the converter window.

pub mod dialogs;
pub mod formats;
pub mod options;
pub mod output;
pub mod tasks;
pub mod titlebar;

use eframe::egui::{
    self, pos2, Align, CursorIcon, Layout, Rect, Sense, Stroke, StrokeKind, Ui, Vec2,
};
use eframe::egui::ViewportCommand;

use crate::app::App;
use crate::theme::{Palette, TITLEBAR_HEIGHT};
use crate::widgets;

/// Shrinks `rect` by per-side margins.
pub fn padded(rect: Rect, left: f32, top: f32, right: f32, bottom: f32) -> Rect {
    Rect::from_min_max(
        pos2(rect.left() + left, rect.top() + top),
        pos2(rect.right() - right, rect.bottom() - bottom),
    )
}

/// Runs `add` inside `rect` with a top-down layout.
pub fn at_rect<R>(ui: &mut Ui, rect: Rect, add: impl FnOnce(&mut Ui) -> R) -> R {
    ui.scope_builder(
        egui::UiBuilder::new()
            .max_rect(rect)
            .layout(Layout::top_down(Align::Min)),
        add,
    )
    .inner
}

/// Paints a card and runs `add` inside its padded area.
pub fn card_at<R>(
    ui: &mut Ui,
    rect: Rect,
    pal: &Palette,
    margin: (f32, f32, f32, f32),
    add: impl FnOnce(&mut Ui) -> R,
) -> R {
    widgets::card(ui, rect, pal);
    let inner = padded(rect, margin.0, margin.1, margin.2, margin.3);
    at_rect(ui, inner, add)
}

pub fn draw(app: &mut App, ctx: &egui::Context) {
    handle_keys(app, ctx);
    handle_drop(app, ctx);
    handle_resize(ctx);
    titlebar::show(app, ctx);
    if app.restricted {
        restricted_banner(app, ctx);
    }
    egui::CentralPanel::default()
        .frame(egui::Frame::NONE.fill(app.pal.bg))
        .show(ctx, |ui| {
            let rect = ui.max_rect();
            let pal = app.pal;
            let (left, right) = split_horizontal(ui, rect, &mut app.split_ratio, &pal);

            // Left column: format selection on top, task list below.
            let left_top_height = (left.height() * 0.31).clamp(250.0, 300.0);
            let left_top = Rect::from_min_size(left.min, Vec2::new(left.width(), left_top_height));
            let left_bottom = Rect::from_min_max(
                pos2(left.left(), left_top.bottom()),
                left.max,
            );
            formats::show(app, ui, left_top);
            tasks::show(app, ui, left_bottom);

            // Right column: advanced settings on top, output settings below.
            let right_bottom_height = 196.0_f32.min(right.height() * 0.4);
            let right_top = Rect::from_min_max(
                right.min,
                pos2(right.right(), right.bottom() - right_bottom_height),
            );
            let right_bottom =
                Rect::from_min_max(pos2(right.left(), right_top.bottom()), right.max);
            options::show(app, ui, right_top);
            output::show(app, ui, right_bottom);
        });
    dialogs::show(app, ctx);
}

/// Permanent warning strip shown when the process cannot write files because it
/// runs in a security sandbox (low integrity level).
fn restricted_banner(app: &App, ctx: &egui::Context) {
    let pal = app.pal;
    egui::TopBottomPanel::top("restricted-banner")
        .exact_height(30.0)
        .frame(egui::Frame::NONE.fill(pal.warning))
        .show(ctx, |ui| {
            ui.centered_and_justified(|ui| {
                ui.label(
                    egui::RichText::new(app.t(
                        "This program is running in a restricted (sandboxed) process and cannot write files. Start it normally and trust it in your security software.",
                    ))
                    .size(Palette::body())
                    .color(egui::Color32::from_rgb(0x1F, 0x1F, 0x1F)),
                );
            });
        });
}

/// Horizontal split with a draggable divider.
fn split_horizontal(
    ui: &mut Ui,
    rect: Rect,
    ratio: &mut f32,
    pal: &Palette,
) -> (Rect, Rect) {
    let min_pane = 320.0_f32.min(rect.width() * 0.5);
    let mut width = (rect.width() * *ratio).clamp(min_pane, rect.width() - min_pane);
    let handle = Rect::from_min_size(
        pos2(rect.left() + width, rect.top()),
        Vec2::new(1.0, rect.height()),
    );
    let response = ui.interact(
        handle.expand2(Vec2::new(4.0, 0.0)),
        ui.id().with("main-split"),
        Sense::drag(),
    );
    if response.hovered() || response.dragged() {
        ui.ctx().set_cursor_icon(CursorIcon::ResizeHorizontal);
    }
    if response.dragged() {
        width = (width + response.drag_delta().x)
            .clamp(min_pane, rect.width() - min_pane);
        *ratio = width / rect.width();
    }
    ui.painter().line_segment(
        [
            pos2(handle.center().x, rect.top()),
            pos2(handle.center().x, rect.bottom()),
        ],
        Stroke::new(1.0_f32, pal.border),
    );
    let left = Rect::from_min_size(rect.min, Vec2::new(width, rect.height()));
    let right = Rect::from_min_max(pos2(left.right(), rect.top()), rect.max);
    (left, right)
}

/// Draggable borders for the frameless window.
fn handle_resize(ctx: &egui::Context) {
    let Some(pointer) = ctx.pointer_latest_pos() else {
        return;
    };
    let screen = ctx.screen_rect();
    const M: f32 = 5.0;
    let near_left = pointer.x <= screen.left() + M;
    let near_right = pointer.x >= screen.right() - M;
    let near_top = pointer.y <= screen.top() + M;
    let near_bottom = pointer.y >= screen.bottom() - M;
    let direction = match (near_left, near_right, near_top, near_bottom) {
        (true, _, true, _) => Some(egui::ResizeDirection::NorthWest),
        (_, true, true, _) => Some(egui::ResizeDirection::NorthEast),
        (true, _, _, true) => Some(egui::ResizeDirection::SouthWest),
        (_, true, _, true) => Some(egui::ResizeDirection::SouthEast),
        (true, _, _, _) => Some(egui::ResizeDirection::West),
        (_, true, _, _) => Some(egui::ResizeDirection::East),
        (_, _, true, _) => Some(egui::ResizeDirection::North),
        (_, _, _, true) => Some(egui::ResizeDirection::South),
        _ => None,
    };
    let Some(direction) = direction else {
        return;
    };
    let cursor = match direction {
        egui::ResizeDirection::NorthWest | egui::ResizeDirection::SouthEast => {
            CursorIcon::ResizeNwSe
        }
        egui::ResizeDirection::NorthEast | egui::ResizeDirection::SouthWest => {
            CursorIcon::ResizeNeSw
        }
        egui::ResizeDirection::North | egui::ResizeDirection::South => CursorIcon::ResizeVertical,
        _ => CursorIcon::ResizeHorizontal,
    };
    ctx.set_cursor_icon(cursor);
    if ctx.input(|input| input.pointer.any_pressed()) {
        ctx.send_viewport_cmd(ViewportCommand::BeginResize(direction));
    }
}

fn handle_drop(app: &mut App, ctx: &egui::Context) {
    let dropped = ctx.input(|input| {
        input
            .raw
            .dropped_files
            .iter()
            .filter_map(|file| file.path.clone())
            .collect::<Vec<_>>()
    });
    if !dropped.is_empty() {
        app.add_paths(dropped);
        ctx.request_repaint();
    }
}

fn handle_keys(app: &mut App, ctx: &egui::Context) {
    use egui::{Key, Modifiers};
    if ctx.input_mut(|input| input.consume_key(Modifiers::CTRL, Key::O)) {
        app.open_files();
    }
    if ctx.input_mut(|input| input.consume_key(Modifiers::CTRL, Key::Enter)) {
        app.start_conversion();
    }
    if ctx.input_mut(|input| input.consume_key(Modifiers::CTRL, Key::R)) {
        app.clear_tasks();
    }
    if ctx.input_mut(|input| input.consume_key(Modifiers::CTRL, Key::Tab)) {
        app.swap_formats();
    }
    if ctx.input_mut(|input| input.consume_key(Modifiers::NONE, Key::F1)) {
        crate::app::open_url("https://libresvip.soulmelody.net/");
    }
}

/// Height of the title bar, re-exported for the ui modules.
#[allow(dead_code)]
pub const TITLE_BAR_HEIGHT: f32 = TITLEBAR_HEIGHT;

/// Draws the dashed drop rectangle used by the empty task list.
pub fn dashed_rect(ui: &Ui, rect: Rect, pal: &Palette, highlight: bool) {
    let stroke = Stroke::new(
        if highlight { 3.0_f32 } else { 1.0_f32 },
        if highlight { pal.primary } else { pal.border },
    );
    let dash = 6.0;
    let gap = 4.0;
    let painter = ui.painter();
    let mut draw_dashes = |from: egui::Pos2, to: egui::Pos2| {
        let delta = to - from;
        let length = delta.length();
        if length <= 0.0 {
            return;
        }
        let step = delta / length;
        let mut offset = 0.0;
        while offset < length {
            let start = from + step * offset;
            let end = from + step * (offset + dash).min(length);
            painter.line_segment([start, end], stroke);
            offset += dash + gap;
        }
    };
    draw_dashes(rect.left_top(), rect.right_top());
    draw_dashes(rect.right_top(), rect.right_bottom());
    draw_dashes(rect.right_bottom(), rect.left_bottom());
    draw_dashes(rect.left_bottom(), rect.left_top());
    if highlight {
        ui.painter()
            .rect_filled(rect, 0.0, pal.primary.gamma_multiply(0.08));
    }
    let _ = StrokeKind::Inside;
}
