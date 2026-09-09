//! Everything that translates Snag's settings into a yt-dlp invocation, plus
//! the parser for the machine-readable progress lines it prints back.

use crate::jobs::JobOverrides;
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
fn format_selector(mode: Mode, overrides: JobOverrides, s: &Settings) -> String {
    let mut filters = String::new();
    if let Some(h) = overrides.height.or_else(|| s.video.quality.height()) {
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
fn format_sort(mode: Mode, overrides: JobOverrides, s: &Settings) -> Option<String> {
    let mut tokens: Vec<String> = Vec::new();

    match mode {
        Mode::Audio => {
            if s.audio.prefer_better_quality {
                tokens.push("quality".into());
                tokens.push("abr".into());
            }
        }
        Mode::Auto | Mode::Mute => {
            if let Some(h) = overrides.height.or_else(|| s.video.quality.height()) {
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
pub fn build_args(url: &str, mode: Mode, overrides: JobOverrides, s: &Settings) -> Vec<String> {
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
    // An explicit choice for this download beats the global preference.
    if overrides.whole_playlist || !s.advanced.ignore_playlists {
        push(&mut a, "--yes-playlist");
    } else {
        push(&mut a, "--no-playlist");
    }

    // --- format selection ---------------------------------------------------
    push(&mut a, "-f");
    a.push(format_selector(mode, overrides, s));
    if let Some(sort) = format_sort(mode, overrides, s) {
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
    // Subtitles can be wanted in the file, beside it, or both, and the
    // language list and the automatic-caption switch apply to whichever.
    let embed_subs = m.embed_subtitles && mode != Mode::Audio;
    if embed_subs {
        push(&mut a, "--embed-subs");
    }
    if m.write_subtitle_files {
        push(&mut a, "--write-subs");
        // Sites hand these over as vtt; srt is the one every player takes.
        push(&mut a, "--convert-subs");
        push(&mut a, "srt");
    }
    if embed_subs || m.write_subtitle_files {
        if m.include_auto_subs {
            push(&mut a, "--write-auto-subs");
        }
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
    if m.split_chapters {
        push(&mut a, "--split-chapters");
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
    a.extend(crate::youtube::cookie_args(s, url));
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
    let args = build_args(url, mode, JobOverrides::default(), s);
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

/// Turn a raw yt-dlp failure into something a person can act on.
///
/// yt-dlp's errors are written for someone reading a terminal, and the useful
/// part is usually buried behind a stack of prefixes. Each case below has a
/// remedy the user can actually carry out, so say that instead.
pub fn explain_error(raw: &str) -> String {
    let lower = raw.to_lowercase();

    let hint = if lower.contains("sign in to confirm your age")
        || lower.contains("age-restricted")
        || lower.contains("inappropriate for some users")
    {
        Some(
            "this video is age restricted. sign in to youtube from settings > youtube to reach it.",
        )
    } else if lower.contains("private video")
        || lower.contains("members-only")
        || lower.contains("join this channel")
    {
        Some("this video is private or members-only. sign in to youtube from settings > youtube, with an account that has access.")
    } else if lower.contains("video unavailable") || lower.contains("removed by the uploader") {
        Some("the site says this video is gone.")
    } else if lower.contains("not available in your country")
        || lower.contains("geo restricted")
        || lower.contains("geo-restricted")
    {
        Some("this video is blocked in your region. a proxy set in settings > network may get around it.")
    } else if lower.contains("sign in to confirm you're not a bot")
        || lower.contains("confirm you are not a bot")
    {
        Some("the site wants to check you are not a bot. signing in from settings > youtube is what gets past this.")
    } else if lower.contains("http error 429") || lower.contains("too many requests") {
        Some("the site is rate limiting you. wait a while, or set a speed limit in settings > network.")
    } else if lower.contains("unsupported url")
        || lower.contains("unable to extract") && lower.contains("extractor")
    {
        Some("yt-dlp does not recognise this link. check the updates screen: a newer yt-dlp often fixes this.")
    } else if lower.contains("ffmpeg")
        && (lower.contains("not found") || lower.contains("not installed"))
    {
        Some("this needs ffmpeg, which was not found. install it from the setup screen or settings > advanced.")
    } else if lower.contains("no space left") {
        Some("the disk is full.")
    } else if lower.contains("name or service not known")
        || lower.contains("temporary failure in name resolution")
        || lower.contains("failed to resolve")
    {
        Some("could not reach the site. check your connection.")
    } else {
        None
    };

    // Strip yt-dlp's prefixes so the underlying message reads cleanly.
    let cleaned = raw
        .trim()
        .trim_start_matches("ERROR:")
        .trim()
        .trim_start_matches("[youtube]")
        .trim();

    match hint {
        Some(h) => format!("{h}\n\n{cleaned}"),
        None => cleaned.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::explain_error;

    #[test]
    fn age_restriction_gets_an_actionable_hint() {
        let out = explain_error("ERROR: [youtube] abc: Sign in to confirm your age");
        assert!(
            out.starts_with("this video is age restricted"),
            "got: {out}"
        );
        // The original is kept underneath rather than thrown away.
        assert!(out.contains("Sign in to confirm your age"));
    }

    #[test]
    fn rate_limiting_is_recognised() {
        assert!(explain_error("ERROR: HTTP Error 429: Too Many Requests")
            .starts_with("the site is rate limiting you"));
    }

    #[test]
    fn an_unknown_error_is_passed_through_cleanly() {
        let out = explain_error("ERROR: something nobody predicted");
        assert_eq!(out, "something nobody predicted");
    }
}
