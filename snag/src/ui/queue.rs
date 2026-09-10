use eframe::egui;

use crate::app::SnagApp;
use crate::jobs::{Job, JobState};
use crate::theme;
use crate::util;

enum Action {
    Cancel(u64),
    ResumeAll,
    Retry(u64),
    Remove(u64),
    ToggleLog(u64),
    Reveal(std::path::PathBuf),
    Open(std::path::PathBuf),
    Clip(std::path::PathBuf),
    CopyDetails(u64),
    CopyUrl(String),
}

pub fn view(app: &mut SnagApp, ui: &mut egui::Ui) {
    let p = app.palette;
    let compact = app.settings.appearance.compact_queue;

    ui.horizontal(|ui| {
        super::page_header(ui, &p, "queue", "");
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
            // Interrupted jobs are terminal in the sense that nothing is
            // happening to them, but they are not finished, and clearing
            // finished work should not quietly throw them away.
            let has_finished = app
                .jobs
                .iter()
                .any(|j| j.state.is_terminal() && !j.state.is_resumable());
            if theme::action_button(ui, &p, "clear finished", false, has_finished).clicked() {
                app.jobs
                    .retain(|j| !j.state.is_terminal() || j.state.is_resumable());
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

    let interrupted = app.jobs.iter().filter(|j| j.state.is_resumable()).count();
    if interrupted > 0 {
        theme::card(ui, &p, |ui| {
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.label(
                        egui::RichText::new(if interrupted == 1 {
                            "1 download was still going when snag last closed".to_string()
                        } else {
                            format!(
                                "{interrupted} downloads were still going when snag last closed"
                            )
                        })
                        .size(13.0)
                        .color(p.text),
                    );
                    ui.add_space(2.0);
                    ui.label(
                        egui::RichText::new(
                            "nothing was lost. picking one up carries on from where it stopped.",
                        )
                        .size(12.0)
                        .color(p.dim),
                    );
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if theme::action_button(ui, &p, "resume all", true, true).clicked() {
                        actions.push(Action::ResumeAll);
                    }
                });
            });
        });
        ui.add_space(10.0);
    }

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
            Action::ResumeAll => {
                for j in app.jobs.iter_mut().filter(|j| j.state.is_resumable()) {
                    j.restart();
                }
            }
            Action::Retry(id) => {
                if let Some(j) = app.jobs.iter_mut().find(|j| j.id == id) {
                    j.restart();
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
            Action::CopyDetails(id) => {
                if let Some(j) = app.jobs.iter().find(|j| j.id == id) {
                    let reason = match &j.state {
                        JobState::Failed(r) => r.as_str(),
                        _ => "",
                    };
                    let ytdlp = if app.ytdlp_version.is_empty() {
                        "unknown"
                    } else {
                        app.ytdlp_version.as_str()
                    };
                    util::set_clipboard_text(&format!(
                        "snag {}\nyt-dlp {}\nos {}\nmode {}\nurl {}\n\n{}",
                        env!("CARGO_PKG_VERSION"),
                        ytdlp,
                        std::env::consts::OS,
                        j.mode.label(),
                        j.url,
                        reason
                    ));
                    app.toast("details copied, paste them into the bug report", false);
                }
            }
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
                // A failure is worth reporting, and a report is only useful
                // with the versions and the mode attached to it.
                if matches!(job.state, JobState::Failed(_))
                    && theme::pill(ui, p, "copy details", false, true).clicked()
                {
                    actions.push(Action::CopyDetails(job.id));
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
