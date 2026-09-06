//! Signing in to YouTube, so age-restricted, private and members-only videos
//! can be downloaded.
//!
//! Snag never asks for a password and has no login form. Handing an account
//! password to a downloader is both something YouTube treats as suspicious
//! (it locks accounts over it) and something Snag has no business holding.
//! Instead this borrows the session cookies from a browser you have already
//! signed in with, which is what yt-dlp is built to accept.
//!
//! The cookies are read from the local browser profile by yt-dlp, sent to
//! youtube.com and nowhere else, and never written down by Snag.

use std::sync::mpsc::Sender;

use crate::settings::{CookieBrowser, Settings};
use crate::util;

/// Watch Later: every account has one, and it cannot be reached at all
/// without being signed in.
///
/// The obvious choice, the subscriptions feed, is no good: signed out it
/// answers with an empty feed and exits successfully, so the check would
/// cheerfully report a sign-in that does not exist. Watch Later fails outright
/// instead, which is the answer this needs.
const SIGNED_IN_PROBE: &str = "https://www.youtube.com/playlist?list=WL";

/// Hosts the sign-in applies to. Cookies are scoped to these by default: a
/// YouTube session has no business being offered to an unrelated site.
const YOUTUBE_HOSTS: [&str; 4] = [
    "youtube.com",
    "youtu.be",
    "youtube-nocookie.com",
    "youtubekids.com",
];

/// The host part of a URL, lowercased, without credentials or port.
fn host_of(url: &str) -> Option<String> {
    let rest = url
        .split_once("://")
        .map(|(_, rest)| rest)
        .unwrap_or(url.trim());
    let authority = rest.split(['/', '?', '#']).next()?;
    // user:pass@host is legal, and would otherwise hide the real host.
    let host = authority.rsplit('@').next()?;
    let host = host.split(':').next()?.trim().trim_end_matches('.');
    if host.is_empty() {
        None
    } else {
        Some(host.to_ascii_lowercase())
    }
}

/// Whether a link belongs to YouTube, including its regional and app
/// subdomains (m., music., www.) and the short youtu.be form.
pub fn is_youtube(url: &str) -> bool {
    let Some(host) = host_of(url) else {
        return false;
    };
    YOUTUBE_HOSTS
        .iter()
        .any(|d| host == *d || host.ends_with(&format!(".{d}")))
}

/// The yt-dlp arguments that carry the sign-in, if there is one to carry.
///
/// Returns nothing when no browser is chosen, or when the link is not a
/// YouTube one and cookies are scoped to YouTube.
pub fn cookie_args(s: &Settings, url: &str) -> Vec<String> {
    let n = &s.network;
    let file = n.cookie_file.trim();
    if n.cookies_from_browser == CookieBrowser::None && file.is_empty() {
        return Vec::new();
    }
    if n.cookies_youtube_only && !is_youtube(url) {
        return Vec::new();
    }

    // A file is an explicit, exported jar: if one is given it is what the user
    // means, so it is used on its own rather than mixed with a live profile.
    if !file.is_empty() {
        return vec!["--cookies".to_string(), file.to_string()];
    }
    vec![
        "--cookies-from-browser".to_string(),
        n.cookies_from_browser.label().to_string(),
    ]
}

/// Whether anything is configured to sign in with at all.
pub fn configured(s: &Settings) -> bool {
    s.network.cookies_from_browser != CookieBrowser::None
        || !s.network.cookie_file.trim().is_empty()
}

/// True for the browsers that seal their cookie store on Windows.
///
/// Chromium started encrypting cookies so that only Chrome itself can read
/// them, which no external tool can undo. It affects the Chromium browsers on
/// Windows and is the most common reason this feature appears to do nothing,
/// so it is worth saying before the attempt rather than after.
pub fn sealed_on_windows(browser: CookieBrowser) -> bool {
    cfg!(windows)
        && matches!(
            browser,
            CookieBrowser::Chrome
                | CookieBrowser::Edge
                | CookieBrowser::Brave
                | CookieBrowser::Opera
                | CookieBrowser::Vivaldi
        )
}

/// What the last sign-in check found.
#[derive(Debug, Clone, PartialEq, Default)]
pub enum SignInState {
    /// Not checked since Snag started.
    #[default]
    Unknown,
    Checking,
    /// The cookies were read and YouTube accepted them.
    SignedIn,
    /// The cookies were read, but they are not a signed-in session.
    SignedOut,
    /// The cookie store could not be read at all.
    Unreadable(String),
    Error(String),
}

impl SignInState {
    pub fn busy(&self) -> bool {
        matches!(self, SignInState::Checking)
    }
}

/// Turn a failed check into the state that describes it.
///
/// Kept separate from the process handling so the classification can be
/// tested against the messages yt-dlp actually prints.
pub fn classify_failure(raw: &str) -> SignInState {
    let lower = raw.to_lowercase();

    // Reaching the feed at all proves the session worked; an account with no
    // subscriptions then gives an empty playlist, which is not a failure.
    if lower.contains("no video") || lower.contains("playlist is empty") {
        return SignInState::SignedIn;
    }

    let unreadable = [
        "could not copy",
        "unable to read",
        "failed to decrypt",
        "permission denied",
        "database is locked",
        "no such file",
        "could not find",
        "unsupported browser",
        "none of the available browsers",
        "cookie database",
        "keyring",
        "app-bound",
    ];
    if unreadable.iter().any(|m| lower.contains(m)) {
        return SignInState::Unreadable(raw.trim().to_string());
    }

    let signed_out = [
        // What youtube says about watch later when nobody is signed in.
        "does not exist",
        "sign in",
        "log in",
        "login required",
        "requires authentication",
        "not logged in",
        "please authenticate",
        "unable to download api page",
        "account cookies are invalid",
    ];
    if signed_out.iter().any(|m| lower.contains(m)) {
        return SignInState::SignedOut;
    }

    SignInState::Error(crate::ytdlp::explain_error(raw))
}

/// Ask YouTube, using the configured cookies, whether we are signed in.
pub fn check(
    bin: String,
    settings: Settings,
    tx: Sender<SignInState>,
    repaint: impl Fn() + Send + 'static,
) {
    std::thread::spawn(move || {
        let _ = tx.send(SignInState::Checking);
        repaint();

        let mut args = vec![
            "--dump-single-json".to_string(),
            "--flat-playlist".to_string(),
            // One item is enough to prove the fetch worked, and pulling the
            // whole subscription feed would be a rude way to answer a yes or
            // no question.
            "--playlist-items".to_string(),
            "1".to_string(),
            "--no-warnings".to_string(),
            "--no-colors".to_string(),
            "--socket-timeout".to_string(),
            "20".to_string(),
        ];
        // The probe URL is a YouTube one, so the scoping rule passes the
        // cookies through whatever the setting says.
        args.extend(cookie_args(&settings, SIGNED_IN_PROBE));
        if !settings.network.proxy.trim().is_empty() {
            args.push("--proxy".to_string());
            args.push(settings.network.proxy.trim().to_string());
        }
        args.push(SIGNED_IN_PROBE.to_string());

        let borrowed: Vec<&str> = args.iter().map(String::as_str).collect();
        let state = match util::run_capture(&bin, &borrowed) {
            Ok(_) => SignInState::SignedIn,
            Err(e) => classify_failure(&e),
        };

        let _ = tx.send(state);
        repaint();
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn youtube_links_are_recognised_in_every_shape() {
        for url in [
            "https://www.youtube.com/watch?v=abc",
            "https://youtube.com/watch?v=abc",
            "https://m.youtube.com/watch?v=abc",
            "https://music.youtube.com/watch?v=abc",
            "https://youtu.be/abc",
            "http://www.youtube-nocookie.com/embed/abc",
            "https://www.youtube.com",
        ] {
            assert!(is_youtube(url), "should be youtube: {url}");
        }
    }

    #[test]
    fn lookalike_hosts_are_not_youtube() {
        for url in [
            "https://vimeo.com/123",
            "https://notyoutube.com/watch?v=abc",
            "https://youtube.com.evil.example/watch?v=abc",
            "https://evil.example/?u=https://youtube.com/watch",
            "",
        ] {
            assert!(!is_youtube(url), "should not be youtube: {url}");
        }
    }

    #[test]
    fn credentials_in_the_url_do_not_hide_the_host() {
        assert!(is_youtube("https://user:pass@www.youtube.com/watch?v=a"));
        assert!(!is_youtube("https://www.youtube.com@evil.example/watch"));
    }

    #[test]
    fn a_locked_cookie_store_is_told_apart_from_being_signed_out() {
        let sealed = classify_failure(
            "ERROR: Could not copy Chrome cookie database. See https://github.com/yt-dlp/yt-dlp/issues/7271 for more info",
        );
        assert!(matches!(sealed, SignInState::Unreadable(_)));

        // What chromium on windows actually says, sealed cookie store and all.
        assert!(matches!(
            classify_failure("ERROR: Failed to decrypt with DPAPI. See https://github.com/yt-dlp/yt-dlp/issues/10927 for more info"),
            SignInState::Unreadable(_)
        ));

        assert_eq!(
            classify_failure("ERROR: [youtube:tab] Please sign in"),
            SignInState::SignedOut
        );
    }

    #[test]
    fn the_sign_in_is_offered_to_youtube_and_withheld_from_everyone_else() {
        let mut s = Settings::default();
        s.network.cookies_from_browser = CookieBrowser::Firefox;
        assert!(s.network.cookies_youtube_only, "scoping is the default");

        assert_eq!(
            cookie_args(&s, "https://youtu.be/abc"),
            vec!["--cookies-from-browser", "firefox"]
        );
        assert!(cookie_args(&s, "https://vimeo.com/1").is_empty());

        // Turning the scope off is what it says it is.
        s.network.cookies_youtube_only = false;
        assert_eq!(cookie_args(&s, "https://vimeo.com/1").len(), 2);
    }

    #[test]
    fn a_cookie_file_replaces_the_browser_rather_than_joining_it() {
        let mut s = Settings::default();
        s.network.cookies_from_browser = CookieBrowser::Firefox;
        s.network.cookie_file = "  C:/jar.txt  ".to_string();
        assert_eq!(
            cookie_args(&s, "https://www.youtube.com/watch?v=a"),
            vec!["--cookies", "C:/jar.txt"]
        );
    }

    /// The whole path, against the real yt-dlp and the real youtube: spawn
    /// the check with no cookies configured and confirm it says so.
    ///
    /// Ignored by default because it needs the network and a yt-dlp on PATH,
    /// neither of which belongs in a unit test run. Run it with
    /// `cargo test -- --ignored` when this code changes.
    #[test]
    #[ignore = "hits the network"]
    fn the_check_reports_signed_out_when_there_are_no_cookies() {
        let (tx, rx) = std::sync::mpsc::channel();
        check(
            Settings::default().ytdlp_bin(),
            Settings::default(),
            tx,
            || {},
        );

        let first = rx.recv_timeout(std::time::Duration::from_secs(60)).unwrap();
        assert_eq!(first, SignInState::Checking);
        let verdict = rx.recv_timeout(std::time::Duration::from_secs(60)).unwrap();
        assert_eq!(verdict, SignInState::SignedOut, "no cookies is signed out");
    }

    #[test]
    fn nothing_is_passed_when_nobody_has_signed_in() {
        let s = Settings::default();
        assert!(cookie_args(&s, "https://www.youtube.com/watch?v=a").is_empty());
        assert!(!configured(&s));
    }

    #[test]
    fn an_empty_watch_later_still_counts_as_signed_in() {
        assert_eq!(
            classify_failure("ERROR: [youtube:tab] WL: No video formats found"),
            SignInState::SignedIn
        );
    }

    /// The exact line yt-dlp prints for watch later with no cookies. If this
    /// were ever classified as anything else, the screen would claim a
    /// sign-in that is not there.
    #[test]
    fn watch_later_without_a_session_reads_as_signed_out() {
        assert_eq!(
            classify_failure("ERROR: [youtube:tab] WL: YouTube said: The playlist does not exist."),
            SignInState::SignedOut
        );
    }
}
