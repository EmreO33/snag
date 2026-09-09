use eframe::egui;

use crate::app::{ClipHandle, SnagApp};
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

/// How tall the scrubber's strip of frames is drawn.
const STRIP_HEIGHT: f32 = 84.0;

/// How close to a handle the pointer has to be to grab it, in pixels.
const GRAB_SLOP: f32 = 12.0;

/// The sub-rectangle of an image that fills `dst` without squashing it.
fn cover_uv(src: egui::Vec2, dst: egui::Vec2) -> egui::Rect {
    let whole = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
    if src.x <= 0.0 || src.y <= 0.0 || dst.x <= 0.0 || dst.y <= 0.0 {
        return whole;
    }
    let (src_aspect, dst_aspect) = (src.x / src.y, dst.x / dst.y);
    if src_aspect > dst_aspect {
        // Wider than the slot it goes in, so the sides come off.
        let inset = (1.0 - dst_aspect / src_aspect) / 2.0;
        egui::Rect::from_min_max(egui::pos2(inset, 0.0), egui::pos2(1.0 - inset, 1.0))
    } else {
        let inset = (1.0 - src_aspect / dst_aspect) / 2.0;
        egui::Rect::from_min_max(egui::pos2(0.0, inset), egui::pos2(1.0, 1.0 - inset))
    }
}

/// The clip scrubber: frames from the file, with a draggable start and end.
///
/// The two text boxes remain what is actually read when the clip runs, and
/// dragging writes into them. One source of truth, so the picture and the
/// numbers cannot end up disagreeing about where the cut is.
fn timeline(app: &mut SnagApp, ui: &mut egui::Ui) {
    let p = app.palette;
    // No duration means ffprobe has not answered yet, or cannot read the file.
    let Some(duration) = app.remux.duration.filter(|d| *d > 0.0) else {
        return;
    };

    let start = crate::remux::parse_timecode(&app.remux.clip_start)
        .unwrap_or(0.0)
        .clamp(0.0, duration);
    let end = crate::remux::parse_timecode(&app.remux.clip_end)
        .unwrap_or(duration)
        .clamp(0.0, duration);

    let width = ui.available_width();
    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(width, STRIP_HEIGHT), egui::Sense::drag());
    let painter = ui.painter_at(rect);
    let rounding = egui::Rounding::same(8.0);

    let x_of = |t: f64| rect.left() + (t / duration) as f32 * rect.width();
    let t_of = |x: f32| ((x - rect.left()) / rect.width()).clamp(0.0, 1.0) as f64 * duration;

    painter.rect_filled(rect, rounding, p.well);

    // --- the frames ---------------------------------------------------------
    if !app.remux.filmstrip.is_empty() {
        let slot = rect.width() / app.remux.filmstrip.len() as f32;
        for (i, texture) in app.remux.filmstrip.iter().enumerate() {
            let cell = egui::Rect::from_min_size(
                egui::pos2(rect.left() + slot * i as f32, rect.top()),
                egui::vec2(slot, rect.height()),
            )
            .intersect(rect);
            painter.image(
                texture.id(),
                cell,
                cover_uv(texture.size_vec2(), cell.size()),
                egui::Color32::WHITE,
            );
        }
    }

    // --- what is being thrown away ------------------------------------------
    let shade = egui::Color32::from_black_alpha(150);
    painter.rect_filled(
        egui::Rect::from_min_max(rect.left_top(), egui::pos2(x_of(start), rect.bottom())),
        rounding,
        shade,
    );
    painter.rect_filled(
        egui::Rect::from_min_max(egui::pos2(x_of(end), rect.top()), rect.right_bottom()),
        rounding,
        shade,
    );

    // --- what is being kept -------------------------------------------------
    painter.rect_stroke(
        egui::Rect::from_min_max(
            egui::pos2(x_of(start), rect.top()),
            egui::pos2(x_of(end), rect.bottom()),
        ),
        0.0,
        egui::Stroke::new(2.0_f32, p.accent),
    );

    for (x, handle) in [
        (x_of(start), ClipHandle::Start),
        (x_of(end), ClipHandle::End),
    ] {
        let held = app.remux.clip_dragging == Some(handle);
        let half = if held { 5.0 } else { 3.0 };
        // Pulled inside the track at the extremes, so a handle sitting at 0 or
        // at the very end is still a bar you can see and aim at.
        let x = x.clamp(rect.left() + half, rect.right() - half);
        painter.rect_filled(
            egui::Rect::from_min_max(
                egui::pos2(x - half, rect.top()),
                egui::pos2(x + half, rect.bottom()),
            ),
            egui::Rounding::same(half),
            p.accent,
        );
    }

    // --- dragging -----------------------------------------------------------
    if let Some(pos) = response.interact_pointer_pos() {
        if response.drag_started() {
            // Whichever end the drag began nearest is the one it moves, so
            // there is no mode to be in and nothing to select first.
            let (to_start, to_end) = ((pos.x - x_of(start)).abs(), (pos.x - x_of(end)).abs());
            app.remux.clip_dragging = if to_start.min(to_end) > GRAB_SLOP {
                None
            } else if to_start <= to_end {
                Some(ClipHandle::Start)
            } else {
                Some(ClipHandle::End)
            };
        }

        if let Some(handle) = app.remux.clip_dragging {
            // Daylight between the two, because a clip of no length is not
            // something anyone is asking for and ffmpeg would refuse it.
            let gap = 1.0_f64.min(duration / 20.0);
            let t = t_of(pos.x);
            match handle {
                ClipHandle::Start => {
                    app.remux.clip_start = crate::remux::format_timecode(t.min(end - gap).max(0.0));
                }
                ClipHandle::End => {
                    app.remux.clip_end =
                        crate::remux::format_timecode(t.max(start + gap).min(duration));
                }
            }
            // The last result described a different cut than the one now set.
            app.remux.state = RemuxState::Idle;
        }
    }
    if response.drag_stopped() {
        app.remux.clip_dragging = None;
    }
    if response.hovered() || app.remux.clip_dragging.is_some() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
    }

    // --- the numbers under it -----------------------------------------------
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new(format!(
                "keeping {}",
                crate::remux::format_timecode(end - start)
            ))
            .size(12.0)
            .color(p.text),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(
                egui::RichText::new(format!("of {}", crate::remux::format_timecode(duration)))
                    .size(12.0)
                    .color(p.faint),
            );
        });
    });
}

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
        .id_salt("remux_scroll")
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
                RemuxOp::Clip => {
                    if app.remux.duration.is_some() {
                        theme::section_title(ui, &p, "the part you are keeping");
                        timeline(app, ui);
                        ui.add_space(12.0);
                    }

                    theme::section_title(ui, &p, "from and to");
                    theme::card(ui, &p, |ui| {
                        super::field_row(ui, &p, "from", &mut app.remux.clip_start, "0:00");
                        ui.add_space(6.0);
                        super::field_row(ui, &p, "to", &mut app.remux.clip_end, "end of file");
                    });
                    theme::note_text(
                        ui,
                        &p,
                        "written the way you would say it: 90, 1:30 or 0:01:30. leave from empty to start at the beginning, and to empty to run to the end.",
                    );

                    theme::section_title(ui, &p, "accuracy");
                    theme::toggle_row(
                        ui,
                        &p,
                        "cut exactly where i asked",
                        "off, the cut lands on the nearest keyframe before your start, so it may begin up to a few seconds early. nothing is re-encoded and it finishes almost instantly. on, it starts on the exact frame, which means re-encoding the video and taking as long as the clip is.",
                        &mut app.remux.clip_exact,
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
