pub mod about;
pub mod history_view;
pub mod home;
pub mod queue;
pub mod remux_view;
pub mod settings_view;
pub mod setup;
pub mod updates_view;

use eframe::egui;
use egui::{Color32, FontFamily, FontId, Rounding, Sense, Stroke, Vec2};

use crate::app::{SnagApp, View};
use crate::jobs::JobState;
use crate::motion;
use crate::theme::Palette;
use crate::updater::UpdateState;

/// A full-width nav entry in the left rail.
fn nav_item(
    ui: &mut egui::Ui,
    p: &Palette,
    label: &str,
    badge: Option<String>,
    selected: bool,
) -> egui::Response {
    let width = ui.available_width();
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, 34.0), Sense::click());

    // Painted by hand, so it has to introduce itself. The badge belongs in the
    // name: "queue, 3" is the entire point of the badge being there.
    let spoken = match &badge {
        Some(b) => format!("{label}, {b}"),
        None => label.to_string(),
    };
    response.widget_info(|| {
        egui::WidgetInfo::selected(
            egui::WidgetType::SelectableLabel,
            true,
            selected,
            spoken.clone(),
        )
    });

    if ui.is_rect_visible(rect) {
        let hover_t = motion::on_off(
            ui.ctx(),
            response.id.with("hover"),
            response.hovered(),
            motion::HOVER,
        );
        let select_t = motion::on_off(
            ui.ctx(),
            response.id.with("select"),
            selected,
            motion::SWITCH,
        );
        let fill = motion::mix(
            motion::mix(Color32::TRANSPARENT, p.panel, hover_t),
            p.card,
            select_t,
        );
        ui.painter().rect_filled(rect, Rounding::same(9.0), fill);

        // The accent mark that says which screen you are on is not drawn
        // here: there is one of it for the whole rail, and it slides. See
        // `sidebar`.

        let color = motion::mix(p.dim, p.text, select_t);
        ui.painter().text(
            egui::pos2(rect.left() + 14.0, rect.center().y),
            egui::Align2::LEFT_CENTER,
            label,
            FontId::new(14.0, FontFamily::Monospace),
            color,
        );

        if let Some(b) = badge {
            let font = FontId::new(11.0, FontFamily::Monospace);
            let galley = ui.painter().layout_no_wrap(b, font, p.on_accent);
            let pill = egui::Rect::from_center_size(
                egui::pos2(rect.right() - 8.0 - galley.size().x * 0.5, rect.center().y),
                Vec2::new(galley.size().x + 12.0, 18.0),
            );
            ui.painter()
                .rect_filled(pill, Rounding::same(9.0), p.accent);
            ui.painter()
                .galley(pill.center() - galley.size() * 0.5, galley, p.on_accent);
        }
    }

    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

pub fn sidebar(app: &mut SnagApp, ctx: &egui::Context) {
    let p = app.palette;

    egui::SidePanel::left("snag_nav")
        .exact_width(148.0)
        .resizable(false)
        .frame(
            egui::Frame::none()
                .fill(p.panel)
                .inner_margin(egui::Margin::symmetric(10.0, 16.0)),
        )
        .show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                crate::theme::logo(ui, &p, 36.0, app.logo.as_ref());
                ui.add_space(2.0);
                ui.label(
                    egui::RichText::new("snag")
                        .size(16.0)
                        .color(p.text)
                        .strong(),
                );
            });
            ui.add_space(16.0);

            let active = app.active_job_count();
            let queued = app
                .jobs
                .iter()
                .filter(|j| j.state == JobState::Queued)
                .count();
            let pending = active + queued;

            ui.spacing_mut().item_spacing.y = 4.0;

            // Where the accent mark should be: the row of the screen you are
            // on. Collected while the rows are drawn and used afterwards.
            let mut mark: Option<egui::Rect> = None;
            let mut item = |ui: &mut egui::Ui, label: &str, badge, view: View, current: View| {
                let response = nav_item(ui, &p, label, badge, current == view);
                if current == view {
                    mark = Some(response.rect);
                }
                response.clicked()
            };

            let current = app.view;
            if item(ui, "save", None, View::Home, current) {
                app.view = View::Home;
            }
            let badge = (pending > 0).then(|| pending.to_string());
            if item(ui, "queue", badge, View::Queue, current) {
                app.view = View::Queue;
            }
            if item(ui, "remux", None, View::Remux, current) {
                app.view = View::Remux;
            }
            if item(ui, "history", None, View::History, current) {
                app.view = View::History;
            }

            ui.with_layout(egui::Layout::bottom_up(egui::Align::Min), |ui| {
                ui.spacing_mut().item_spacing.y = 4.0;
                ui.add_space(4.0);
                if item(ui, "about", None, View::About, current) {
                    app.view = View::About;
                }
                let update_badge = matches!(app.update_state, UpdateState::Available { .. })
                    .then(|| "!".to_string());
                if item(ui, "updates", update_badge, View::Updates, current) {
                    app.view = View::Updates;
                }
                if item(ui, "settings", None, View::Settings, current) {
                    app.view = View::Settings;
                }
            });

            // One mark for the whole rail, which travels to the row you
            // picked instead of blinking out of one row and into another.
            // Painted last so it sits on top of the row fills.
            if let Some(rect) = mark {
                let y = motion::toward(
                    ui.ctx(),
                    egui::Id::new("nav_mark_y"),
                    rect.center().y,
                    motion::SWITCH,
                );
                if (y - rect.center().y).abs() > 0.5 {
                    ui.ctx().request_repaint();
                }
                let bar = egui::Rect::from_center_size(
                    egui::pos2(rect.left() + 4.5, y),
                    Vec2::new(3.0, 14.0),
                );
                ui.painter().rect_filled(bar, Rounding::same(2.0), p.accent);
            }
        });
}

pub fn status_bar(app: &mut SnagApp, ctx: &egui::Context) {
    let p = app.palette;

    // Toasts fade out after a few seconds.
    let toast = app.toast.as_ref().and_then(|t| {
        let age = t.at.elapsed().as_secs_f32();
        (age < 5.0).then(|| (t.text.clone(), t.bad, age))
    });
    if toast.is_none() {
        app.toast = None;
    }

    egui::TopBottomPanel::bottom("snag_status")
        .exact_height(30.0)
        .frame(
            egui::Frame::none()
                .fill(p.panel)
                .inner_margin(egui::Margin::symmetric(14.0, 6.0)),
        )
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                match &toast {
                    Some((text, bad, age)) => {
                        let arriving = motion::ramp(*age, motion::ENTER);
                        let leaving = if *age > 4.0 { 5.0 - *age } else { 1.0 };
                        let base = if *bad { p.bad } else { p.good };
                        let color = base.gamma_multiply((arriving * leaving).clamp(0.0, 1.0));
                        // Slides in from the left by a few pixels as it fades.
                        ui.add_space((1.0 - arriving) * 8.0);
                        ui.label(egui::RichText::new(text).size(12.0).color(color));
                        ctx.request_repaint_after(std::time::Duration::from_millis(
                            if arriving < 1.0 { 16 } else { 120 },
                        ));
                    }
                    None => {
                        let active = app.active_job_count();
                        let text = if active > 0 {
                            format!("{active} running")
                        } else {
                            "idle".to_string()
                        };
                        ui.label(egui::RichText::new(text).size(12.0).color(p.faint));
                    }
                }

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    // A missing ffmpeg is worth a permanent word in the corner,
                    // since it breaks the last step of nearly every download.
                    if app.ffmpeg_missing()
                        && ui
                            .add(
                                egui::Label::new(
                                    egui::RichText::new("ffmpeg: not found").size(12.0).color(p.bad),
                                )
                                .sense(Sense::click()),
                            )
                            .on_hover_text("merging, converting, remuxing and clipping all need it. click to install.")
                            .on_hover_cursor(egui::CursorIcon::PointingHand)
                            .clicked()
                    {
                        app.view = View::Updates;
                    }
                    let v = if app.ytdlp_version.is_empty() {
                        "yt-dlp: not found".to_string()
                    } else {
                        format!("yt-dlp {}", app.ytdlp_version)
                    };
                    let color = if app.ytdlp_version.is_empty() {
                        p.bad
                    } else if matches!(app.update_state, UpdateState::Available { .. }) {
                        p.accent
                    } else {
                        p.faint
                    };
                    if ui
                        .add(
                            egui::Label::new(egui::RichText::new(v).size(12.0).color(color))
                                .sense(Sense::click()),
                        )
                        .on_hover_text("open the updates screen")
                        .clicked()
                    {
                        app.view = View::Updates;
                    }
                });
            });
        });
}

/// Page heading shared by every screen.
pub fn page_header(ui: &mut egui::Ui, p: &Palette, title: &str, subtitle: &str) {
    ui.label(egui::RichText::new(title).size(22.0).color(p.text).strong());
    if !subtitle.is_empty() {
        ui.add_space(2.0);
        ui.label(egui::RichText::new(subtitle).size(12.0).color(p.dim));
    }
    ui.add_space(14.0);
}

/// A single-line text field styled to match the rest of the surface.
/// A text box, named by `hint` for anything reading the screen aloud.
///
/// egui gives a text edit no name of its own, so without this a screen reader
/// announces an empty edit box and leaves you to guess what belongs in it.
/// The hint is already the plain description of what goes here.
pub fn text_field(
    ui: &mut egui::Ui,
    p: &Palette,
    value: &mut String,
    hint: &str,
    width: f32,
) -> egui::Response {
    let edit = egui::TextEdit::singleline(value)
        .hint_text(egui::RichText::new(hint).color(p.faint))
        .text_color(p.text)
        .font(FontId::new(14.0, FontFamily::Monospace))
        .margin(egui::Margin::symmetric(12.0, 9.0))
        .desired_width(width);

    let before = ui.visuals().clone();
    ui.visuals_mut().extreme_bg_color = p.well;
    ui.visuals_mut().widgets.inactive.bg_stroke = Stroke::new(1.0_f32, p.line);
    ui.visuals_mut().widgets.hovered.bg_stroke = Stroke::new(1.0_f32, p.line);
    ui.visuals_mut().widgets.active.bg_stroke = Stroke::new(1.0_f32, p.accent);
    ui.visuals_mut().widgets.inactive.rounding = Rounding::same(10.0);
    ui.visuals_mut().widgets.hovered.rounding = Rounding::same(10.0);
    ui.visuals_mut().widgets.active.rounding = Rounding::same(10.0);
    let r = ui.add(edit);
    *ui.visuals_mut() = before;

    let (name, spoken) = (hint.to_string(), value.clone());
    r.widget_info(|| {
        let mut info = egui::WidgetInfo::labeled(egui::WidgetType::TextEdit, true, name.clone());
        // Keep what is typed as the value, so it is read back as well as named.
        info.current_text_value = Some(spoken.clone());
        info
    });
    r
}

/// A labelled text field row used throughout settings.
pub fn field_row(
    ui: &mut egui::Ui,
    p: &Palette,
    label: &str,
    value: &mut String,
    hint: &str,
) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(label).size(13.0).color(p.dim));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let w = (ui.available_width() - 8.0).clamp(120.0, 360.0);
            // Named by the row's own label rather than the hint: "subtitle
            // languages" says more than "en,en-orig".
            let field = text_field(ui, p, value, hint, w);
            let spoken = (label.to_string(), value.clone());
            field.widget_info(|| {
                let mut info =
                    egui::WidgetInfo::labeled(egui::WidgetType::TextEdit, true, spoken.0.clone());
                info.current_text_value = Some(spoken.1.clone());
                info
            });
            if field.changed() {
                changed = true;
            }
        });
    });
    changed
}

/// A labelled integer field with plus/minus stepping.
pub fn number_row(
    ui: &mut egui::Ui,
    p: &Palette,
    label: &str,
    value: &mut u32,
    range: std::ops::RangeInclusive<u32>,
) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(label).size(13.0).color(p.dim));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if crate::theme::pill(ui, p, "+", false, *value < *range.end()).clicked() {
                *value = (*value + 1).min(*range.end());
                changed = true;
            }
            ui.label(
                egui::RichText::new(format!("{value:>3}"))
                    .size(14.0)
                    .color(p.text),
            );
            if crate::theme::pill(ui, p, "-", false, *value > *range.start()).clicked() {
                *value = value.saturating_sub(1).max(*range.start());
                changed = true;
            }
        });
    });
    changed
}
