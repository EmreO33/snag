use eframe::egui;

use crate::app::SnagApp;
use crate::jobs::{Job, JobState};
use crate::theme;
use crate::util;

enum Action {
    Cancel(u64),
    Retry(u64),
    Remove(u64),
    ToggleLog(u64),
    Reveal(std::path::PathBuf),
    Open(std::path::PathBuf),
    Clip(std::path::PathBuf),
    CopyUrl(String),
}

pub fn view(app: &mut SnagApp, ui: &mut egui::Ui) {
    let p = app.palette;
    let compact = app.settings.appearance.compact_queue;

    ui.horizontal(|ui| {
        super::page_header(ui, &p, "queue", "");
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
            let has_finished = app.jobs.iter().any(|j| j.state.is_terminal());
            if theme::action_button(ui, &p, "clear finished", false, has_finished).clicked() {
                app.jobs.retain(|j| !j.state.is_terminal());
            }
            let has_active = app.jobs.iter().any(|j| !j.state.is_terminal());
            if theme::action_button(ui, &p, "cancel all", false, has_active).clicked() {
                for j in app.jobs.iter_mut().filter(|j| !j.state.is_terminal()) {
                    j.cancel();
                    if j.state == JobState::Queued {
                        j.state = JobState::Cancelled;
                    }
                }
            }
        });
    });

    if app.jobs.is_empty() {
        ui.add_space(40.0);
        ui.vertical_centered(|ui| {
            ui.label(
                egui::RichText::new("nothing here yet")
                    .size(14.0)
                    .color(p.dim),
            );
            ui.add_space(4.0);
            ui.label(
                egui::RichText::new("links you snag will show up here")
                    .size(12.0)
                    .color(p.faint),
            );
        });
        return;
    }

    let mut actions: Vec<Action> = Vec::new();

    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .id_salt("queue_scroll")
        .show(ui, |ui| {
            for job in app.jobs.iter().rev() {
                job_card(ui, &p, job, compact, &mut actions);
                ui.add_space(8.0);
            }
        });

    for action in actions {
        match action {
            Action::Cancel(id) => {
                if let Some(j) = app.jobs.iter_mut().find(|j| j.id == id) {
                    j.cancel();
                    if j.state == JobState::Queued {
                        j.state = JobState::Cancelled;
                    }
                }
            }
            Action::Retry(id) => {
                if let Some(j) = app.jobs.iter_mut().find(|j| j.id == id) {
                    j.cancel_flag = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
                    j.state = JobState::Queued;
                    j.downloaded = 0.0;
                    j.total = 0.0;
                    j.speed = 0.0;
                    j.eta = -1.0;
                    j.file = None;
                    j.log.clear();
                }
            }
            Action::Remove(id) => {
                if let Some(j) = app.jobs.iter_mut().find(|j| j.id == id) {
                    j.cancel();
                }
                app.jobs.retain(|j| j.id != id);
            }
            Action::ToggleLog(id) => {
                if let Some(j) = app.jobs.iter_mut().find(|j| j.id == id) {
                    j.show_log = !j.show_log;
                }
            }
            Action::Reveal(path) => util::reveal(&path),
            Action::Open(path) => util::open_path(&path),
            Action::Clip(path) => app.clip_file(path),
            Action::CopyUrl(url) => {
                util::set_clipboard_text(&url);
                app.toast("link copied", false);
            }
        }
    }
}

fn job_card(
    ui: &mut egui::Ui,
    p: &crate::theme::Palette,
    job: &Job,
    compact: bool,
    actions: &mut Vec<Action>,
) {
    theme::card(ui, p, |ui| {
        ui.horizontal(|ui| {
            let name = job.display_name();
            let short: String = name.chars().take(70).collect();
            ui.label(egui::RichText::new(short).size(14.0).color(p.text));

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let color = match job.state {
                    JobState::Done => p.good,
                    JobState::Failed(_) => p.bad,
                    JobState::Cancelled => p.faint,
                    _ => p.dim,
                };
                ui.label(
                    egui::RichText::new(job.state.label())
                        .size(12.0)
                        .color(color),
                );
                ui.label(
                    egui::RichText::new(job.mode.label())
                        .size(12.0)
                        .color(p.faint),
                );
            });
        });

        if job.state.is_active() {
            ui.add_space(8.0);
            theme::progress_bar(ui, p, job.fraction(), job.total <= 0.0);
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                let size = if job.total > 0.0 {
                    format!(
                        "{} / {}",
                        util::human_bytes(job.downloaded),
                        util::human_bytes(job.total)
                    )
                } else {
                    util::human_bytes(job.downloaded)
                };
                ui.label(egui::RichText::new(size).size(12.0).color(p.dim));
                if job.speed > 0.0 {
                    ui.label(
                        egui::RichText::new(util::human_speed(job.speed))
                            .size(12.0)
                            .color(p.dim),
                    );
                }
                if job.eta >= 0.0 {
                    ui.label(
                        egui::RichText::new(format!("eta {}", util::human_eta(job.eta)))
                            .size(12.0)
                            .color(p.dim),
                    );
                }
                if job.total > 0.0 {
                    ui.label(
                        egui::RichText::new(format!("{:.0}%", job.fraction() * 100.0))
                            .size(12.0)
                            .color(p.faint),
                    );
                }
            });
        }

        if let JobState::Failed(reason) = &job.state {
            ui.add_space(6.0);
            let short: String = reason.chars().take(220).collect();
            ui.label(egui::RichText::new(short).size(12.0).color(p.bad));
        }

        if !compact {
            ui.add_space(10.0);
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing.x = 6.0;

                if !job.state.is_terminal() && theme::pill(ui, p, "cancel", false, true).clicked() {
                    actions.push(Action::Cancel(job.id));
                }
                if job.state.is_terminal() && theme::pill(ui, p, "retry", false, true).clicked() {
                    actions.push(Action::Retry(job.id));
                }
                if let Some(file) = &job.file {
                    if theme::pill(ui, p, "open", false, true).clicked() {
                        actions.push(Action::Open(file.clone()));
                    }
                    if theme::pill(ui, p, "show in folder", false, true).clicked() {
                        actions.push(Action::Reveal(file.clone()));
                    }
                    if theme::pill(ui, p, "clip", false, true).clicked() {
                        actions.push(Action::Clip(file.clone()));
                    }
                }
                if theme::pill(ui, p, "copy link", false, true).clicked() {
                    actions.push(Action::CopyUrl(job.url.clone()));
                }
                if theme::pill(
                    ui,
                    p,
                    if job.show_log { "hide log" } else { "log" },
                    job.show_log,
                    true,
                )
                .clicked()
                {
                    actions.push(Action::ToggleLog(job.id));
                }
                if theme::pill(ui, p, "remove", false, true).clicked() {
                    actions.push(Action::Remove(job.id));
                }
            });
        }

        if job.show_log {
            ui.add_space(8.0);
            egui::Frame::none()
                .fill(p.well)
                .rounding(egui::Rounding::same(8.0))
                .inner_margin(egui::Margin::symmetric(10.0, 8.0))
                .show(ui, |ui| {
                    egui::ScrollArea::vertical()
                        .max_height(160.0)
                        .stick_to_bottom(true)
                        .id_salt(("joblog", job.id))
                        .show(ui, |ui| {
                            if job.log.is_empty() {
                                ui.label(
                                    egui::RichText::new("no output yet")
                                        .size(11.0)
                                        .color(p.faint),
                                );
                            }
                            for line in &job.log {
                                ui.label(egui::RichText::new(line).size(11.0).color(
                                    if line.starts_with("ERROR") {
                                        p.bad
                                    } else {
                                        p.dim
                                    },
                                ));
                            }
                        });
                });
        }
    });
}
