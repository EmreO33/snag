#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod bootstrap;
mod clipboard;
mod history;
mod icon;
mod installer;
mod jobs;
mod probe;
mod remux;
mod selfupdate;
mod settings;
mod theme;
mod tray;
mod ui;
mod updater;
mod util;
mod window;
mod ytdlp;

use eframe::egui;

/// `snag --print-command <link>` prints the yt-dlp invocation the current
/// settings would produce, one argument per line, and exits. Useful for
/// checking what Snag is actually doing without starting a download.
fn print_command_and_exit() -> bool {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(pos) = args.iter().position(|a| a == "--print-command") else {
        return false;
    };
    let url = args.get(pos + 1).map(String::as_str).unwrap_or("<link>");
    let (settings, _) = settings::Settings::load();
    let mode = match args.iter().find_map(|a| a.strip_prefix("--mode=")) {
        Some("audio") => settings::Mode::Audio,
        Some("mute") => settings::Mode::Mute,
        _ => settings::Mode::Auto,
    };
    println!("{}", settings.ytdlp_bin());
    for arg in ytdlp::build_args(url, mode, jobs::JobOverrides::default(), &settings) {
        println!("{arg}");
    }
    true
}

/// `snag --install-ytdlp [dir]` downloads the current yt-dlp release and exits,
/// which is the setup screen's install step without the window.
fn install_ytdlp_and_exit() -> bool {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(pos) = args.iter().position(|a| a == "--install-ytdlp") else {
        return false;
    };

    let dir = args
        .get(pos + 1)
        .filter(|a| !a.starts_with("--"))
        .map(std::path::PathBuf::from)
        .unwrap_or_else(bootstrap::managed_bin_dir);

    println!("installing into {}", dir.display());
    let (tx, rx) = std::sync::mpsc::channel();
    installer::install(dir, tx, || {});

    let mut last_pct = u64::MAX;
    while let Ok(ev) = rx.recv() {
        match ev {
            installer::InstallEvent::Log(l) => println!("{l}"),
            installer::InstallEvent::State(st) => match st {
                installer::InstallState::Downloading { got, total } if total > 0 => {
                    let pct = got * 100 / total;
                    if pct != last_pct {
                        last_pct = pct;
                        println!("{pct}% ({got} / {total} bytes)");
                    }
                }
                installer::InstallState::Verifying => println!("verifying"),
                installer::InstallState::Done { path, version } => {
                    println!("installed yt-dlp {version} at {}", path.display());
                    break;
                }
                installer::InstallState::Failed(e) => {
                    eprintln!("failed: {e}");
                    break;
                }
                _ => {}
            },
        }
    }
    true
}

/// `snag --notify-test` sends one desktop notification and reports whether the
/// desktop accepted it. Notifications are the one feature that can fail
/// silently on someone else's machine, so there is a way to check.
fn notify_test_and_exit() -> bool {
    if !std::env::args().any(|a| a == "--notify-test") {
        return false;
    }
    let ok = clipboard::notify_found("https://example.com/a-video");
    println!(
        "notification accepted by the desktop: {ok}{}",
        if ok {
            ""
        } else {
            " (nothing was shown; on windows an unpackaged app needs a registered app id)"
        }
    );
    true
}

/// A link passed on the command line, so `snag <url>` fills the box ready to
/// go. This is also what a future "open with Snag" or notification click would
/// use.
fn startup_url() -> Option<String> {
    std::env::args().skip(1).find(|a| util::looks_like_url(a))
}

/// `snag --view=<name>` opens straight to a screen instead of the link box.
fn startup_view() -> Option<app::View> {
    std::env::args()
        .find_map(|a| a.strip_prefix("--view=").map(str::to_string))
        .and_then(|v| match v.as_str() {
            "home" | "save" => Some(app::View::Home),
            "queue" => Some(app::View::Queue),
            "remux" => Some(app::View::Remux),
            "history" => Some(app::View::History),
            "settings" => Some(app::View::Settings),
            "updates" => Some(app::View::Updates),
            "about" => Some(app::View::About),
            _ => None,
        })
}

fn main() -> eframe::Result<()> {
    if print_command_and_exit() || install_ytdlp_and_exit() || notify_test_and_exit() {
        return Ok(());
    }

    // A previous self-update leaves the old binary beside us; it can only be
    // deleted once it is no longer the running process.
    selfupdate::clean_stale_binary();

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Snag")
            .with_inner_size([980.0, 660.0])
            .with_min_inner_size([720.0, 480.0])
            .with_app_id("snag")
            .with_icon(icon::icon_data().unwrap_or_default()),
        vsync: true,
        centered: true,
        ..Default::default()
    };

    eframe::run_native(
        "Snag",
        options,
        Box::new(move |cc| {
            let mut app = app::SnagApp::new(cc);
            if let Some(url) = startup_url() {
                app.url_input = url;
            }
            if let Some(v) = startup_view() {
                app.view = v;
            }
            Ok(Box::new(app))
        }),
    )
}
