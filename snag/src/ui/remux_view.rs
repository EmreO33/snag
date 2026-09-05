use eframe::egui;

use crate::app::SnagApp;
use crate::remux::{RemuxOp, RemuxState};
use crate::theme;
use crate::util;

const CONTAINERS: &[&str] = &["mp4", "mkv", "webm", "mov"];
const AUDIO_CODECS: &[(&str, &str)] = &[
    ("copy", "copy"),
    ("aac", "aac"),
    ("libmp3lame", "mp3"),
    ("libopus", "opus"),
    ("flac", "flac"),
];

/// A pill row over string values, which the remux options use.
fn string_pills(
    ui: &mut egui::Ui,
    p: &theme::Palette,
    value: &mut String,
    options: &[(&str, &str)],
) {
    egui::Frame::none()
        .fill(p.card)
        .rounding(egui::Rounding::same(12.0))
        .inner_margin(egui::Margin::same(4.0))
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing = egui::Vec2::new(4.0, 4.0);
            ui.horizontal_wrapped(|ui| {
                for (val, label) in options {
                    if theme::pill(ui, p, label, value == *val, true).clicked() {
                        *value = (*val).to_string();
                    }
                }
            });
        });
}

pub fn view(app: &mut SnagApp, ui: &mut egui::Ui) {
    let p = app.palette;
    let running = app.remux.state == RemuxState::Running;

    super::page_header(
        ui,
        &p,
        "remux",
        "rewrap or convert a file you already have. drag one onto the window to load it.",
    );

    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            // --- input file ------------------------------------------------
            theme::card(ui, &p, |ui| {
                ui.horizontal(|ui| {
                    let label = match &app.remux.input {
                        Some(path) => path
                            .file_name()
                            .map(|n| n.to_string_lossy().to_string())
                            .unwrap_or_else(|| path.display().to_string()),
                        None => "no file selected".to_string(),
                    };
                    let color = if app.remux.input.is_some() {
                        p.text
                    } else {
                        p.faint
                    };
                    let short: String = label.chars().take(58).collect();
                    ui.label(egui::RichText::new(short).size(14.0).color(color));

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if theme::action_button(ui, &p, "choose file", false, !running).clicked() {
                            let mut dialog = rfd::FileDialog::new().add_filter(
                                "media",
                                &[
                                    "mp4", "mkv", "webm", "mov", "avi", "flv", "ts", "m4a", "mp3",
                                    "opus", "ogg", "wav", "flac", "aac",
                                ],
                            );
                            if app.settings.processing.download_dir.exists() {
                                dialog = dialog.set_directory(&app.settings.processing.download_dir);
                            }
                            if let Some(path) = dialog.pick_file() {
                                app.remux.input = Some(path);
                                app.remux.state = RemuxState::Idle;
                                app.remux.log.clear();
                                app.remux.progress = 0.0;
                            }
                        }
                    });
                });
            });

            ui.add_space(14.0);

            // --- operation --------------------------------------------------
            theme::section_title(ui, &p, "operation");
            let mut op = app.remux.op;
            if theme::pill_group(ui, &p, &mut op, RemuxOp::ALL, |o| o.label()) {
                app.remux.op = op;
                app.remux.state = RemuxState::Idle;
            }
            theme::note_text(ui, &p, app.remux.op.note());

            // --- operation options ------------------------------------------
            match app.remux.op {
                RemuxOp::Container | RemuxOp::StripAudio => {
                    theme::section_title(ui, &p, "output container");
                    let opts: Vec<(&str, &str)> = CONTAINERS.iter().map(|c| (*c, *c)).collect();
                    string_pills(ui, &p, &mut app.remux.container, &opts);
                    theme::note_text(
                        ui,
                        &p,
                        "streams are copied as they are. a container that cannot hold the source codecs will fail.",
                    );
                }
                RemuxOp::ExtractAudio => {
                    theme::section_title(ui, &p, "audio codec");
                    string_pills(ui, &p, &mut app.remux.audio_codec, AUDIO_CODECS);
                    theme::note_text(
                        ui,
                        &p,
                        "copy keeps the original track untouched. anything else re-encodes it.",
                    );
                }
                RemuxOp::ToGif => {
                    theme::section_title(ui, &p, "gif options");
                    theme::card(ui, &p, |ui| {
                        let mut fps = app.remux.gif_fps;
                        if super::number_row(ui, &p, "frames per second", &mut fps, 5..=30) {
                            app.remux.gif_fps = fps;
                        }
                        ui.add_space(6.0);
                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new("width").size(13.0).color(p.dim));
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    for w in [720u32, 640, 480, 320] {
                                        if theme::pill(
                                            ui,
                                            &p,
                                            &format!("{w}px"),
                                            app.remux.gif_width == w,
                                            true,
                                        )
                                        .clicked()
                                        {
                                            app.remux.gif_width = w;
                                        }
                                    }
                                },
                            );
                        });
                    });
                    theme::note_text(
                        ui,
                        &p,
                        "gif conversion is inefficient. the converted file may be obnoxiously big and low quality.",
                    );
                }
            }

            // --- run --------------------------------------------------------
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                let can_run = app.remux.input.is_some() && !running;
                if theme::action_button(ui, &p, "run", true, can_run).clicked() {
                    let ctx = ui.ctx().clone();
                    app.start_remux(&ctx);
                }
                if theme::action_button(ui, &p, "cancel", false, running).clicked() {
                    app.cancel_remux();
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let mut keep = app.settings.processing.keep_source_after_remux;
                    ui.label(
                        egui::RichText::new("keep the source file")
                            .size(12.0)
                            .color(p.dim),
                    );
                    if theme::toggle(ui, &p, &mut keep).changed() {
                        app.settings.processing.keep_source_after_remux = keep;
                        app.mark_dirty();
                    }
                });
            });

            if running {
                ui.add_space(12.0);
                theme::progress_bar(ui, &p, app.remux.progress, app.remux.progress <= 0.0);
                ui.add_space(6.0);
                let pct = if app.remux.progress > 0.0 {
                    format!("{:.0}%", app.remux.progress * 100.0)
                } else {
                    "working".to_string()
                };
                ui.label(egui::RichText::new(pct).size(12.0).color(p.dim));
            }

            // --- result -----------------------------------------------------
            match app.remux.state.clone() {
                RemuxState::Done(path) => {
                    ui.add_space(14.0);
                    theme::card(ui, &p, |ui| {
                        ui.horizontal(|ui| {
                            let name = path
                                .file_name()
                                .map(|n| n.to_string_lossy().to_string())
                                .unwrap_or_default();
                            ui.label(egui::RichText::new(name).size(13.0).color(p.good));
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if theme::pill(ui, &p, "show in folder", false, true).clicked()
                                    {
                                        util::reveal(&path);
                                    }
                                    if theme::pill(ui, &p, "open", false, true).clicked() {
                                        util::open_path(&path);
                                    }
                                },
                            );
                        });
                    });
                }
                RemuxState::Failed(err) => {
                    ui.add_space(14.0);
                    let short: String = err.chars().take(300).collect();
                    ui.label(egui::RichText::new(short).size(12.0).color(p.bad));
                }
                RemuxState::Cancelled => {
                    ui.add_space(14.0);
                    ui.label(
                        egui::RichText::new("cancelled")
                            .size(12.0)
                            .color(p.faint),
                    );
                }
                _ => {}
            }

            if !app.remux.log.is_empty() {
                ui.add_space(14.0);
                theme::section_title(ui, &p, "ffmpeg output");
                egui::Frame::none()
                    .fill(p.well)
                    .rounding(egui::Rounding::same(8.0))
                    .inner_margin(egui::Margin::symmetric(10.0, 8.0))
                    .show(ui, |ui| {
                        egui::ScrollArea::vertical()
                            .max_height(150.0)
                            .stick_to_bottom(true)
                            .id_salt("remuxlog")
                            .show(ui, |ui| {
                                for line in &app.remux.log {
                                    ui.label(
                                        egui::RichText::new(line).size(11.0).color(p.dim),
                                    );
                                }
                            });
                    });
            }

            ui.add_space(20.0);
        });
}
