//! Visual language for Snag: a flat, monospace, high-contrast surface with
//! pill-shaped choice groups, plus the small custom widgets that draw it.

use eframe::egui;
use egui::{
    Align, Color32, FontFamily, FontId, Layout, Rect, Response, Rounding, Sense, Stroke, TextStyle,
    Ui, Vec2,
};

use crate::settings::{Accent, ThemeMode};

#[derive(Clone, Copy)]
pub struct Palette {
    pub bg: Color32,
    pub panel: Color32,
    pub card: Color32,
    pub well: Color32,
    pub line: Color32,
    pub text: Color32,
    pub dim: Color32,
    pub faint: Color32,
    pub accent: Color32,
    pub on_accent: Color32,
    pub good: Color32,
    /// Not broken, but it wants attention before it will work.
    pub warn: Color32,
    pub bad: Color32,
}

fn accent_color(accent: Accent, dark: bool) -> Color32 {
    match accent {
        Accent::Mono => {
            if dark {
                Color32::from_rgb(0xf2, 0xf2, 0xf2)
            } else {
                Color32::from_rgb(0x18, 0x18, 0x18)
            }
        }
        Accent::Blue => Color32::from_rgb(0x5b, 0x8d, 0xff),
        Accent::Purple => Color32::from_rgb(0xa4, 0x7b, 0xff),
        Accent::Green => Color32::from_rgb(0x4a, 0xd6, 0x8f),
        Accent::Orange => Color32::from_rgb(0xff, 0x9d, 0x4d),
        Accent::Pink => Color32::from_rgb(0xff, 0x6f, 0xb5),
    }
}

pub fn palette(theme: ThemeMode, accent: Accent) -> Palette {
    let dark = theme != ThemeMode::Light;
    let a = accent_color(accent, dark);
    // Mono accent needs inverted foreground; coloured accents read on near-black.
    let on_accent = if accent == Accent::Mono {
        if dark {
            Color32::from_rgb(0x0b, 0x0b, 0x0b)
        } else {
            Color32::from_rgb(0xfa, 0xfa, 0xfa)
        }
    } else {
        Color32::from_rgb(0x0b, 0x0b, 0x0b)
    };

    match theme {
        ThemeMode::Dark => Palette {
            bg: Color32::from_rgb(0x08, 0x08, 0x08),
            panel: Color32::from_rgb(0x0e, 0x0e, 0x0e),
            card: Color32::from_rgb(0x16, 0x16, 0x16),
            well: Color32::from_rgb(0x1e, 0x1e, 0x1e),
            line: Color32::from_rgb(0x26, 0x26, 0x26),
            text: Color32::from_rgb(0xe8, 0xe8, 0xe8),
            dim: Color32::from_rgb(0x8c, 0x8c, 0x8c),
            faint: Color32::from_rgb(0x5a, 0x5a, 0x5a),
            accent: a,
            on_accent,
            good: Color32::from_rgb(0x5c, 0xd2, 0x9a),
            warn: Color32::from_rgb(0xe8, 0xb3, 0x4a),
            bad: Color32::from_rgb(0xff, 0x6b, 0x6b),
        },
        ThemeMode::Dim => Palette {
            bg: Color32::from_rgb(0x14, 0x15, 0x17),
            panel: Color32::from_rgb(0x1a, 0x1c, 0x1f),
            card: Color32::from_rgb(0x22, 0x25, 0x29),
            well: Color32::from_rgb(0x2b, 0x2f, 0x34),
            line: Color32::from_rgb(0x34, 0x39, 0x3f),
            text: Color32::from_rgb(0xe4, 0xe7, 0xea),
            dim: Color32::from_rgb(0x96, 0x9d, 0xa6),
            faint: Color32::from_rgb(0x66, 0x6d, 0x76),
            accent: a,
            on_accent,
            good: Color32::from_rgb(0x5c, 0xd2, 0x9a),
            warn: Color32::from_rgb(0xe0, 0xae, 0x52),
            bad: Color32::from_rgb(0xff, 0x7b, 0x7b),
        },
        ThemeMode::Light => Palette {
            bg: Color32::from_rgb(0xf7, 0xf7, 0xf7),
            panel: Color32::from_rgb(0xff, 0xff, 0xff),
            card: Color32::from_rgb(0xf1, 0xf1, 0xf1),
            well: Color32::from_rgb(0xe7, 0xe7, 0xe7),
            line: Color32::from_rgb(0xd8, 0xd8, 0xd8),
            text: Color32::from_rgb(0x16, 0x16, 0x16),
            dim: Color32::from_rgb(0x60, 0x60, 0x60),
            faint: Color32::from_rgb(0x90, 0x90, 0x90),
            accent: a,
            on_accent,
            good: Color32::from_rgb(0x1e, 0x8f, 0x5f),
            warn: Color32::from_rgb(0x9a, 0x6b, 0x0a),
            bad: Color32::from_rgb(0xc4, 0x3a, 0x3a),
        },
    }
}

/// Apply the palette to egui's global style. Everything is monospace on purpose.
pub fn apply(ctx: &egui::Context, p: &Palette, scale: f32) {
    let mut style = (*ctx.style()).clone();

    let mono = FontFamily::Monospace;
    style.text_styles = [
        (TextStyle::Heading, FontId::new(19.0, mono.clone())),
        (TextStyle::Body, FontId::new(14.0, mono.clone())),
        (TextStyle::Monospace, FontId::new(13.0, mono.clone())),
        (TextStyle::Button, FontId::new(14.0, mono.clone())),
        (TextStyle::Small, FontId::new(12.0, mono)),
    ]
    .into();

    let v = &mut style.visuals;
    v.dark_mode = p.text.r() > 0x80;
    v.override_text_color = Some(p.text);
    v.panel_fill = p.bg;
    v.window_fill = p.panel;
    v.extreme_bg_color = p.well;
    v.faint_bg_color = p.card;
    v.window_stroke = Stroke::new(1.0_f32, p.line);
    v.window_rounding = Rounding::same(12.0);
    v.menu_rounding = Rounding::same(10.0);
    v.selection.bg_fill = p.accent.linear_multiply(0.35);
    v.selection.stroke = Stroke::new(1.0_f32, p.text);
    v.hyperlink_color = p.accent;

    for (w, fill) in [
        (&mut v.widgets.noninteractive, p.card),
        (&mut v.widgets.inactive, p.card),
        (&mut v.widgets.hovered, p.well),
        (&mut v.widgets.active, p.well),
        (&mut v.widgets.open, p.well),
    ] {
        w.bg_fill = fill;
        w.weak_bg_fill = fill;
        w.bg_stroke = Stroke::new(1.0_f32, p.line);
        w.rounding = Rounding::same(10.0);
        w.fg_stroke = Stroke::new(1.0_f32, p.text);
        w.expansion = 0.0;
    }
    v.widgets.noninteractive.fg_stroke = Stroke::new(1.0_f32, p.dim);
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, p.line);

    style.spacing.item_spacing = Vec2::new(8.0, 8.0);
    style.spacing.button_padding = Vec2::new(12.0, 7.0);
    style.spacing.interact_size.y = 30.0;
    style.spacing.scroll.bar_width = 8.0;
    style.spacing.slider_width = 220.0;

    ctx.set_style(style);
    ctx.set_pixels_per_point(scale.clamp(0.75, 2.0));
}

// --- widgets ---------------------------------------------------------------

/// One pill. Selected pills invert: accent fill, accent-appropriate text.
pub fn pill(ui: &mut Ui, p: &Palette, text: &str, selected: bool, enabled: bool) -> Response {
    let font = FontId::new(14.0, FontFamily::Monospace);
    let galley = ui.painter().layout_no_wrap(
        text.to_string(),
        font,
        if selected { p.on_accent } else { p.text },
    );
    let padding = Vec2::new(14.0, 8.0);
    let size = galley.size() + padding * 2.0;
    let (rect, response) = ui.allocate_exact_size(
        Vec2::new(size.x, size.y.max(30.0)),
        if enabled {
            Sense::click()
        } else {
            Sense::hover()
        },
    );

    if ui.is_rect_visible(rect) {
        let hovered = response.hovered() && enabled;
        let fill = if selected {
            p.accent
        } else if hovered {
            p.well
        } else {
            Color32::TRANSPARENT
        };
        let text_color = if selected {
            p.on_accent
        } else if enabled {
            p.text
        } else {
            p.faint
        };
        ui.painter().rect_filled(rect, Rounding::same(9.0), fill);
        let pos = rect.center() - galley.size() * 0.5;
        ui.painter().galley(pos, galley, text_color);
    }

    if enabled {
        response.on_hover_cursor(egui::CursorIcon::PointingHand)
    } else {
        response
    }
}

/// A group of mutually exclusive pills, wrapping when the row runs out of width.
pub fn pill_group<T: PartialEq + Copy>(
    ui: &mut Ui,
    p: &Palette,
    value: &mut T,
    options: &[T],
    label: impl Fn(&T) -> &'static str,
) -> bool {
    let mut changed = false;
    egui::Frame::none()
        .fill(p.card)
        .rounding(Rounding::same(12.0))
        .inner_margin(egui::Margin::same(4.0))
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing = Vec2::new(4.0, 4.0);
            ui.horizontal_wrapped(|ui| {
                for opt in options {
                    if pill(ui, p, label(opt), *value == *opt, true).clicked() {
                        *value = *opt;
                        changed = true;
                    }
                }
            });
        });
    changed
}

/// A flat rectangular button used for actions rather than choices.
pub fn action_button(
    ui: &mut Ui,
    p: &Palette,
    text: &str,
    primary: bool,
    enabled: bool,
) -> Response {
    let font = FontId::new(14.0, FontFamily::Monospace);
    // Measured first, painted later: the text colour depends on the hover state,
    // which is not known until the rect has been allocated.
    let galley = ui
        .painter()
        .layout_no_wrap(text.to_string(), font.clone(), Color32::WHITE);
    let size = Vec2::new(galley.size().x + 30.0, 34.0);
    let (rect, response) = ui.allocate_exact_size(
        size,
        if enabled {
            Sense::click()
        } else {
            Sense::hover()
        },
    );

    if ui.is_rect_visible(rect) {
        let hovered = response.hovered() && enabled;
        let (fill, fg) = match (primary, enabled) {
            (_, false) => (p.card, p.faint),
            (true, true) => (
                if hovered {
                    p.accent.linear_multiply(0.82)
                } else {
                    p.accent
                },
                p.on_accent,
            ),
            (false, true) => (if hovered { p.well } else { p.card }, p.text),
        };
        ui.painter().rect_filled(rect, Rounding::same(10.0), fill);
        if !primary {
            ui.painter()
                .rect_stroke(rect, Rounding::same(10.0), Stroke::new(1.0_f32, p.line));
        }
        ui.painter()
            .text(rect.center(), egui::Align2::CENTER_CENTER, text, font, fg);
    }

    if enabled {
        response.on_hover_cursor(egui::CursorIcon::PointingHand)
    } else {
        response
    }
}

/// The sliding switch used for every boolean setting.
pub fn toggle(ui: &mut Ui, p: &Palette, on: &mut bool) -> Response {
    let size = Vec2::new(46.0, 26.0);
    let (rect, mut response) = ui.allocate_exact_size(size, Sense::click());
    if response.clicked() {
        *on = !*on;
        response.mark_changed();
    }

    if ui.is_rect_visible(rect) {
        let t = ui.ctx().animate_bool(response.id, *on);
        let track = if *on { p.accent } else { p.well };
        let radius = rect.height() * 0.5;
        ui.painter()
            .rect_filled(rect, Rounding::same(radius), track);
        if !*on {
            ui.painter()
                .rect_stroke(rect, Rounding::same(radius), Stroke::new(1.0_f32, p.line));
        }
        let knob_x = egui::lerp((rect.left() + radius)..=(rect.right() - radius), t);
        let knob_color = if *on { p.on_accent } else { p.text };
        ui.painter().circle_filled(
            egui::pos2(knob_x, rect.center().y),
            radius - 4.0,
            knob_color,
        );
    }

    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// A labelled toggle row: title on the left, switch on the right, note beneath.
pub fn toggle_row(ui: &mut Ui, p: &Palette, title: &str, note: &str, on: &mut bool) -> bool {
    let mut changed = false;
    egui::Frame::none()
        .fill(p.card)
        .rounding(Rounding::same(12.0))
        .inner_margin(egui::Margin::symmetric(14.0, 10.0))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(title).color(p.text));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if toggle(ui, p, on).changed() {
                        changed = true;
                    }
                });
            });
        });
    if !note.is_empty() {
        note_text(ui, p, note);
    }
    changed
}

pub fn section_title(ui: &mut Ui, p: &Palette, title: &str) {
    ui.add_space(6.0);
    ui.label(egui::RichText::new(title).size(17.0).color(p.text).strong());
    ui.add_space(4.0);
}

pub fn note_text(ui: &mut Ui, p: &Palette, text: &str) {
    ui.add_space(2.0);
    ui.label(egui::RichText::new(text).size(12.0).color(p.dim));
    ui.add_space(8.0);
}

pub fn card<R>(ui: &mut Ui, p: &Palette, add: impl FnOnce(&mut Ui) -> R) -> R {
    egui::Frame::none()
        .fill(p.card)
        .rounding(Rounding::same(14.0))
        .inner_margin(egui::Margin::symmetric(16.0, 14.0))
        .show(ui, add)
        .inner
}

/// The Snag mark, tinted with the current theme's text colour. The artwork is
/// white, so a multiply tint recolours it exactly.
pub fn logo(ui: &mut Ui, p: &Palette, size: f32, texture: Option<&egui::TextureHandle>) {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(size), Sense::hover());
    if !ui.is_rect_visible(rect) {
        return;
    }
    let Some(texture) = texture else {
        return;
    };
    ui.painter().image(
        texture.id(),
        rect,
        Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
        p.text,
    );
}

/// A thin progress bar with the accent fill.
pub fn progress_bar(ui: &mut Ui, p: &Palette, fraction: f32, indeterminate: bool) {
    let width = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, 6.0), Sense::hover());
    if !ui.is_rect_visible(rect) {
        return;
    }
    ui.painter().rect_filled(rect, Rounding::same(3.0), p.well);

    if indeterminate {
        // A short bar sweeping back and forth while we have no percentage.
        let t = ui.input(|i| i.time) as f32;
        let span = rect.width() * 0.25;
        let travel = (rect.width() - span).max(0.0);
        let x = ((t * 0.6).sin() * 0.5 + 0.5) * travel;
        let bar = Rect::from_min_size(
            egui::pos2(rect.left() + x, rect.top()),
            Vec2::new(span, rect.height()),
        );
        ui.painter().rect_filled(bar, Rounding::same(3.0), p.accent);
        ui.ctx().request_repaint();
    } else if fraction > 0.0 {
        let bar = Rect::from_min_size(
            rect.min,
            Vec2::new(rect.width() * fraction.clamp(0.0, 1.0), rect.height()),
        );
        ui.painter().rect_filled(bar, Rounding::same(3.0), p.accent);
    }
}
