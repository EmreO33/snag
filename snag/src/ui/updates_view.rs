use eframe::egui;

use crate::app::SnagApp;
use crate::selfupdate::{InstallKind, SelfUpdateState};
use crate::settings::UpdateCheck;
use crate::theme;
use crate::updater::UpdateState;
use crate::util;

pub fn view(app: &mut SnagApp, ui: &mut egui::Ui) {
    let p = app.palette;
    let ctx = ui.ctx().clone();

    super::page_header(
        ui,
        &p,
        "updates",
        "snag downloads through yt-dlp, and sites break it often. keeping yt-dlp current is the single best fix for a download that suddenly stopped working.",
    );

    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            // --- snag itself -----------------------------------------------
            theme::section_title(ui, &p, "snag");
            theme::card(ui, &p, |ui| {
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.label(
                            egui::RichText::new(format!(
                                "snag {}",
                                crate::selfupdate::current_version()
                            ))
                            .size(16.0)
                            .color(p.text)
                            .strong(),
                        );
                        ui.add_space(2.0);
                        let (msg, color) = match &app.app_update_state {
                            SelfUpdateState::Unknown => ("not checked yet".to_string(), p.dim),
                            SelfUpdateState::Checking => ("checking github...".to_string(), p.dim),
                            SelfUpdateState::UpToDate => ("up to date".to_string(), p.good),
                            SelfUpdateState::Available { latest } => {
                                (format!("{latest} is available"), p.accent)
                            }
                            SelfUpdateState::Downloading { .. } => {
                                ("downloading...".to_string(), p.dim)
                            }
                            SelfUpdateState::RestartRequired => (
                                "updated. restart snag to use the new version.".to_string(),
                                p.good,
                            ),
                            SelfUpdateState::HandedOff => (
                                "the installer is running. snag will close.".to_string(),
                                p.good,
                            ),
                            SelfUpdateState::Error(e) => (e.clone(), p.bad),
                        };
                        let short: String = msg.chars().take(120).collect();
                        ui.label(egui::RichText::new(short).size(12.0).color(color));
                    });

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let busy = app.app_update_state.busy();
                        let available =
                            matches!(app.app_update_state, SelfUpdateState::Available { .. });

                        // A copy that Scoop owns must be updated through Scoop,
                        // or the two end up fighting over the same files.
                        if app.install_kind == InstallKind::Scoop {
                            if theme::action_button(ui, &p, "copy the command", true, available)
                                .clicked()
                            {
                                util::set_clipboard_text("scoop update snag");
                                app.toast("copied: scoop update snag", false);
                            }
                        } else if theme::action_button(ui, &p, "update snag", true, available && !busy)
                            .clicked()
                        {
                            app.start_app_update_install(&ctx);
                        }

                        if theme::action_button(ui, &p, "check", false, !busy).clicked() {
                            app.start_app_update_check(&ctx);
                        }
                    });
                });

                if let SelfUpdateState::Downloading { got, total } = &app.app_update_state {
                    ui.add_space(10.0);
                    let frac = app.app_update_state.fraction();
                    theme::progress_bar(ui, &p, frac.unwrap_or(0.0), frac.is_none());
                    ui.add_space(6.0);
                    let label = if *total > 0 {
                        format!(
                            "{} / {}",
                            util::human_bytes(*got as f64),
                            util::human_bytes(*total as f64)
                        )
                    } else {
                        util::human_bytes(*got as f64)
                    };
                    ui.label(egui::RichText::new(label).size(12.0).color(p.dim));
                }

                if app.app_update_state == SelfUpdateState::RestartRequired {
                    ui.add_space(10.0);
                    if theme::action_button(ui, &p, "close snag", true, true).clicked() {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                }
            });

            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(app.install_kind.label())
                        .size(12.0)
                        .color(p.faint),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if theme::pill(ui, &p, "release notes", false, true).clicked() {
                        ctx.open_url(egui::OpenUrl::new_tab(crate::selfupdate::releases_url()));
                    }
                });
            });
            theme::note_text(ui, &p, app.install_kind.update_note());

            let mut check_app = app.settings.updater.check_app;
            if theme::toggle_row(
                ui,
                &p,
                "check for snag updates too",
                "uses the same schedule as the yt-dlp check below. nothing is ever installed without you asking.",
                &mut check_app,
            ) {
                app.settings.updater.check_app = check_app;
                app.mark_dirty();
            }

            ui.add_space(6.0);
            theme::section_title(ui, &p, "yt-dlp");

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
