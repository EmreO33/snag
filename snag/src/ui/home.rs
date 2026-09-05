use eframe::egui;
use egui::{Key, Rounding, Vec2};

use crate::app::{SnagApp, View};
use crate::jobs::JobState;
use crate::settings::Mode;
use crate::theme;
use crate::util;

pub fn view(app: &mut SnagApp, ui: &mut egui::Ui) {
    let p = app.palette;
    let full_width = ui.available_width();
    let box_width = full_width.min(660.0);

    ui.vertical_centered(|ui| {
        let top_gap = ((ui.available_height() - 420.0) * 0.32).max(10.0);
        ui.add_space(top_gap);

        theme::logo(ui, &p, 64.0);
        ui.add_space(10.0);
        ui.label(
            egui::RichText::new("paste a link, pick what you want, done")
                .size(13.0)
                .color(p.dim),
        );
        ui.add_space(18.0);

        ui.allocate_ui_with_layout(
            Vec2::new(box_width, 0.0),
            egui::Layout::top_down(egui::Align::Min),
            |ui| {
                // --- the link field ---------------------------------------
                let mut submit = false;
                egui::Frame::none()
                    .fill(p.well)
                    .rounding(Rounding::same(12.0))
                    .stroke(egui::Stroke::new(1.0_f32, p.line))
                    .inner_margin(egui::Margin::symmetric(6.0, 4.0))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.add_space(6.0);
                            ui.label(egui::RichText::new("->").size(14.0).color(p.faint));
                            let edit = egui::TextEdit::singleline(&mut app.url_input)
                                .hint_text(
                                    egui::RichText::new("paste the link here").color(p.faint),
                                )
                                .text_color(p.text)
                                .font(egui::FontId::new(15.0, egui::FontFamily::Monospace))
                                .frame(false)
                                .desired_width(ui.available_width() - 8.0)
                                .margin(egui::Margin::symmetric(6.0, 10.0));
                            let r = ui.add(edit);
                            if r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                                submit = true;
                            }
                        });
                    });

                ui.add_space(10.0);

                // --- mode pills + actions ---------------------------------
                ui.horizontal(|ui| {
                    let mut mode = app.mode;
                    if theme::pill_group(ui, &p, &mut mode, Mode::ALL, |m| m.label()) {
                        app.mode = mode;
                    }

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if theme::action_button(ui, &p, "download", true, true).clicked() {
                            submit = true;
                        }
                        if theme::action_button(ui, &p, "paste", false, true).clicked() {
                            match util::clipboard_text() {
                                Some(t) if !t.trim().is_empty() => {
                                    app.url_input = t.trim().to_string();
                                }
                                _ => app.toast("clipboard is empty", true),
                            }
                        }
                    });
                });

                ui.add_space(8.0);
                ui.label(
                    egui::RichText::new(app.mode.hint())
                        .size(12.0)
                        .color(p.faint),
                );

                if submit {
                    app.enqueue_current();
                }

                // --- what is happening right now --------------------------
                let recent: Vec<(String, JobState, f32)> = app
                    .jobs
                    .iter()
                    .rev()
                    .take(3)
                    .map(|j| (j.display_name(), j.state.clone(), j.fraction()))
                    .collect();

                if !recent.is_empty() {
                    ui.add_space(22.0);
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new("recent").size(12.0).color(p.faint));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui
                                .add(
                                    egui::Label::new(
                                        egui::RichText::new("open queue")
                                            .size(12.0)
                                            .color(p.accent),
                                    )
                                    .sense(egui::Sense::click()),
                                )
                                .on_hover_cursor(egui::CursorIcon::PointingHand)
                                .clicked()
                            {
                                app.view = View::Queue;
                            }
                        });
                    });
                    ui.add_space(6.0);

                    for (name, state, frac) in recent {
                        theme::card(ui, &p, |ui| {
                            ui.horizontal(|ui| {
                                let short: String = name.chars().take(52).collect();
                                ui.label(egui::RichText::new(short).size(13.0).color(p.text));
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        let color = match state {
                                            JobState::Done => p.good,
                                            JobState::Failed(_) => p.bad,
                                            _ => p.dim,
                                        };
                                        ui.label(
                                            egui::RichText::new(state.label())
                                                .size(12.0)
                                                .color(color),
                                        );
                                    },
                                );
                            });
                            if state.is_active() {
                                ui.add_space(6.0);
                                theme::progress_bar(ui, &p, frac, frac <= 0.0);
                            }
                        });
                        ui.add_space(6.0);
                    }
                }
            },
        );
    });
}
