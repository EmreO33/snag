use eframe::egui;

use crate::app::{SettingsTab, SnagApp};
use crate::settings::{
    Accent, AudioBitrate, AudioFormat, Container, CookieBrowser, Mode, Settings, ThemeMode,
    VideoCodec, VideoQuality,
};
use crate::theme;
use crate::util;
use crate::youtube::SignInState;
use crate::ytdlp;

pub fn view(app: &mut SnagApp, ui: &mut egui::Ui) {
    let p = app.palette;

    ui.horizontal(|ui| {
        super::page_header(ui, &p, "settings", "");
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
            if theme::action_button(ui, &p, "open config folder", false, true).clicked() {
                let dir = Settings::config_dir();
                let _ = std::fs::create_dir_all(&dir);
                util::reveal(&dir);
            }
        });
    });

    let mut tab = app.settings_tab;
    theme::pill_group(ui, &p, &mut tab, SettingsTab::ALL, |t| t.label());
    app.settings_tab = tab;
    ui.add_space(16.0);

    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .id_salt("settings_scroll")
        .show(ui, |ui| {
            let mut changed = false;
            match app.settings_tab {
                SettingsTab::Appearance => changed |= appearance(app, ui),
                SettingsTab::Video => changed |= video(app, ui),
                SettingsTab::Audio => changed |= audio(app, ui),
                SettingsTab::Metadata => changed |= metadata(app, ui),
                SettingsTab::Processing => changed |= processing(app, ui),
                SettingsTab::Background => changed |= background(app, ui),
                SettingsTab::Network => changed |= network(app, ui),
                SettingsTab::Youtube => changed |= youtube(app, ui),
                SettingsTab::Advanced => changed |= advanced(app, ui),
            }
            if changed {
                app.mark_dirty();
            }
            ui.add_space(30.0);
        });
}

fn appearance(app: &mut SnagApp, ui: &mut egui::Ui) -> bool {
    let p = app.palette;
    let mut changed = false;

    theme::section_title(ui, &p, "theme");
    changed |= theme::pill_group(
        ui,
        &p,
        &mut app.settings.appearance.theme,
        ThemeMode::ALL,
        |t| t.label(),
    );
    theme::note_text(
        ui,
        &p,
        "dim is a softer dark. light is there if you need it.",
    );

    theme::section_title(ui, &p, "accent");
    changed |= theme::pill_group(
        ui,
        &p,
        &mut app.settings.appearance.accent,
        Accent::ALL,
        |a| a.label(),
    );
    theme::note_text(
        ui,
        &p,
        "used for selected options, progress bars and links.",
    );

    theme::section_title(ui, &p, "interface scale");
    theme::card(ui, &p, |ui| {
        let mut scale = app.settings.appearance.ui_scale;
        if ui
            .add(
                egui::Slider::new(&mut scale, 0.8..=1.6)
                    .step_by(0.05)
                    .show_value(true)
                    .text(""),
            )
            .changed()
        {
            app.settings.appearance.ui_scale = scale;
            changed = true;
        }
    });
    theme::note_text(ui, &p, "everything scales, including the window contents.");

    changed |= theme::toggle_row(
        ui,
        &p,
        "compact queue",
        "hides the per-item action row so more downloads fit on screen.",
        &mut app.settings.appearance.compact_queue,
    );

    changed
}

fn video(app: &mut SnagApp, ui: &mut egui::Ui) -> bool {
    let p = app.palette;
    let mut changed = false;

    theme::section_title(ui, &p, "video quality");
    changed |= theme::pill_group(
        ui,
        &p,
        &mut app.settings.video.quality,
        VideoQuality::ALL,
        |q| q.label(),
    );
    theme::note_text(
        ui,
        &p,
        "if preferred video quality isn't available, next best is picked instead.",
    );

    theme::section_title(ui, &p, "preferred video codec");
    changed |= theme::pill_group(
        ui,
        &p,
        &mut app.settings.video.codec,
        VideoCodec::ALL,
        |c| c.label(),
    );
    ui.add_space(4.0);
    for c in VideoCodec::ALL {
        ui.label(
            egui::RichText::new(format!(
                "{}: {}",
                c.label().split(' ').next().unwrap_or(""),
                c.note()
            ))
            .size(12.0)
            .color(p.dim),
        );
    }
    theme::note_text(
        ui,
        &p,
        "av1 and vp9 aren't widely supported, you might need extra software to play or edit them. snag picks the next best codec if the preferred one isn't available.",
    );

    theme::section_title(ui, &p, "file container");
    changed |= theme::pill_group(
        ui,
        &p,
        &mut app.settings.video.container,
        Container::ALL,
        |c| c.label(),
    );
    theme::note_text(
        ui,
        &p,
        "when auto is selected, snag picks the best container for the codec: mp4 for h264, webm for vp9/av1.",
    );

    theme::section_title(ui, &p, "high efficiency video codec");
    changed |= theme::toggle_row(
        ui,
        &p,
        "allow h265 for videos",
        "allows downloading videos from platforms like tiktok in higher quality, at the cost of compatibility.",
        &mut app.settings.video.allow_h265,
    );

    changed |= theme::toggle_row(
        ui,
        &p,
        "prefer free formats",
        "breaks ties in favour of open containers and codecs when quality is otherwise equal.",
        &mut app.settings.video.prefer_free_formats,
    );

    theme::section_title(ui, &p, "frame rate cap");
    theme::card(ui, &p, |ui| {
        let mut fps = app.settings.video.max_fps;
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("max fps").size(13.0).color(p.dim));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                for v in [0u32, 30, 60] {
                    let label = if v == 0 {
                        "off".to_string()
                    } else {
                        v.to_string()
                    };
                    if theme::pill(ui, &p, &label, fps == v, true).clicked() {
                        fps = v;
                    }
                }
            });
        });
        if fps != app.settings.video.max_fps {
            app.settings.video.max_fps = fps;
            changed = true;
        }
    });
    theme::note_text(
        ui,
        &p,
        "caps the frame rate of the picked format. useful when 60fps files are larger than you want.",
    );

    changed
}

fn audio(app: &mut SnagApp, ui: &mut egui::Ui) -> bool {
    let p = app.palette;
    let mut changed = false;

    theme::section_title(ui, &p, "audio format");
    changed |= theme::pill_group(
        ui,
        &p,
        &mut app.settings.audio.format,
        AudioFormat::ALL,
        |f| f.label(),
    );
    theme::note_text(
        ui,
        &p,
        "all formats but best are converted from the source format, so there will be some quality loss. when best is selected, the audio is kept in its original format whenever possible.",
    );

    theme::section_title(ui, &p, "audio bitrate");
    let lossy = app.settings.audio.format.is_lossy();
    if lossy {
        changed |= theme::pill_group(
            ui,
            &p,
            &mut app.settings.audio.bitrate,
            AudioBitrate::ALL,
            |b| b.label(),
        );
    } else {
        ui.label(
            egui::RichText::new("not used by the selected format")
                .size(12.0)
                .color(p.faint),
        );
        ui.add_space(6.0);
    }
    theme::note_text(
        ui,
        &p,
        "bitrate is applied only when converting audio to a lossy format. snag can't improve the source audio quality, so choosing a bitrate over 128kbps may inflate the file size with no audible difference.",
    );

    theme::section_title(ui, &p, "audio quality");
    changed |= theme::toggle_row(
        ui,
        &p,
        "prefer better quality",
        "snag will try to pick the highest quality audio in audio mode. it may not be available depending on the site's response.",
        &mut app.settings.audio.prefer_better_quality,
    );

    changed |= theme::toggle_row(
        ui,
        &p,
        "normalize loudness",
        "runs an ffmpeg loudness pass over extracted audio so files sit at a consistent volume.",
        &mut app.settings.audio.normalize_loudness,
    );

    theme::section_title(ui, &p, "audio track");
    theme::card(ui, &p, |ui| {
        changed |= super::field_row(
            ui,
            &p,
            "preferred dub language",
            &mut app.settings.audio.dub_language,
            "original",
        );
    });
    theme::note_text(
        ui,
        &p,
        "a language code such as en, de or ja. snag uses the dubbed track when it exists, otherwise the original.",
    );

    changed
}

fn metadata(app: &mut SnagApp, ui: &mut egui::Ui) -> bool {
    let p = app.palette;
    let mut changed = false;
    let m = &mut app.settings.metadata;

    theme::section_title(ui, &p, "embedded in the file");
    changed |= theme::toggle_row(
        ui,
        &p,
        "embed metadata",
        "writes the title, uploader and description into the file's own tags.",
        &mut m.embed_metadata,
    );
    changed |= theme::toggle_row(
        ui,
        &p,
        "embed thumbnail",
        "uses the video thumbnail as cover art. this is what music players show.",
        &mut m.embed_thumbnail,
    );
    changed |= theme::toggle_row(
        ui,
        &p,
        "embed chapters",
        "keeps chapter markers so players can jump between sections.",
        &mut m.embed_chapters,
    );
    changed |= theme::toggle_row(
        ui,
        &p,
        "embed subtitles",
        "adds subtitle tracks to video downloads. ignored in audio mode.",
        &mut m.embed_subtitles,
    );

    theme::section_title(ui, &p, "extra files");
    changed |= theme::toggle_row(
        ui,
        &p,
        "save thumbnail separately",
        "writes the thumbnail next to the media file as its own image.",
        &mut m.write_thumbnail_file,
    );
    changed |= theme::toggle_row(
        ui,
        &p,
        "save subtitles as files",
        "writes subtitles next to the media file as .srt, which every player takes. works in audio mode too.",
        &mut m.write_subtitle_files,
    );
    changed |= theme::toggle_row(
        ui,
        &p,
        "split into chapters",
        "writes one file per chapter as well as the whole thing. videos without chapters are downloaded as usual. needs ffmpeg.",
        &mut m.split_chapters,
    );

    if m.embed_subtitles || m.write_subtitle_files {
        theme::section_title(ui, &p, "subtitles");
        changed |= theme::toggle_row(
            ui,
            &p,
            "include automatic captions",
            "counts a site's machine transcript as a subtitle. most videos have no hand written ones, so with this off you will usually get nothing.",
            &mut m.include_auto_subs,
        );
        theme::card(ui, &p, |ui| {
            changed |= super::field_row(
                ui,
                &p,
                "subtitle languages",
                &mut m.subtitle_languages,
                "en,en-orig",
            );
        });
        theme::note_text(
            ui,
            &p,
            "comma separated. use all for every available track.",
        );
    }

    theme::section_title(ui, &p, "sponsorblock");
    changed |= theme::toggle_row(
        ui,
        &p,
        "remove sponsor segments",
        "cuts sponsor, intro and self-promo segments out using community submitted timestamps.",
        &mut m.sponsorblock_remove,
    );

    theme::section_title(ui, &p, "timestamps");
    changed |= theme::toggle_row(
        ui,
        &p,
        "keep the original upload date",
        "sets the file's modified time to the upload date instead of the download time.",
        &mut m.keep_original_date,
    );

    changed
}

fn processing(app: &mut SnagApp, ui: &mut egui::Ui) -> bool {
    let p = app.palette;
    let mut changed = false;

    theme::section_title(ui, &p, "download folder");
    theme::card(ui, &p, |ui| {
        ui.horizontal(|ui| {
            let dir = app.settings.processing.download_dir.display().to_string();
            let short: String = if dir.len() > 48 {
                format!("...{}", &dir[dir.len() - 45..])
            } else {
                dir.clone()
            };
            ui.label(egui::RichText::new(short).size(13.0).color(p.text));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if theme::pill(ui, &p, "change", false, true).clicked() {
                    let start = app.settings.processing.download_dir.clone();
                    let mut dialog = rfd::FileDialog::new();
                    if start.exists() {
                        dialog = dialog.set_directory(&start);
                    }
                    if let Some(dir) = dialog.pick_folder() {
                        app.settings.processing.download_dir = dir;
                        changed = true;
                    }
                }
                if theme::pill(ui, &p, "open", false, true).clicked() {
                    util::reveal(&app.settings.processing.download_dir);
                }
            });
        });
    });
    theme::note_text(ui, &p, "every finished file lands here.");

    theme::section_title(ui, &p, "file names");
    theme::card(ui, &p, |ui| {
        changed |= super::field_row(
            ui,
            &p,
            "output template",
            &mut app.settings.processing.output_template,
            "%(title)s.%(ext)s",
        );
        ui.add_space(6.0);
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            for preset in [
                "%(title)s.%(ext)s",
                "%(uploader)s - %(title)s.%(ext)s",
                "%(upload_date)s - %(title)s.%(ext)s",
                "%(playlist_index)s - %(title)s.%(ext)s",
            ] {
                let selected = app.settings.processing.output_template == preset;
                let label = preset.replace("%(", "").replace(")s", "");
                if theme::pill(ui, &p, &label, selected, true).clicked() {
                    app.settings.processing.output_template = preset.to_string();
                    changed = true;
                }
            }
        });
    });
    theme::note_text(
        ui,
        &p,
        "yt-dlp output template fields. the presets cover most cases.",
    );

    changed |= theme::toggle_row(
        ui,
        &p,
        "restrict file names",
        "strips spaces and non-ascii characters, which keeps names portable across systems.",
        &mut app.settings.processing.restrict_filenames,
    );
    changed |= theme::toggle_row(
        ui,
        &p,
        "overwrite existing files",
        "off means a file that already exists is left alone and the download is skipped.",
        &mut app.settings.processing.overwrite_existing,
    );
    changed |= theme::toggle_row(
        ui,
        &p,
        "keep the source file after remuxing",
        "off deletes the input once a remux finishes successfully.",
        &mut app.settings.processing.keep_source_after_remux,
    );

    theme::section_title(ui, &p, "throughput");
    theme::card(ui, &p, |ui| {
        let mut jobs = app.settings.processing.max_concurrent_jobs as u32;
        if super::number_row(ui, &p, "downloads at once", &mut jobs, 1..=8) {
            app.settings.processing.max_concurrent_jobs = jobs as usize;
            changed = true;
        }
        ui.add_space(6.0);
        let mut frags = app.settings.processing.concurrent_fragments;
        if super::number_row(ui, &p, "fragments per download", &mut frags, 1..=16) {
            app.settings.processing.concurrent_fragments = frags;
            changed = true;
        }
    });
    theme::note_text(
        ui,
        &p,
        "more fragments at once is faster on fast connections and rougher on slow ones.",
    );

    changed
}

fn background(app: &mut SnagApp, ui: &mut egui::Ui) -> bool {
    let p = app.palette;
    let mut changed = false;

    theme::section_title(ui, &p, "keep running when closed");
    if crate::tray::supported() {
        changed |= theme::toggle_row(
            ui,
            &p,
            "run in the background",
            "closing the window puts snag in the system tray instead of quitting it. click the tray icon to bring it back, or use quit there to close it properly.",
            &mut app.settings.background.run_in_background,
        );
    } else {
        theme::card(ui, &p, |ui| {
            ui.label(
                egui::RichText::new("not available on this platform")
                    .size(13.0)
                    .color(p.dim),
            );
        });
        theme::note_text(
            ui,
            &p,
            "a tray icon on linux would need libayatana-appindicator at build time, which would add a system dependency to the appimage for an optional feature.",
        );
    }

    theme::section_title(ui, &p, "clipboard");
    changed |= theme::toggle_row(
        ui,
        &p,
        "watch the clipboard for links",
        "when you copy a link, snag offers it rather than downloading it. nothing is read but the clipboard text, nothing is stored, and nothing is sent anywhere.",
        &mut app.settings.background.watch_clipboard,
    );

    if app.settings.background.watch_clipboard {
        changed |= theme::toggle_row(
            ui,
            &p,
            "bring snag back when a link is copied",
            "off by default, because hiding snag is a request to be left alone. worth turning on if notifications do not reach you.",
            &mut app.settings.background.show_on_copied_link,
        );

        theme::note_text(
            ui,
            &p,
            "while snag is hidden it also tries a desktop notification. that works on linux and macos, but windows silently drops notifications from apps that were not installed from the store, so do not rely on it there.",
        );
    }

    changed
}

fn network(app: &mut SnagApp, ui: &mut egui::Ui) -> bool {
    let p = app.palette;
    let mut changed = false;

    theme::section_title(ui, &p, "connection");
    theme::card(ui, &p, |ui| {
        changed |= super::field_row(
            ui,
            &p,
            "proxy",
            &mut app.settings.network.proxy,
            "http://host:port",
        );
        ui.add_space(6.0);
        changed |= super::field_row(
            ui,
            &p,
            "speed limit",
            &mut app.settings.network.rate_limit,
            "e.g. 2M",
        );
        ui.add_space(6.0);
        let mut retries = app.settings.network.retries;
        if super::number_row(ui, &p, "retries", &mut retries, 0..=50) {
            app.settings.network.retries = retries;
            changed = true;
        }
        ui.add_space(6.0);
        let mut timeout = app.settings.network.socket_timeout;
        if super::number_row(ui, &p, "socket timeout (s)", &mut timeout, 5..=120) {
            app.settings.network.socket_timeout = timeout;
            changed = true;
        }
    });
    theme::note_text(ui, &p, "leave the text fields empty to use the defaults.");

    theme::section_title(ui, &p, "identity");
    theme::card(ui, &p, |ui| {
        changed |= super::field_row(
            ui,
            &p,
            "user agent",
            &mut app.settings.network.user_agent,
            "default",
        );
    });
    theme::note_text(
        ui,
        &p,
        "signing in lives on its own screen now: settings > youtube.",
    );

    changed
}

/// Signing in to YouTube.
///
/// This is deliberately its own screen rather than a line in the network tab:
/// it is the one setting people come looking for by name, and it is worth the
/// room to say plainly what it does, what it does not do, and why there is no
/// password box.
fn youtube(app: &mut SnagApp, ui: &mut egui::Ui) -> bool {
    let p = app.palette;
    let mut changed = false;

    theme::section_title(ui, &p, "sign in to youtube");
    theme::note_text(
        ui,
        &p,
        "for age restricted, private and members-only videos. this screen is youtube only: nothing here changes how any other site is downloaded.",
    );

    theme::card(ui, &p, |ui| {
        ui.label(
            egui::RichText::new("snag never asks for your password")
                .size(13.0)
                .color(p.text),
        );
        ui.add_space(4.0);
        ui.label(
            egui::RichText::new(
                "sign in to youtube in your browser as you normally would, then pick that browser below. snag borrows the session from it, so your password is never typed into snag and never stored by it.",
            )
            .size(12.0)
            .color(p.dim),
        );
    });

    // Said before the browser is picked, because that is the moment the
    // decision is made. Every tool in this space works this way, but a user
    // should hear what it costs from us rather than from a locked account.
    theme::section_title(ui, &p, "before you sign in");
    theme::card(ui, &p, |ui| {
        ui.label(
            egui::RichText::new(
                "downloading from youtube is against youtube's terms, signed in or not.",
            )
            .size(13.0)
            .color(p.warn),
        );
        ui.add_space(6.0);
        for line in [
            "signing in ties that activity to your account, which downloading without it does not. youtube can answer with bot checks, with throttling, and in rare cases by closing the account. use a second google account for this rather than the one your email and everything else sits on.",
            "a cookie file is as good as your password. anyone who has a copy is signed in as you, without needing your login or a two-factor code. keep it out of shared folders and out of repositories.",
        ] {
            ui.label(egui::RichText::new(line).size(12.0).color(p.dim));
            ui.add_space(6.0);
        }
    });

    theme::section_title(ui, &p, "browser to borrow the session from");
    let before = app.settings.network.cookies_from_browser;
    changed |= theme::pill_group(
        ui,
        &p,
        &mut app.settings.network.cookies_from_browser,
        CookieBrowser::ALL,
        |b| b.label(),
    );
    // A status from the previous browser would be a lie about this one.
    if app.settings.network.cookies_from_browser != before {
        app.signin = SignInState::Unknown;
    }
    theme::note_text(
        ui,
        &p,
        "close the browser first: it holds a lock on its own cookie store while it is running.",
    );

    if crate::youtube::sealed_on_windows(app.settings.network.cookies_from_browser) {
        theme::card(ui, &p, |ui| {
            ui.label(
                egui::RichText::new(
                    "windows: chromium browsers now encrypt their cookies so that only the browser itself can read them, and no external tool can undo that. firefox is the one that reliably works here. the alternative is a cookie file, below.",
                )
                .size(12.0)
                .color(p.warn),
            );
        });
    }

    ui.add_space(10.0);
    ui.horizontal(|ui| {
        let ready = crate::youtube::configured(&app.settings) && !app.signin.busy();
        if theme::action_button(ui, &p, "check sign-in", true, ready).clicked() {
            app.check_signin(ui.ctx());
        }
        ui.add_space(10.0);
        let (text, color) = match &app.signin {
            SignInState::Unknown => ("not checked yet".to_string(), p.dim),
            SignInState::Checking => ("asking youtube...".to_string(), p.dim),
            SignInState::SignedIn => ("signed in".to_string(), p.good),
            SignInState::SignedOut => (
                "cookies read, but not signed in to youtube".to_string(),
                p.warn,
            ),
            SignInState::Unreadable(_) => ("could not read the cookies".to_string(), p.bad),
            SignInState::Error(_) => ("the check failed".to_string(), p.bad),
        };
        ui.label(egui::RichText::new(text).size(12.0).color(color));
    });

    // The advice differs per outcome, and the raw message is worth keeping
    // for anything that has to be searched for.
    match &app.signin {
        SignInState::SignedOut => theme::note_text(
            ui,
            &p,
            "the browser profile was read, but there is no youtube session in it. sign in to youtube in that browser, close it, and check again.",
        ),
        SignInState::Unreadable(raw) | SignInState::Error(raw) => {
            theme::note_text(ui, &p, "close the browser and try again. if it keeps failing, use a cookie file instead.");
            theme::card(ui, &p, |ui| {
                ui.label(
                    egui::RichText::new(raw.chars().take(400).collect::<String>())
                        .size(11.0)
                        .color(p.faint),
                );
            });
        }
        SignInState::SignedIn => theme::note_text(
            ui,
            &p,
            "restricted videos you have access to will download from now on.",
        ),
        _ => {}
    }

    theme::section_title(ui, &p, "cookie file");
    theme::card(ui, &p, |ui| {
        changed |= super::field_row(
            ui,
            &p,
            "cookie file",
            &mut app.settings.network.cookie_file,
            "path to cookies.txt",
        );
    });
    theme::note_text(
        ui,
        &p,
        "an exported cookies.txt, for when snag cannot read the browser directly. it takes priority over the browser above. treat the file like a password: anyone who has it is signed in as you.",
    );

    theme::section_title(ui, &p, "scope");
    changed |= theme::toggle_row(
        ui,
        &p,
        "only use the sign-in for youtube links",
        "on by default. turned off, the same cookies are offered to every site you download from, which is rarely what you want.",
        &mut app.settings.network.cookies_youtube_only,
    );

    if !app.settings.network.cookies_youtube_only {
        theme::note_text(
            ui,
            &p,
            "your youtube session is now sent to every site snag downloads from.",
        );
    }

    changed
}

fn advanced(app: &mut SnagApp, ui: &mut egui::Ui) -> bool {
    let p = app.palette;
    let mut changed = false;

    theme::section_title(ui, &p, "binaries");
    theme::card(ui, &p, |ui| {
        ui.horizontal(|ui| {
            changed |= super::field_row(
                ui,
                &p,
                "yt-dlp path",
                &mut app.settings.advanced.ytdlp_path,
                "found on PATH",
            );
        });
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            changed |= super::field_row(
                ui,
                &p,
                "ffmpeg path",
                &mut app.settings.advanced.ffmpeg_path,
                "found on PATH",
            );
        });
        ui.add_space(10.0);
        ui.horizontal(|ui| {
            if theme::pill(ui, &p, "browse for yt-dlp", false, true).clicked() {
                if let Some(f) = rfd::FileDialog::new().pick_file() {
                    app.settings.advanced.ytdlp_path = f.display().to_string();
                    changed = true;
                }
            }
            if theme::pill(ui, &p, "browse for ffmpeg", false, true).clicked() {
                if let Some(f) = rfd::FileDialog::new().pick_file() {
                    app.settings.advanced.ffmpeg_path = f.display().to_string();
                    changed = true;
                }
            }
        });
    });
    theme::note_text(
        ui,
        &p,
        "leave empty to use whatever is on PATH. snag never downloads binaries on its own.",
    );

    theme::section_title(ui, &p, "behaviour");
    changed |= theme::toggle_row(
        ui,
        &p,
        "ignore playlists",
        "on means a link that points into a playlist downloads only that one item.",
        &mut app.settings.advanced.ignore_playlists,
    );
    changed |= theme::toggle_row(
        ui,
        &p,
        "verbose log",
        "keeps yt-dlp's full output in each job's log. useful when something misbehaves.",
        &mut app.settings.advanced.verbose_log,
    );

    theme::section_title(ui, &p, "extra yt-dlp arguments");
    theme::card(ui, &p, |ui| {
        let mut extra = app.settings.advanced.extra_args.clone();
        let r = ui.add(
            egui::TextEdit::multiline(&mut extra)
                .desired_rows(2)
                .desired_width(f32::INFINITY)
                .font(egui::FontId::new(13.0, egui::FontFamily::Monospace))
                .hint_text(egui::RichText::new("--no-part --write-description").color(p.faint)),
        );
        if r.changed() {
            app.settings.advanced.extra_args = extra;
            changed = true;
        }
    });
    theme::note_text(
        ui,
        &p,
        "appended to every download, after everything snag builds. quotes are respected.",
    );

    theme::section_title(ui, &p, "command preview");
    let preview = ytdlp::preview_command("<link>", Mode::Auto, &app.settings);
    theme::card(ui, &p, |ui| {
        ui.label(egui::RichText::new(&preview).size(11.0).color(p.dim));
        ui.add_space(8.0);
        if theme::pill(ui, &p, "copy", false, true).clicked() {
            util::set_clipboard_text(&preview);
            app.toast("command copied", false);
        }
    });
    theme::note_text(
        ui,
        &p,
        "exactly what snag runs in auto mode with the current settings.",
    );

    theme::section_title(ui, &p, "reset");
    if theme::action_button(ui, &p, "reset all settings", false, true).clicked() {
        let dir = app.settings.processing.download_dir.clone();
        app.settings = Settings::default();
        app.settings.processing.download_dir = dir;
        app.toast("settings reset to defaults", false);
        changed = true;
    }

    changed
}
