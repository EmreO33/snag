use eframe::egui;

use crate::app::SnagApp;
use crate::settings::UpdateCheck;
use crate::theme;
use crate::updater::UpdateState;

pub fn view(app: &mut SnagApp, ui: &mut egui::Ui) {
    let p = app.palette;
    let ctx = ui.ctx().clone();

    super::page_header(
        ui,
        &p,
        "updates",
        "snag downloads through yt-dlp, and sites break it often. keeping it current is the single best fix for a download that suddenly stopped working.",
    );

    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            // --- current state ---------------------------------------------
            theme::card(ui, &p, |ui| {
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        let version = if app.ytdlp_version.is_empty() {
                            "not found".to_string()
                        } else {
                            app.ytdlp_version.clone()
                        };
                        ui.label(
                            egui::RichText::new(format!("yt-dlp {version}"))
                                .size(16.0)
                                .color(if app.ytdlp_version.is_empty() {
                                    p.bad
                                } else {
                                    p.text
                                })
                                .strong(),
                        );
                        ui.add_space(2.0);
                        let (msg, color) = match &app.update_state {
                            UpdateState::Unknown => ("not checked yet".to_string(), p.dim),
                            UpdateState::Checking => ("checking github...".to_string(), p.dim),
                            UpdateState::UpToDate => ("up to date".to_string(), p.good),
                            UpdateState::Available { latest } => {
                                (format!("{latest} is available"), p.accent)
                            }
                            UpdateState::Installing => ("installing...".to_string(), p.dim),
                            UpdateState::Installed { version } => {
                                (format!("updated to {version}"), p.good)
                            }
                            UpdateState::Missing(e) => {
                                (format!("yt-dlp is not runnable: {e}"), p.bad)
                            }
                            UpdateState::Error(e) => (e.clone(), p.bad),
                        };
                        let short: String = msg.chars().take(120).collect();
                        ui.label(egui::RichText::new(short).size(12.0).color(color));
                    });

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let busy = app.update_state.busy();
                        let can_install = matches!(
                            app.update_state,
                            UpdateState::Available { .. } | UpdateState::UpToDate
                        ) && !busy;

                        if theme::action_button(ui, &p, "update now", true, can_install).clicked() {
                            app.start_update_install(&ctx);
                        }
                        if theme::action_button(ui, &p, "check", false, !busy).clicked() {
                            app.start_update_check(&ctx, false);
                        }
                    });
                });

                if app.update_state.busy() {
                    ui.add_space(10.0);
                    theme::progress_bar(ui, &p, 0.0, true);
                }
            });

            if app.settings.updater.last_check_unix > 0 {
                let ago = crate::updater::now_unix()
                    .saturating_sub(app.settings.updater.last_check_unix);
                let text = if ago < 60 {
                    "last checked just now".to_string()
                } else if ago < 3600 {
                    format!("last checked {} min ago", ago / 60)
                } else if ago < 86400 {
                    format!("last checked {} h ago", ago / 3600)
                } else {
                    format!("last checked {} days ago", ago / 86400)
                };
                theme::note_text(ui, &p, &text);
            }

            ui.add_space(10.0);

            // --- policy -----------------------------------------------------
            theme::section_title(ui, &p, "check for updates");
            let mut check = app.settings.updater.check;
            if theme::pill_group(ui, &p, &mut check, UpdateCheck::ALL, |c| c.label()) {
                app.settings.updater.check = check;
                app.mark_dirty();
            }
            theme::note_text(
                ui,
                &p,
                "checks read the latest release tag from github. nothing is installed unless you say so below.",
            );

            let mut auto = app.settings.updater.auto_install;
            if theme::toggle_row(
                ui,
                &p,
                "install updates automatically",
                "when a check finds a newer yt-dlp, snag installs it in the background instead of asking.",
                &mut auto,
            ) {
                app.settings.updater.auto_install = auto;
                app.mark_dirty();
            }

            // --- output ------------------------------------------------------
            if !app.update_log.is_empty() {
                ui.add_space(6.0);
                theme::section_title(ui, &p, "updater output");
                egui::Frame::none()
                    .fill(p.well)
                    .rounding(egui::Rounding::same(8.0))
                    .inner_margin(egui::Margin::symmetric(10.0, 8.0))
                    .show(ui, |ui| {
                        for line in &app.update_log {
                            ui.label(egui::RichText::new(line).size(11.0).color(p.dim));
                        }
                    });
            }

            ui.add_space(14.0);
            theme::note_text(
                ui,
                &p,
                "snag updates yt-dlp by running its own self-update. if yt-dlp came from a package manager it cannot replace itself, and the updater will say so.",
            );
            ui.add_space(20.0);
        });
}
