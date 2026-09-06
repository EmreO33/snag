//! The first-run screen. Three decisions, then out of the way for good:
//! where settings live, where downloads go, and which yt-dlp to use.

use eframe::egui;

use crate::app::SnagApp;
use crate::installer::InstallState;
use crate::theme;
use crate::util;

/// A path row with the value on the left and the buttons on the right.
fn path_row(
    ui: &mut egui::Ui,
    p: &theme::Palette,
    path: &mut std::path::PathBuf,
    default: Option<std::path::PathBuf>,
    enabled: bool,
) {
    ui.horizontal(|ui| {
        let text = path.display().to_string();
        let short: String = if text.chars().count() > 46 {
            let tail: String = text.chars().skip(text.chars().count() - 43).collect();
            format!("...{tail}")
        } else {
            text
        };
        ui.label(egui::RichText::new(short).size(13.0).color(p.text));

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if theme::pill(ui, p, "change", false, enabled).clicked() {
                let mut dialog = rfd::FileDialog::new();
                if path.exists() {
                    dialog = dialog.set_directory(&*path);
                }
                if let Some(dir) = dialog.pick_folder() {
                    *path = dir;
                }
            }
            if let Some(def) = default {
                if *path != def && theme::pill(ui, p, "default", false, enabled).clicked() {
                    *path = def;
                }
            }
        });
    });
}

pub fn view(app: &mut SnagApp, ui: &mut egui::Ui) {
    let p = app.palette;
    let logo = app.logo.clone();
    let ctx = ui.ctx().clone();
    let installing = app.setup.install.busy();

    // The header sits outside the scroll region so it never scrolls away.
    ui.vertical_centered(|ui| {
        ui.add_space(6.0);
        theme::logo(ui, &p, 44.0, logo.as_ref());
        ui.add_space(8.0);
        ui.label(
            egui::RichText::new("welcome to snag")
                .size(20.0)
                .color(p.text)
                .strong(),
        );
        ui.add_space(4.0);
        ui.label(
            egui::RichText::new("three things to set up, then you never see this again")
                .size(12.0)
                .color(p.dim),
        );
        ui.add_space(18.0);
    });

    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .id_salt("setup_scroll")
        .show(ui, |ui| {
            // Centred with explicit padding rather than vertical_centered, and
            // sized with set_max_width rather than a fixed allocation: giving
            // the column a zero height made the scroll area mismeasure its own
            // content and drift away from the top as the cards changed size.
            let width = ui.available_width().min(680.0);
            let pad = ((ui.available_width() - width) * 0.5).max(0.0);
            ui.horizontal_top(|ui| {
                ui.add_space(pad);
                ui.vertical(|ui| {
                    ui.set_max_width(width);
                    {
                        // --- 1. where settings live -----------------------
                        theme::section_title(ui, &p, "1. where snag keeps its settings");
                        let portable = crate::bootstrap::is_portable();
                        theme::card(ui, &p, |ui| {
                            if portable {
                                ui.label(
                                    egui::RichText::new(
                                        app.setup.config_dir.display().to_string(),
                                    )
                                    .size(13.0)
                                    .color(p.text),
                                );
                            } else {
                                let default = crate::bootstrap::platform_config_dir();
                                path_row(
                                    ui,
                                    &p,
                                    &mut app.setup.config_dir,
                                    Some(default),
                                    !installing,
                                );
                            }
                        });
                        theme::note_text(
                            ui,
                            &p,
                            if portable {
                                "this is a portable copy, so everything stays in a data folder beside the executable and nothing is written anywhere else. a yt-dlp that snag installs lands there too, under bin."
                            } else {
                                "the default is your appdata folder, which is the right answer unless you want snag portable. a yt-dlp that snag installs also lives here, under bin."
                            },
                        );

                        // --- 2. downloads ---------------------------------
                        theme::section_title(ui, &p, "2. where downloads go");
                        theme::card(ui, &p, |ui| {
                            let default = util::default_download_dir();
                            path_row(ui, &p, &mut app.setup.download_dir, Some(default), !installing);
                        });
                        theme::note_text(ui, &p, "you can change this later in settings > local processing.");

                        // --- 3. yt-dlp ------------------------------------
                        theme::section_title(ui, &p, "3. yt-dlp");
                        theme::card(ui, &p, |ui| {
                            if app.setup.detecting {
                                ui.label(
                                    egui::RichText::new("looking for yt-dlp...")
                                        .size(13.0)
                                        .color(p.dim),
                                );
                                return;
                            }

                            match app.setup.resolved_ytdlp() {
                                Some((path, version)) => {
                                    ui.horizontal(|ui| {
                                        ui.label(
                                            egui::RichText::new(format!("yt-dlp {version}"))
                                                .size(14.0)
                                                .color(p.good),
                                        );
                                        ui.with_layout(
                                            egui::Layout::right_to_left(egui::Align::Center),
                                            |ui| {
                                                if theme::pill(ui, &p, "reinstall", false, !installing)
                                                    .clicked()
                                                {
                                                    app.start_ytdlp_install(&ctx);
                                                }
                                            },
                                        );
                                    });
                                    ui.add_space(4.0);
                                    let short: String = path.chars().take(60).collect();
                                    ui.label(egui::RichText::new(short).size(11.0).color(p.faint));
                                }
                                None => {
                                    ui.label(
                                        egui::RichText::new("yt-dlp was not found on this machine")
                                            .size(14.0)
                                            .color(p.text),
                                    );
                                    ui.add_space(4.0);
                                    ui.label(
                                        egui::RichText::new(
                                            "snag does not ship it. it can download the current release from the yt-dlp project on github.",
                                        )
                                        .size(12.0)
                                        .color(p.dim),
                                    );
                                    ui.add_space(10.0);
                                    ui.horizontal(|ui| {
                                        if theme::action_button(
                                            ui,
                                            &p,
                                            "install yt-dlp",
                                            true,
                                            !installing,
                                        )
                                        .clicked()
                                        {
                                            app.start_ytdlp_install(&ctx);
                                        }
                                        if theme::action_button(
                                            ui,
                                            &p,
                                            "look again",
                                            false,
                                            !installing,
                                        )
                                        .clicked()
                                        {
                                            app.start_detection(&ctx);
                                        }
                                    });
                                }
                            }

                            // Live progress for an install in flight.
                            match &app.setup.install {
                                InstallState::Downloading { got, total } => {
                                    ui.add_space(10.0);
                                    let frac = app.setup.install.fraction();
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
                                InstallState::Verifying => {
                                    ui.add_space(10.0);
                                    ui.label(
                                        egui::RichText::new("checking the binary runs...")
                                            .size(12.0)
                                            .color(p.dim),
                                    );
                                }
                                InstallState::Failed(e) => {
                                    ui.add_space(10.0);
                                    let short: String = e.chars().take(200).collect();
                                    ui.label(egui::RichText::new(short).size(12.0).color(p.bad));
                                }
                                _ => {}
                            }
                        });
                        theme::note_text(
                            ui,
                            &p,
                            "downloaded from github.com/yt-dlp/yt-dlp, the project's own release page. snag keeps it updated for you from the updates screen.",
                        );

                        // --- ffmpeg ---------------------------------------
                        let plan = crate::installer::ffmpeg_plan();
                        let ffmpeg_busy = app.setup.ffmpeg_install.busy();
                        theme::card(ui, &p, |ui| {
                            ui.horizontal(|ui| {
                                match &app.setup.found_ffmpeg {
                                    Some(v) => {
                                        let short: String = v.chars().take(46).collect();
                                        ui.label(
                                            egui::RichText::new("ffmpeg found")
                                                .size(13.0)
                                                .color(p.good),
                                        );
                                        ui.label(
                                            egui::RichText::new(short).size(11.0).color(p.faint),
                                        );
                                    }
                                    None if app.setup.detecting => {
                                        ui.label(
                                            egui::RichText::new("looking for ffmpeg...")
                                                .size(13.0)
                                                .color(p.dim),
                                        );
                                    }
                                    None => {
                                        ui.label(
                                            egui::RichText::new("ffmpeg was not found")
                                                .size(13.0)
                                                .color(p.bad),
                                        );
                                    }
                                }

                                if app.setup.found_ffmpeg.is_none() && !app.setup.detecting {
                                    ui.with_layout(
                                        egui::Layout::right_to_left(egui::Align::Center),
                                        |ui| match &plan {
                                            // Windows can do this without asking for
                                            // elevation, so just do it.
                                            crate::installer::FfmpegPlan::Automatic { .. } => {
                                                if theme::action_button(
                                                    ui,
                                                    &p,
                                                    "install ffmpeg",
                                                    true,
                                                    !ffmpeg_busy && !installing,
                                                )
                                                .clicked()
                                                {
                                                    app.start_ffmpeg_install(&ctx);
                                                }
                                            }
                                            // Elsewhere it needs root, so hand over
                                            // the command rather than asking for it.
                                            crate::installer::FfmpegPlan::Manual { command } => {
                                                if theme::pill(
                                                    ui,
                                                    &p,
                                                    "copy the command",
                                                    false,
                                                    true,
                                                )
                                                .clicked()
                                                {
                                                    util::set_clipboard_text(command);
                                                    app.toast("command copied", false);
                                                }
                                                ui.label(
                                                    egui::RichText::new(command)
                                                        .size(11.0)
                                                        .color(p.dim),
                                                );
                                            }
                                        },
                                    );
                                }
                            });

                            if ffmpeg_busy {
                                ui.add_space(10.0);
                                theme::progress_bar(ui, &p, 0.0, true);
                                ui.add_space(4.0);
                                ui.label(
                                    egui::RichText::new("installing through the package manager...")
                                        .size(12.0)
                                        .color(p.dim),
                                );
                            }
                            if let InstallState::Failed(e) = &app.setup.ffmpeg_install {
                                ui.add_space(8.0);
                                let short: String = e.chars().take(200).collect();
                                ui.label(egui::RichText::new(short).size(11.0).color(p.bad));
                            }
                        });
                        theme::note_text(
                            ui,
                            &p,
                            match &plan {
                                crate::installer::FfmpegPlan::Automatic { .. } => {
                                    "ffmpeg merges video with audio and does every conversion. snag installs it through winget, which needs no admin rights, rather than hosting a build of its own."
                                }
                                crate::installer::FfmpegPlan::Manual { .. } => {
                                    "ffmpeg merges video with audio and does every conversion. installing it needs root, which snag will not ask for: run the command above, then look again."
                                }
                            },
                        );

                        // --- finish ---------------------------------------
                        if let Some(err) = &app.setup.error {
                            ui.add_space(6.0);
                            ui.label(egui::RichText::new(err).size(12.0).color(p.bad));
                        }

                        ui.add_space(10.0);
                        ui.horizontal(|ui| {
                            let ready = app.setup.resolved_ytdlp().is_some();
                            let label = if ready { "start using snag" } else { "continue anyway" };
                            if theme::action_button(ui, &p, label, ready, !installing).clicked() {
                                app.finish_setup(&ctx);
                            }
                            if !ready && !app.setup.detecting {
                                ui.label(
                                    egui::RichText::new("downloads will not work until yt-dlp is available")
                                        .size(11.0)
                                        .color(p.faint),
                                );
                            }
                        });

                        // When something goes wrong, everything needed to
                        // understand it should be on screen and copyable. No
                        // terminal archaeology.
                        if !app.setup.log.is_empty() {
                            ui.add_space(14.0);
                            let failed = matches!(app.setup.install, InstallState::Failed(_));
                            theme::section_title(
                                ui,
                                &p,
                                if failed { "what happened" } else { "details" },
                            );
                            egui::Frame::none()
                                .fill(p.well)
                                .rounding(egui::Rounding::same(8.0))
                                .inner_margin(egui::Margin::symmetric(12.0, 10.0))
                                .show(ui, |ui| {
                                    for line in &app.setup.log {
                                        ui.label(
                                            egui::RichText::new(line).size(11.0).color(p.dim),
                                        );
                                    }
                                    if let InstallState::Failed(e) = &app.setup.install {
                                        ui.add_space(6.0);
                                        ui.label(
                                            egui::RichText::new(format!("error:       {e}"))
                                                .size(11.0)
                                                .color(p.bad),
                                        );
                                    }
                                    ui.add_space(10.0);
                                    if theme::pill(ui, &p, "copy these details", false, true)
                                        .clicked()
                                    {
                                        let mut report = app.setup.log.join("
");
                                        if let InstallState::Failed(e) = &app.setup.install {
                                            report.push_str(&format!("
error:       {e}"));
                                        }
                                        report.push_str(&format!(
                                            "
snag:        {} on {}",
                                            crate::selfupdate::current_version(),
                                            std::env::consts::OS
                                        ));
                                        util::set_clipboard_text(&report);
                                        app.toast("details copied", false);
                                    }
                                });
                        }

                        ui.add_space(30.0);
                    }
                });
            });
        });
}
