//! Everything that translates Snag's settings into a yt-dlp invocation, plus
//! the parser for the machine-readable progress lines it prints back.

use crate::settings::{AudioFormat, Container, Mode, Settings};

/// Field separator for our custom progress template. Chosen because it is
/// vanishingly rare in titles, and the title is emitted last regardless.
pub const SEP: char = '\u{1F}';
pub const TAG_PROGRESS: &str = "@SNAG";
pub const TAG_POST: &str = "@SNAGPP";
pub const TAG_FILE: &str = "@SNAGFILE";

/// One parsed progress line from yt-dlp.
#[derive(Debug, Clone, Default)]
pub struct Progress {
    pub status: String,
    pub downloaded: Option<f64>,
    pub total: Option<f64>,
    pub speed: Option<f64>,
    pub eta: Option<f64>,
    pub title: Option<String>,
}

#[derive(Debug, Clone)]
pub enum Line {
    Progress(Progress),
    PostProcess { status: String },
    File(String),
    Other(String),
}

fn num(field: &str) -> Option<f64> {
    let f = field.trim();
    if f.is_empty() || f == "NA" || f == "None" {
        return None;
    }
    f.parse::<f64>().ok().filter(|v| v.is_finite())
}

/// Parse one stdout line from yt-dlp into something the UI can use.
pub fn parse_line(line: &str) -> Line {
    let line = line.trim_end_matches(['\r', '\n']);

    if let Some(rest) = line.strip_prefix(TAG_FILE) {
        return Line::File(rest.trim_start_matches(SEP).trim().to_string());
    }

    if let Some(rest) = line.strip_prefix(TAG_POST) {
        let mut it = rest.split(SEP).skip(1);
        return Line::PostProcess {
            status: it.next().unwrap_or("processing").trim().to_string(),
        };
    }

    if let Some(rest) = line.strip_prefix(TAG_PROGRESS) {
        // status, downloaded, total, total_estimate, speed, eta, title
        let mut parts = rest.split(SEP);
        parts.next(); // leading empty field before the first separator
        let status = parts.next().unwrap_or("").trim().to_string();
        let downloaded = parts.next().and_then(num);
        let total = parts.next().and_then(num);
        let estimate = parts.next().and_then(num);
        let speed = parts.next().and_then(num);
        let eta = parts.next().and_then(num);
        // The title is last so separators inside it cannot break the parse.
        let title = parts
            .collect::<Vec<_>>()
            .join(&SEP.to_string())
            .trim()
            .to_string();

        return Line::Progress(Progress {
            status,
            downloaded,
            total: total.or(estimate),
            speed,
            eta,
            title: if title.is_empty() || title == "NA" {
                None
            } else {
                Some(title)
            },
        });
    }

    Line::Other(line.to_string())
}

/// Split a user-supplied argument string the way a shell would, honouring quotes.
pub fn split_args(raw: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quote: Option<char> = None;
    let mut has_token = false;

    for ch in raw.chars() {
        match quote {
            Some(q) if ch == q => quote = None,
            Some(_) => cur.push(ch),
            None if ch == '"' || ch == '\'' => {
                quote = Some(ch);
                has_token = true;
            }
            None if ch.is_whitespace() => {
                if has_token {
                    out.push(std::mem::take(&mut cur));
                    has_token = false;
                }
            }
            None => {
                cur.push(ch);
                has_token = true;
            }
        }
    }
    if has_token {
        out.push(cur);
    }
    out
}

/// Build the `-f` format selector for a mode.
fn format_selector(mode: Mode, s: &Settings) -> String {
    let mut filters = String::new();
    if let Some(h) = s.video.quality.height() {
        filters.push_str(&format!("[height<={h}]"));
    }
    if s.video.max_fps > 0 {
        filters.push_str(&format!("[fps<={}]", s.video.max_fps));
    }
    if !s.video.allow_h265 {
        // Exclude every spelling of HEVC that shows up in format listings.
        filters.push_str("[vcodec!*=hev][vcodec!*=hvc][vcodec!*=h265]");
    }

    match mode {
        Mode::Audio => "bestaudio/best".to_string(),
        Mode::Mute => format!("bv*{filters}/bv*/b{filters}/b"),
        Mode::Auto => format!("bv*{filters}+ba/b{filters}/bv*+ba/b"),
    }
}

/// Build the `-S` format sort, which is what actually steers codec preference.
fn format_sort(mode: Mode, s: &Settings) -> Option<String> {
    let mut tokens: Vec<String> = Vec::new();

    match mode {
        Mode::Audio => {
            if s.audio.prefer_better_quality {
                tokens.push("quality".into());
                tokens.push("abr".into());
            }
        }
        Mode::Auto | Mode::Mute => {
            if let Some(h) = s.video.quality.height() {
                tokens.push(format!("res:{h}"));
            }
            tokens.push(s.video.codec.sort_token().to_string());
            if mode == Mode::Auto {
                tokens.push(s.video.codec.audio_sort_token().to_string());
            }
            if s.video.prefer_free_formats {
                tokens.push("ext".into());
            }
        }
    }

    if tokens.is_empty() {
        None
    } else {
        Some(tokens.join(","))
    }
}

/// Full argument vector for downloading `url` in `mode`.
pub fn build_args(url: &str, mode: Mode, s: &Settings) -> Vec<String> {
    let mut a: Vec<String> = Vec::new();
    let push = |a: &mut Vec<String>, v: &str| a.push(v.to_string());

    // --- machine-readable output -------------------------------------------
    push(&mut a, "--newline");
    push(&mut a, "--no-colors");
    push(&mut a, "--progress");
    push(&mut a, "--no-simulate");
    push(&mut a, "--progress-template");
    a.push(format!(
        "download:{TAG_PROGRESS}{SEP}%(progress.status)s{SEP}%(progress.downloaded_bytes)s{SEP}%(progress.total_bytes)s{SEP}%(progress.total_bytes_estimate)s{SEP}%(progress.speed)s{SEP}%(progress.eta)s{SEP}%(info.title)s"
    ));
    push(&mut a, "--progress-template");
    a.push(format!("postprocess:{TAG_POST}{SEP}%(progress.status)s"));
    push(&mut a, "--print");
    a.push(format!("after_move:{TAG_FILE}{SEP}%(filepath)s"));

    // --- output location ----------------------------------------------------
    push(&mut a, "-P");
    a.push(s.processing.download_dir.display().to_string());
    push(&mut a, "-o");
    a.push(s.processing.output_template.clone());
    if s.processing.restrict_filenames {
        push(&mut a, "--restrict-filenames");
    }
    if s.processing.overwrite_existing {
        push(&mut a, "--force-overwrites");
    } else {
        push(&mut a, "--no-overwrites");
    }
    if s.advanced.ignore_playlists {
        push(&mut a, "--no-playlist");
    } else {
        push(&mut a, "--yes-playlist");
    }

    // --- format selection ---------------------------------------------------
    push(&mut a, "-f");
    a.push(format_selector(mode, s));
    if let Some(sort) = format_sort(mode, s) {
        push(&mut a, "-S");
        a.push(sort);
    }

    match mode {
        Mode::Audio => {
            push(&mut a, "-x");
            push(&mut a, "--audio-format");
            a.push(s.audio.format.ytdlp_value().to_string());
            if s.audio.format.is_lossy() {
                push(&mut a, "--audio-quality");
                a.push(format!("{}K", s.audio.bitrate.kbps()));
            } else if s.audio.format == AudioFormat::Best {
                push(&mut a, "--audio-quality");
                push(&mut a, "0");
            }
        }
        Mode::Auto => {
            push(&mut a, "--merge-output-format");
            a.push(s.video.container.resolve(s.video.codec).to_string());
            if s.video.container != Container::Auto {
                push(&mut a, "--remux-video");
                a.push(s.video.container.resolve(s.video.codec).to_string());
            }
        }
        Mode::Mute => {
            push(&mut a, "--remux-video");
            a.push(s.video.container.resolve(s.video.codec).to_string());
        }
    }

    if s.audio.normalize_loudness && mode == Mode::Audio {
        push(&mut a, "--postprocessor-args");
        push(&mut a, "ffmpeg:-af loudnorm=I=-16:TP=-1.5:LRA=11");
    }

    // Dubbed audio track selection (YouTube).
    let lang = s.audio.dub_language.trim();
    if !lang.is_empty() && lang != "original" {
        push(&mut a, "--extractor-args");
        a.push(format!("youtube:lang={lang}"));
    }

    // --- metadata -----------------------------------------------------------
    let m = &s.metadata;
    if m.embed_metadata {
        push(&mut a, "--embed-metadata");
    }
    if m.embed_thumbnail {
        push(&mut a, "--embed-thumbnail");
    }
    if m.embed_chapters {
        push(&mut a, "--embed-chapters");
    }
    if m.embed_subtitles && mode != Mode::Audio {
        push(&mut a, "--embed-subs");
        push(&mut a, "--sub-langs");
        a.push(if m.subtitle_languages.trim().is_empty() {
            "en".into()
        } else {
            m.subtitle_languages.trim().to_string()
        });
    }
    if m.write_thumbnail_file {
        push(&mut a, "--write-thumbnail");
    }
    if m.sponsorblock_remove {
        push(&mut a, "--sponsorblock-remove");
        push(&mut a, "default");
    }
    if !m.keep_original_date {
        push(&mut a, "--no-mtime");
    }

    // --- network ------------------------------------------------------------
    let n = &s.network;
    if !n.proxy.trim().is_empty() {
        push(&mut a, "--proxy");
        a.push(n.proxy.trim().to_string());
    }
    if !n.rate_limit.trim().is_empty() {
        push(&mut a, "-r");
        a.push(n.rate_limit.trim().to_string());
    }
    push(&mut a, "-R");
    a.push(n.retries.to_string());
    push(&mut a, "--socket-timeout");
    a.push(n.socket_timeout.to_string());
    if n.cookies_from_browser != crate::settings::CookieBrowser::None {
        push(&mut a, "--cookies-from-browser");
        a.push(n.cookies_from_browser.label().to_string());
    }
    if !n.cookie_file.trim().is_empty() {
        push(&mut a, "--cookies");
        a.push(n.cookie_file.trim().to_string());
    }
    if !n.user_agent.trim().is_empty() {
        push(&mut a, "--user-agent");
        a.push(n.user_agent.trim().to_string());
    }

    // --- processing ---------------------------------------------------------
    if s.processing.concurrent_fragments > 1 {
        push(&mut a, "-N");
        a.push(s.processing.concurrent_fragments.to_string());
    }
    let ffmpeg = s.advanced.ffmpeg_path.trim();
    if !ffmpeg.is_empty() {
        push(&mut a, "--ffmpeg-location");
        a.push(ffmpeg.to_string());
    }
    if s.advanced.verbose_log {
        push(&mut a, "--verbose");
    }

    a.extend(split_args(&s.advanced.extra_args));
    a.push(url.to_string());
    a
}

/// A readable one-line preview of the command, for the settings screen.
pub fn preview_command(url: &str, mode: Mode, s: &Settings) -> String {
    let args = build_args(url, mode, s);
    let quoted: Vec<String> = args
        .iter()
        .map(|a| {
            let shown = a.replace(SEP, "|");
            if shown.contains(' ') {
                format!("\"{shown}\"")
            } else {
                shown
            }
        })
        .collect();
    format!("{} {}", s.ytdlp_bin(), quoted.join(" "))
}
