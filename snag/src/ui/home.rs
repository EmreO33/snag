use eframe::egui;
use egui::{Key, Rounding, Vec2};

use crate::app::{SnagApp, View};
use crate::jobs::JobState;
use crate::settings::Mode;
use crate::theme;
use crate::util;

/// yt-dlp keeps the canonical list, and it is far too long to mirror here.
const SUPPORTED_SITES_URL: &str = "https://github.com/yt-dlp/yt-dlp/blob/master/supportedsites.md";

pub fn view(app: &mut SnagApp, ui: &mut egui::Ui) {
    let p = app.palette;
    let logo = app.logo.clone();
    let full_width = ui.available_width();
    let box_width = full_width.min(660.0);

    // Sits above everything else, the way cobalt does it.
    ui.vertical_centered(|ui| {
        if theme::pill(ui, &p, "+ supported services", false, true)
            .on_hover_text(SUPPORTED_SITES_URL)
            .clicked()
        {
            ui.ctx()
                .open_url(egui::OpenUrl::new_tab(SUPPORTED_SITES_URL));
        }
    });

    ui.vertical_centered(|ui| {
        let top_gap = ((ui.available_height() - 420.0) * 0.32).max(10.0);
        ui.add_space(top_gap);

        theme::logo(ui, &p, 64.0, logo.as_ref());
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

                // --- what the link turned out to be ------------------------
                preview(app, ui, &p);

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

/// The card under the link box: what this link is, and the choices that only
/// make sense once we know.
fn preview(app: &mut SnagApp, ui: &mut egui::Ui, p: &theme::Palette) {
    match &app.probe {
        crate::probe::ProbeState::Working => {
            ui.add_space(12.0);
            ui.label(
                egui::RichText::new("checking the link...")
                    .size(12.0)
                    .color(p.dim),
            );
        }
        crate::probe::ProbeState::Failed(e) => {
            ui.add_space(12.0);
            // Only the first line: the rest is yt-dlp's own wording, which
            // belongs in the job log rather than under the link box.
            let first: String = e
                .lines()
                .next()
                .unwrap_or_default()
                .chars()
                .take(140)
                .collect();
            ui.label(egui::RichText::new(first).size(12.0).color(p.bad));
        }
        _ => {}
    }

    let Some(probe) = app.current_probe().cloned() else {
        return;
    };

    ui.add_space(12.0);
    theme::card(ui, p, |ui| {
        let title = if probe.title.is_empty() {
            "untitled".to_string()
        } else {
            probe.title.clone()
        };
        let short: String = title.chars().take(64).collect();
        ui.label(egui::RichText::new(short).size(14.0).color(p.text));

        ui.add_space(4.0);
        ui.horizontal(|ui| {
            let mut facts: Vec<String> = Vec::new();
            if let Some(u) = &probe.uploader {
                facts.push(u.chars().take(28).collect());
            }
            if let Some(d) = probe.duration_label() {
                facts.push(d);
            }
            if probe.live {
                facts.push("live".to_string());
            }
            if let Some(n) = probe.playlist_items.filter(|n| *n > 1) {
                facts.push(format!("playlist, {n} items"));
            }
            ui.label(
                egui::RichText::new(facts.join("  ·  "))
                    .size(12.0)
                    .color(p.dim),
            );
        });

        // A playlist link would otherwise quietly download one item, which is
        // rarely what someone pasting a playlist wants.
        if probe.is_playlist() {
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("take").size(12.0).color(p.dim));
                let mut whole = app.whole_playlist;
                if theme::pill(ui, p, "just this one", !whole, true).clicked() {
                    whole = false;
                }
                let count = probe.playlist_items.unwrap_or(0);
                if theme::pill(ui, p, &format!("all {count}"), whole, true).clicked() {
                    whole = true;
                }
                app.whole_playlist = whole;
            });
        }

        // Only offer qualities the site actually has.
        if !probe.heights.is_empty() && app.mode != Mode::Audio {
            ui.add_space(10.0);
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                ui.label(egui::RichText::new("quality").size(12.0).color(p.dim));
                let mut chosen = app.height_override;
                if theme::pill(ui, p, "from settings", chosen.is_none(), true).clicked() {
                    chosen = None;
                }
                for h in probe.heights.iter().take(6) {
                    let label = format!("{h}p");
                    if theme::pill(ui, p, &label, chosen == Some(*h), true).clicked() {
                        chosen = Some(*h);
                    }
                }
                app.height_override = chosen;
            });
        }
    });
}
