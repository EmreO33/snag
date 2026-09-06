use eframe::egui;

use crate::app::SnagApp;
use crate::theme;
use crate::util;

enum Action {
    Open(std::path::PathBuf),
    Reveal(std::path::PathBuf),
    CopyUrl(String),
    Again(String, crate::settings::Mode),
    Forget(usize),
}

/// "just now", "3 h ago", "12 days ago".
fn ago(unix: u64) -> String {
    let now = crate::updater::now_unix();
    let secs = now.saturating_sub(unix);
    if secs < 60 {
        "just now".to_string()
    } else if secs < 3600 {
        format!("{} min ago", secs / 60)
    } else if secs < 86_400 {
        format!("{} h ago", secs / 3600)
    } else {
        format!("{} days ago", secs / 86_400)
    }
}

pub fn view(app: &mut SnagApp, ui: &mut egui::Ui) {
    let p = app.palette;

    ui.horizontal(|ui| {
        super::page_header(ui, &p, "history", "");
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
            let any = !app.history.entries.is_empty();
            if theme::action_button(ui, &p, "clear history", false, any).clicked() {
                app.history.clear();
                let _ = app.history.save();
            }
        });
    });

    if app.history.entries.is_empty() {
        ui.add_space(40.0);
        ui.vertical_centered(|ui| {
            ui.label(
                egui::RichText::new("nothing downloaded yet")
                    .size(14.0)
                    .color(p.dim),
            );
            ui.add_space(4.0);
            ui.label(
                egui::RichText::new("finished downloads are remembered here, across restarts")
                    .size(12.0)
                    .color(p.faint),
            );
        });
        return;
    }

    let mut actions: Vec<Action> = Vec::new();

    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .id_salt("history_scroll")
        .show(ui, |ui| {
            for (i, entry) in app.history.entries.iter().enumerate() {
                theme::card(ui, &p, |ui| {
                    ui.horizontal(|ui| {
                        let name: String = entry.display_name().chars().take(66).collect();
                        ui.label(egui::RichText::new(name).size(14.0).color(p.text));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.label(
                                egui::RichText::new(ago(entry.finished_unix))
                                    .size(12.0)
                                    .color(p.faint),
                            );
                        });
                    });

                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        let mut facts = vec![entry.mode.label().to_string()];
                        if entry.bytes > 0.0 {
                            facts.push(util::human_bytes(entry.bytes));
                        }
                        // Say so plainly rather than offering a button that
                        // would just fail.
                        if !entry.file_exists() {
                            facts.push("file moved or deleted".to_string());
                        }
                        ui.label(
                            egui::RichText::new(facts.join("  ·  "))
                                .size(12.0)
                                .color(p.dim),
                        );
                    });

                    ui.add_space(10.0);
                    ui.horizontal_wrapped(|ui| {
                        ui.spacing_mut().item_spacing.x = 6.0;
                        let here = entry.file_exists();
                        if let Some(file) = &entry.file {
                            if theme::pill(ui, &p, "open", false, here).clicked() {
                                actions.push(Action::Open(file.clone()));
                            }
                            if theme::pill(ui, &p, "show in folder", false, here).clicked() {
                                actions.push(Action::Reveal(file.clone()));
                            }
                        }
                        if theme::pill(ui, &p, "download again", false, true).clicked() {
                            actions.push(Action::Again(entry.url.clone(), entry.mode));
                        }
                        if theme::pill(ui, &p, "copy link", false, true).clicked() {
                            actions.push(Action::CopyUrl(entry.url.clone()));
                        }
                        if theme::pill(ui, &p, "forget", false, true).clicked() {
                            actions.push(Action::Forget(i));
                        }
                    });
                });
                ui.add_space(8.0);
            }
        });

    let mut dirty = false;
    for action in actions {
        match action {
            Action::Open(path) => util::open_path(&path),
            Action::Reveal(path) => util::reveal(&path),
            Action::CopyUrl(url) => {
                util::set_clipboard_text(&url);
                app.toast("link copied", false);
            }
            Action::Again(url, mode) => {
                app.url_input = url;
                app.mode = mode;
                app.view = crate::app::View::Home;
                app.toast("loaded into the link box", false);
            }
            Action::Forget(i) => {
                app.history.remove(i);
                dirty = true;
            }
        }
    }
    if dirty {
        let _ = app.history.save();
    }
}
