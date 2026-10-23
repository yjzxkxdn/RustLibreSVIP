//! Vector icons drawn with the egui painter.
//!
//! The Python GUI uses the "Material Design Icons" font; this port draws the
//! same glyphs as line art so the binary does not need an extra font.

use eframe::egui::{Align2, Color32, FontId, Painter, Pos2, Rect, Shape, Stroke, Vec2};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Icon {
    Minimize,
    Maximize,
    Restore,
    Close,
    ChevronDown,
    Info,
    SwapVertical,
    Folder,
    Trash,
    Check,
    Cross,
    Alert,
    Refresh,
    Plus,
    ArrowRight,
    ArrowUp,
    Tune,
    Help,
    Search,
}

/// Paints `icon` centred in `rect` with the given colour.
pub fn paint(painter: &Painter, rect: Rect, icon: Icon, color: Color32) {
    let size = rect.width().min(rect.height());
    let stroke = Stroke::new((size * 0.085).clamp(1.0, 2.0), color);
    let center = rect.center();
    // Unit coordinates are relative to the half-size of the icon box.
    let h = size * 0.5;
    let p = |x: f32, y: f32| Pos2::new(center.x + x * h, center.y + y * h);
    let line = |points: Vec<Pos2>| {
        painter.add(Shape::line(points, stroke));
    };
    let closed = |points: Vec<Pos2>| {
        painter.add(Shape::closed_line(points, stroke));
    };
    match icon {
        Icon::Minimize => { line(vec![p(-0.5, 0.0), p(0.5, 0.0)]); }
        Icon::Maximize => {
            painter.rect_stroke(
                Rect::from_center_size(center, Vec2::splat(size * 0.62)),
                0.0,
                stroke,
                eframe::egui::StrokeKind::Inside,
            );
        }
        Icon::Restore => {
            painter.rect_stroke(
                Rect::from_min_max(p(-0.5, -0.5), p(0.12, 0.12)),
                0.0,
                stroke,
                eframe::egui::StrokeKind::Inside,
            );
            painter.rect_stroke(
                Rect::from_min_max(p(-0.12, -0.12), p(0.5, 0.5)),
                0.0,
                stroke,
                eframe::egui::StrokeKind::Inside,
            );
        }
        Icon::Close => {
            line(vec![p(-0.42, -0.42), p(0.42, 0.42)]);
            line(vec![p(-0.42, 0.42), p(0.42, -0.42)]);
        }
        Icon::ChevronDown => { line(vec![p(-0.42, -0.18), p(0.0, 0.24), p(0.42, -0.18)]); }
        Icon::Info => {
            painter.circle_stroke(center, size * 0.45, stroke);
            painter.circle_filled(p(0.0, -0.22), stroke.width * 0.85, color);
            line(vec![p(0.0, -0.02), p(0.0, 0.26)]);
        }
        Icon::SwapVertical => {
            line(vec![p(0.28, 0.5), p(0.28, -0.35)]);
            line(vec![p(0.08, -0.15), p(0.28, -0.42), p(0.48, -0.15)]);
            line(vec![p(-0.28, -0.5), p(-0.28, 0.35)]);
            line(vec![p(-0.48, 0.15), p(-0.28, 0.42), p(-0.08, 0.15)]);
        }
        Icon::Folder => {
            closed(vec![
                p(-0.5, -0.34),
                p(-0.16, -0.34),
                p(-0.04, -0.16),
                p(0.5, -0.16),
                p(0.5, 0.38),
                p(-0.5, 0.38),
            ]);
        }
        Icon::Trash => {
            line(vec![p(-0.5, -0.32), p(0.5, -0.32)]);
            line(vec![p(-0.16, -0.32), p(-0.16, -0.46), p(0.16, -0.46), p(0.16, -0.32)]);
            line(vec![p(-0.34, -0.22), p(-0.3, 0.46), p(0.3, 0.46), p(0.34, -0.22)]);
            line(vec![p(-0.08, -0.12), p(-0.08, 0.34)]);
            line(vec![p(0.08, -0.12), p(0.08, 0.34)]);
        }
        Icon::Check => { line(vec![p(-0.42, 0.02), p(-0.1, 0.32), p(0.44, -0.32)]); }
        Icon::Cross => {
            line(vec![p(-0.32, -0.32), p(0.32, 0.32)]);
            line(vec![p(-0.32, 0.32), p(0.32, -0.32)]);
        }
        Icon::Alert => {
            line(vec![p(0.0, -0.36), p(0.0, 0.1)]);
            painter.circle_filled(p(0.0, 0.3), stroke.width * 0.95, color);
        }
        Icon::Refresh => {
            let mut points = Vec::new();
            let start = -0.6_f32;
            let end = 4.0_f32;
            let steps = 24;
            for index in 0..=steps {
                let t = start + (end - start) * index as f32 / steps as f32;
                points.push(p(t.cos() * 0.42, t.sin() * 0.42));
            }
            line(points);
            line(vec![p(0.12, -0.6), p(0.46, -0.36), p(0.16, -0.2)]);
        }
        Icon::Plus => {
            line(vec![p(-0.42, 0.0), p(0.42, 0.0)]);
            line(vec![p(0.0, -0.42), p(0.0, 0.42)]);
        }
        Icon::ArrowRight => {
            line(vec![p(-0.42, 0.0), p(0.3, 0.0)]);
            line(vec![p(0.06, -0.26), p(0.36, 0.0), p(0.06, 0.26)]);
        }
        Icon::ArrowUp => {
            line(vec![p(0.0, 0.42), p(0.0, -0.3)]);
            line(vec![p(-0.26, -0.06), p(0.0, -0.36), p(0.26, -0.06)]);
        }
        Icon::Tune => {
            line(vec![p(-0.45, -0.28), p(0.45, -0.28)]);
            line(vec![p(-0.45, 0.06), p(0.45, 0.06)]);
            line(vec![p(-0.45, 0.4), p(0.45, 0.4)]);
            painter.circle_filled(p(-0.12, -0.28), stroke.width * 1.5, color);
            painter.circle_filled(p(0.22, 0.06), stroke.width * 1.5, color);
            painter.circle_filled(p(-0.24, 0.4), stroke.width * 1.5, color);
        }
        Icon::Help => {
            painter.circle_stroke(center, size * 0.45, stroke);
            painter.text(
                center,
                Align2::CENTER_CENTER,
                "?",
                FontId::proportional(size * 0.62),
                color,
            );
        }
        Icon::Search => {
            painter.circle_stroke(p(-0.08, -0.08), size * 0.3, stroke);
            line(vec![p(0.14, 0.14), p(0.44, 0.44)]);
        }
    }
}
