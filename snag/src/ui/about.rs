use eframe::egui;

use crate::app::SnagApp;
use crate::settings::Settings;
use crate::theme;
use crate::util;

/// The author's github, linked from the byline.
const AUTHOR_URL: &str = "https://github.com/EmreO33";

pub fn view(app: &mut SnagApp, ui: &mut egui::Ui) {
    let p = app.palette;
    let logo = app.logo.clone();

    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .id_salt("about_scroll")
        .show(ui, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(20.0);
                theme::logo(ui, &p, 56.0, logo.as_ref());
                ui.add_space(8.0);
                ui.label(egui::RichText::new("snag").size(20.0).color(p.text).strong());
                ui.label(
                    egui::RichText::new(format!("v{}", env!("CARGO_PKG_VERSION")))
                        .size(12.0)
                        .color(p.faint),
                );
                ui.add_space(6.0);
                ui.label(
                    egui::RichText::new("a small native front end for yt-dlp")
                        .size(13.0)
                        .color(p.dim),
                );
                ui.add_space(6.0);
                // ui.link rather than a plain label, so it underlines on hover
                // and takes the pointing cursor without hand-rolling either.
                if ui
                    .link(
                        egui::RichText::new("made by EmreO33")
                            .size(12.0)
                            .color(p.accent),
                    )
                    .on_hover_text(AUTHOR_URL)
                    .clicked()
                {
                    ui.ctx().open_url(egui::OpenUrl::new_tab(AUTHOR_URL));
                }
            });

            ui.add_space(24.0);

            theme::section_title(ui, &p, "how it works");
            theme::card(ui, &p, |ui| {
                for line in [
                    "snag does not download anything itself. it builds a yt-dlp command from your settings, runs it, and reads the progress back.",
                    "merging, remuxing and audio conversion are handled by ffmpeg, which yt-dlp calls on its own.",
                    "nothing is uploaded anywhere. snag reaches the network on its own for three things only: checking github for newer versions of itself and of yt-dlp, downloading them if you ask, and fetching the preview image for a link you have pasted.",
                    "signing in to youtube borrows the session from a browser you are already signed in to. snag has no login form, never sees your password, and never stores the cookies: yt-dlp reads them from the browser and sends them to youtube, and nowhere else.",
                ] {
                    ui.label(egui::RichText::new(line).size(13.0).color(p.dim));
                    ui.add_space(6.0);
                }
            });

            ui.add_space(16.0);
            theme::section_title(ui, &p, "licence");
            theme::card(ui, &p, |ui| {
                for line in [
                    "snag is free software under the GNU General Public License, version 3 or later. you may use, study, change and share it.",
                    "it comes with absolutely no warranty. the full licence text ships with snag and is in the LICENSE file in the source.",
                    "the source lives at github.com/EmreO33/snag.",
                ] {
                    ui.label(egui::RichText::new(line).size(13.0).color(p.dim));
                    ui.add_space(6.0);
                }
                if theme::pill(ui, &p, "copy the source link", false, true).clicked() {
                    util::set_clipboard_text("https://github.com/EmreO33/snag");
                }
            });

            ui.add_space(16.0);
            theme::section_title(ui, &p, "credit where it is due");
            theme::card(ui, &p, |ui| {
                for line in [
                    "snag's interface is heavily inspired by cobalt.tools: the mode pills, the single link field, the plain-language settings copy, and the remux idea all come from there.",
                    "snag is not affiliated with, endorsed by, or connected to cobalt or its developers in any way. it is a separate project that borrows their design ideas, and any faults in it are snag's own.",
                    "the actual downloading is done by yt-dlp, and the media work by ffmpeg. both are separate projects, and snag simply drives them.",
                ] {
                    ui.label(egui::RichText::new(line).size(13.0).color(p.dim));
                    ui.add_space(6.0);
                }
            });

            ui.add_space(16.0);
            theme::section_title(ui, &p, "shortcuts");
            theme::card(ui, &p, |ui| {
                for (k, v) in [
                    ("enter", "start the download in the link field"),
                    ("ctrl + v", "paste into the link field"),
                    ("drag a file in", "load it straight into remux"),
                ] {
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new(k).size(13.0).color(p.text));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.label(egui::RichText::new(v).size(12.0).color(p.dim));
                        });
                    });
                    ui.add_space(4.0);
                }
            });

            ui.add_space(16.0);
            theme::section_title(ui, &p, "where things live");
            theme::card(ui, &p, |ui| {
                let config = Settings::config_path().display().to_string();
                let downloads = app.settings.processing.download_dir.display().to_string();
                for (label, path) in [("settings", config), ("downloads", downloads)] {
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new(label).size(13.0).color(p.text));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let short: String = if path.len() > 52 {
                                format!("...{}", &path[path.len() - 49..])
                            } else {
                                path.clone()
                            };
                            ui.label(egui::RichText::new(short).size(11.0).color(p.dim));
                        });
                    });
                    ui.add_space(4.0);
                }
                ui.add_space(6.0);
                if theme::pill(ui, &p, "open settings folder", false, true).clicked() {
                    let dir = Settings::config_dir();
                    let _ = std::fs::create_dir_all(&dir);
                    util::reveal(&dir);
                }
            });

            ui.add_space(16.0);
            theme::note_text(
                ui,
                &p,
                "you are responsible for what you download. respect the terms of the sites you use and the rights of the people who made what you are saving.",
            );
            ui.add_space(20.0);
        });
}
