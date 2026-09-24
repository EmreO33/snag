use eframe::egui;
use egui::{Key, Rounding, Vec2};

use crate::app::{PlaylistChoice, SnagApp, View};
use crate::jobs::JobState;
use crate::motion;
use crate::settings::Mode;
use crate::theme;
use crate::util;

/// yt-dlp keeps the canonical list, and it is far too long to mirror here.
const SUPPORTED_SITES_URL: &str = "https://github.com/yt-dlp/yt-dlp/blob/master/supportedsites.md";

pub fn view(app: &mut SnagApp, ui: &mut egui::Ui) {
    // The page grew a playlist picker and a recent list under the link box,
    // either of which can run past the bottom of a small window.
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| page(app, ui));
}

fn page(app: &mut SnagApp, ui: &mut egui::Ui) {
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
                // --- a link the user just copied --------------------------
                offered_link(app, ui, &p);

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
                            // The one box that matters most, and egui gives a
                            // text edit no name of its own, so a screen reader
                            // would otherwise land on an unlabelled field.
                            let typed = app.url_input.clone();
                            r.widget_info(|| {
                                let mut info = egui::WidgetInfo::labeled(
                                    egui::WidgetType::TextEdit,
                                    true,
                                    "link",
                                );
                                info.current_text_value = Some(typed.clone());
                                info
                            });
                            if r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                                submit = true;
                            }
                        });
                    });

                ui.add_space(10.0);

                // Said here, before the download, rather than left for the
                // failure to explain afterwards.
                if app.ffmpeg_missing() {
                    ui.horizontal_wrapped(|ui| {
                        ui.spacing_mut().item_spacing.x = 4.0;
                        ui.label(
                            egui::RichText::new(
                                "ffmpeg is not installed, so merging and converting will fail. install it from",
                            )
                            .size(12.0)
                            .color(p.warn),
                        );
                        if ui
                            .add(
                                egui::Label::new(
                                    egui::RichText::new("updates").size(12.0).color(p.accent),
                                )
                                .sense(egui::Sense::click()),
                            )
                            .on_hover_cursor(egui::CursorIcon::PointingHand)
                            .clicked()
                        {
                            app.view = View::Updates;
                        }
                    });
                    ui.add_space(6.0);
                }

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
                            // Why it failed, not just that it did. This list is
                            // where a failure is first seen, and sending someone
                            // to another screen to find out what went wrong is how
                            // you end up being sent a photograph of the word
                            // "failed".
                            if let JobState::Failed(reason) = &state {
                                ui.add_space(4.0);
                                // The first line is the plain explanation;
                                // yt-dlp's own words follow it underneath.
                                let short: String = reason
                                    .lines()
                                    .next()
                                    .unwrap_or_default()
                                    .chars()
                                    .take(96)
                                    .collect();
                                ui.label(egui::RichText::new(short).size(11.0).color(p.dim));
                            }
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

    let thumbnail = app.thumbnail.clone();

    // Keyed on the link, so each new answer fades up rather than the card
    // flicking from one video's details to another's.
    let arrived = motion::on_off(
        ui.ctx(),
        egui::Id::new(("preview", probe.url.as_str())),
        true,
        motion::ENTER,
    );

    ui.add_space(12.0);
    ui.set_opacity(arrived);
    theme::card(ui, p, |ui| {
        ui.horizontal(|ui| {
            // The preview image, when the site had one and it arrived.
            if let Some((_, texture)) = &thumbnail {
                let height = 68.0;
                let aspect = {
                    let [w, h] = texture.size();
                    if h > 0 {
                        w as f32 / h as f32
                    } else {
                        16.0 / 9.0
                    }
                };
                let size = egui::Vec2::new(height * aspect, height);
                let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
                if ui.is_rect_visible(rect) {
                    // The picture arrives seconds after the card, from the
                    // network, so it gets a fade of its own.
                    let shown = motion::on_off(
                        ui.ctx(),
                        egui::Id::new(("thumb", texture.id())),
                        true,
                        motion::ENTER,
                    );
                    ui.painter().image(
                        texture.id(),
                        rect,
                        egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                        egui::Color32::WHITE.gamma_multiply(shown),
                    );
                }
                ui.add_space(10.0);
            }

            ui.vertical(|ui| {
                let title = if probe.title.is_empty() {
                    "untitled".to_string()
                } else {
                    probe.title.clone()
                };
                let short: String = title.chars().take(58).collect();
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
            });
        });

        // A playlist link would otherwise quietly download one item, which is
        // rarely what someone pasting a playlist wants.
        if probe.is_playlist() {
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("take").size(12.0).color(p.dim));
                let mut choice = app.playlist_choice;
                if theme::pill(ui, p, "just this one", choice == PlaylistChoice::One, true)
                    .clicked()
                {
                    choice = PlaylistChoice::One;
                }
                let count = probe.playlist_items.unwrap_or(0);
                if theme::pill(
                    ui,
                    p,
                    &format!("all {count}"),
                    choice == PlaylistChoice::All,
                    true,
                )
                .clicked()
                {
                    choice = PlaylistChoice::All;
                }
                // Picking needs the items, which a listing that only counted
                // them cannot offer.
                let can_pick = !probe.entries.is_empty();
                let picked = app.playlist_picks.len();
                let label = if choice == PlaylistChoice::Pick && picked > 0 {
                    format!("pick ({picked})")
                } else {
                    "pick".to_string()
                };
                if theme::pill(ui, p, &label, choice == PlaylistChoice::Pick, can_pick)
                    .on_disabled_hover_text(
                        "this site listed how many items there are, but not which",
                    )
                    .clicked()
                {
                    choice = PlaylistChoice::Pick;
                }
                app.playlist_choice = choice;
            });

            if app.playlist_choice == PlaylistChoice::Pick {
                ui.add_space(8.0);
                playlist_picker(app, ui, p, &probe.entries);
            }
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

/// The playlist's items with a box each, for taking some and not others.
fn playlist_picker(
    app: &mut SnagApp,
    ui: &mut egui::Ui,
    p: &theme::Palette,
    entries: &[crate::probe::Entry],
) {
    ui.horizontal(|ui| {
        let picked = app.playlist_picks.len();
        ui.label(
            egui::RichText::new(format!("{picked} of {} picked", entries.len()))
                .size(12.0)
                .color(p.dim),
        );
        if theme::pill(ui, p, "all", false, picked < entries.len()).clicked() {
            app.playlist_picks = entries.iter().map(|e| e.index).collect();
        }
        if theme::pill(ui, p, "none", false, picked > 0).clicked() {
            app.playlist_picks.clear();
        }
    });
    ui.add_space(4.0);

    // Tall enough for a handful, scrolling for a hundred, and never so tall
    // that the download button leaves the screen.
    egui::Frame::none()
        .fill(p.well)
        .rounding(egui::Rounding::same(10.0))
        .inner_margin(egui::Margin::symmetric(6.0, 4.0))
        .show(ui, |ui| {
            egui::ScrollArea::vertical()
                .max_height(260.0)
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = 2.0;
                    for entry in entries {
                        let mut on = app.playlist_picks.contains(&entry.index);
                        let title = if entry.title.is_empty() {
                            format!("item {}", entry.index)
                        } else {
                            entry.title.clone()
                        };
                        let mut label = format!("{}.  {}", entry.index, title);
                        if let Some(d) = entry.duration_label() {
                            label.push_str(&format!("   {d}"));
                        }
                        if theme::checkbox(ui, p, &mut on, &label).changed() {
                            if on {
                                app.playlist_picks.insert(entry.index);
                            } else {
                                app.playlist_picks.remove(&entry.index);
                            }
                        }
                    }
                });
        });
}

/// The strip that appears when clipboard watching spots a link. Deliberately
/// an offer rather than an action: copying a link is not the same as asking
/// for it to be downloaded.
fn offered_link(app: &mut SnagApp, ui: &mut egui::Ui, p: &theme::Palette) {
    let Some(url) = app.offered_link.clone() else {
        return;
    };
    let ctx = ui.ctx().clone();

    egui::Frame::none()
        .fill(p.card)
        .rounding(Rounding::same(12.0))
        .stroke(egui::Stroke::new(1.0_f32, p.accent))
        .inner_margin(egui::Margin::symmetric(14.0, 10.0))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.label(
                        egui::RichText::new("you copied a link")
                            .size(13.0)
                            .color(p.text),
                    );
                    let short: String = url.chars().take(58).collect();
                    ui.label(egui::RichText::new(short).size(11.0).color(p.dim));
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if theme::pill(ui, p, "dismiss", false, true).clicked() {
                        app.offered_link = None;
                    }
                    if theme::action_button(ui, p, "use it", true, true).clicked() {
                        app.accept_offered_link(&ctx);
                    }
                });
            });
        });
    ui.add_space(12.0);
}
