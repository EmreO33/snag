//! Asking yt-dlp what a link actually is, before committing to downloading it.
//!
//! One `--dump-single-json` call tells us the title, how long it is, whether it
//! is a playlist, and which qualities exist. That turns the link box from
//! "paste and hope" into something that can show you what you are about to get.

use std::io::Read;
use std::sync::mpsc::Sender;

use crate::settings::Settings;
use crate::util;

/// What a link turned out to be.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Probe {
    pub url: String,
    pub title: String,
    pub uploader: Option<String>,
    pub duration: Option<f64>,
    /// Item count when the link is a playlist.
    pub playlist_items: Option<usize>,
    /// Distinct video heights the site offers, tallest first.
    pub heights: Vec<u32>,
    pub live: bool,
    /// Where the preview image lives, if the site offers one.
    pub thumbnail: Option<String>,
}

impl Probe {
    pub fn is_playlist(&self) -> bool {
        self.playlist_items.is_some_and(|n| n > 1)
    }

    /// "12:34" or "1:02:03", or None for things with no duration.
    pub fn duration_label(&self) -> Option<String> {
        self.duration.filter(|d| *d > 0.0).map(util::human_eta)
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub enum ProbeState {
    #[default]
    Idle,
    Working,
    Done(Box<Probe>),
    Failed(String),
}

#[derive(Debug)]
pub struct ProbeResult {
    /// The URL this was for, so a stale reply for an old link can be dropped.
    pub url: String,
    pub state: ProbeState,
}

fn as_u32(v: &serde_json::Value) -> Option<u32> {
    v.as_u64().and_then(|n| u32::try_from(n).ok())
}

/// Pull the interesting parts out of yt-dlp's JSON.
fn parse(url: &str, json: &serde_json::Value) -> Probe {
    let is_playlist = json.get("_type").and_then(|t| t.as_str()) == Some("playlist");

    let playlist_items = if is_playlist {
        json.get("playlist_count")
            .and_then(|c| c.as_u64())
            .map(|c| c as usize)
            .or_else(|| json.get("entries").and_then(|e| e.as_array()).map(Vec::len))
    } else {
        None
    };

    // Heights are only meaningful for a single video: a flat playlist listing
    // carries no format information.
    let mut heights: Vec<u32> = json
        .get("formats")
        .and_then(|f| f.as_array())
        .map(|formats| {
            formats
                .iter()
                .filter_map(|f| f.get("height").and_then(as_u32))
                .filter(|h| *h > 0)
                .collect()
        })
        .unwrap_or_default();
    heights.sort_unstable_by(|a, b| b.cmp(a));
    heights.dedup();

    Probe {
        url: url.to_string(),
        title: json
            .get("title")
            .and_then(|t| t.as_str())
            .unwrap_or("")
            .to_string(),
        uploader: json
            .get("uploader")
            .or_else(|| json.get("channel"))
            .and_then(|u| u.as_str())
            .map(str::to_string),
        duration: json.get("duration").and_then(|d| d.as_f64()),
        playlist_items,
        heights,
        live: json
            .get("is_live")
            .and_then(|l| l.as_bool())
            .unwrap_or(false),
        thumbnail: json
            .get("thumbnail")
            .and_then(|t| t.as_str())
            .filter(|t| t.starts_with("http"))
            .map(str::to_string)
            .or_else(|| {
                // Playlists carry a list rather than a single one; the last is
                // usually the largest.
                json.get("thumbnails")?
                    .as_array()?
                    .iter()
                    .filter_map(|t| t.get("url")?.as_str())
                    .rfind(|u| u.starts_with("http"))
                    .map(str::to_string)
            }),
    }
}

/// Ask yt-dlp about `url` in the background.
pub fn spawn(
    url: String,
    settings: Settings,
    tx: Sender<ProbeResult>,
    repaint: impl Fn() + Send + 'static,
) {
    std::thread::spawn(move || {
        let _ = tx.send(ProbeResult {
            url: url.clone(),
            state: ProbeState::Working,
        });
        repaint();

        let mut args = vec![
            "--dump-single-json".to_string(),
            // Without this a 200-item playlist would be resolved item by item,
            // which takes far too long for something that runs as you type.
            "--flat-playlist".to_string(),
            "--no-warnings".to_string(),
            "--no-colors".to_string(),
            "-R".to_string(),
            "2".to_string(),
            "--socket-timeout".to_string(),
            "15".to_string(),
        ];
        if !settings.network.proxy.trim().is_empty() {
            args.push("--proxy".to_string());
            args.push(settings.network.proxy.trim().to_string());
        }
        if settings.network.cookies_from_browser != crate::settings::CookieBrowser::None {
            args.push("--cookies-from-browser".to_string());
            args.push(settings.network.cookies_from_browser.label().to_string());
        }
        args.push(url.clone());

        let borrowed: Vec<&str> = args.iter().map(String::as_str).collect();
        let state = match util::run_capture(&settings.ytdlp_bin(), &borrowed) {
            Ok(out) => match serde_json::from_str::<serde_json::Value>(&out) {
                Ok(json) => ProbeState::Done(Box::new(parse(&url, &json))),
                Err(e) => ProbeState::Failed(format!("could not read yt-dlp's reply: {e}")),
            },
            Err(e) => ProbeState::Failed(crate::ytdlp::explain_error(&e)),
        };

        let _ = tx.send(ProbeResult { url, state });
        repaint();
    });
}

/// How wide a fetched thumbnail is kept. The card shows it far smaller than
/// the source image, and there is no sense holding a 1280px texture for it.
const THUMBNAIL_WIDTH: u32 = 320;

/// Fetch and decode a thumbnail. Returns None for anything that does not
/// arrive as a usable image, which is not worth troubling the user about.
pub fn fetch_thumbnail(url: &str) -> Option<egui::ColorImage> {
    let resp = ureq::get(url)
        .set("User-Agent", "Snag")
        .timeout(std::time::Duration::from_secs(15))
        .call()
        .ok()?;

    let mut bytes = Vec::new();
    // Cap it: a preview image has no business being larger than this, and an
    // unbounded read from a remote server is not something to invite.
    std::io::Read::take(resp.into_reader(), 8 * 1024 * 1024)
        .read_to_end(&mut bytes)
        .ok()?;

    let decoded = image::load_from_memory(&bytes).ok()?;
    let scaled = decoded.thumbnail(THUMBNAIL_WIDTH, THUMBNAIL_WIDTH);
    let rgba = scaled.to_rgba8();
    let size = [rgba.width() as usize, rgba.height() as usize];
    Some(egui::ColorImage::from_rgba_unmultiplied(
        size,
        rgba.as_raw(),
    ))
}

#[cfg(test)]
mod tests {
    use super::parse;

    #[test]
    fn reads_a_single_video() {
        let json = serde_json::json!({
            "title": "Big Buck Bunny",
            "channel": "Blender",
            "duration": 635.0,
            "formats": [
                {"height": 1080}, {"height": 720}, {"height": 1080}, {"height": null}
            ]
        });
        let p = parse("https://example.test/v", &json);
        assert_eq!(p.title, "Big Buck Bunny");
        assert_eq!(p.uploader.as_deref(), Some("Blender"));
        assert_eq!(p.duration_label().as_deref(), Some("10:35"));
        assert!(!p.is_playlist());
        // Tallest first, no duplicates, nothing without a height.
        assert_eq!(p.heights, vec![1080, 720]);
    }

    #[test]
    fn reads_a_playlist() {
        let json = serde_json::json!({
            "_type": "playlist",
            "title": "My mix",
            "playlist_count": 40
        });
        let p = parse("https://example.test/list", &json);
        assert!(p.is_playlist());
        assert_eq!(p.playlist_items, Some(40));
    }

    #[test]
    fn a_playlist_of_one_is_not_treated_as_a_playlist() {
        let json = serde_json::json!({ "_type": "playlist", "playlist_count": 1 });
        let p = parse("https://example.test/list", &json);
        assert!(
            !p.is_playlist(),
            "offering to download all of one item is noise"
        );
    }
}
