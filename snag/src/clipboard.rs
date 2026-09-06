//! Watching the clipboard for links, so copying one is enough to offer it.
//!
//! This deliberately reads nothing but the clipboard's text, keeps only the
//! last value seen so the same link is not offered twice, and never sends
//! anything anywhere. It runs only when the user turns it on.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::Arc;
use std::time::Duration;

use crate::util;

/// How often to look. Slow enough to be invisible on a battery, fast enough
/// that copying a link and glancing at Snag feels immediate.
const POLL: Duration = Duration::from_millis(900);

/// A link the user copied.
#[derive(Debug, Clone)]
pub struct Found {
    pub url: String,
}

/// Watch until `stop` is set. Sends each newly copied link once.
///
/// Surfacing the link is done from here rather than from the UI, because a
/// minimised window gets no redraws: the update loop simply is not running,
/// so anything that waits for it would never happen.
pub fn watch(
    stop: Arc<AtomicBool>,
    hidden: Arc<AtomicBool>,
    restore_on_link: Arc<AtomicBool>,
    tx: Sender<Found>,
    repaint: impl Fn() + Send + 'static,
) {
    std::thread::spawn(move || {
        // Whatever is already on the clipboard when watching starts is not
        // something the user just copied, so take it as already seen.
        let mut last = util::clipboard_text().unwrap_or_default();

        while !stop.load(Ordering::Relaxed) {
            std::thread::sleep(POLL);
            if stop.load(Ordering::Relaxed) {
                break;
            }

            let Some(text) = util::clipboard_text() else {
                continue;
            };
            if text == last {
                continue;
            }
            last = text.clone();

            let trimmed = text.trim();
            // One link, not a wall of text that happens to start with http.
            if trimmed.lines().count() == 1 && util::looks_like_url(trimmed) && trimmed.len() < 2048
            {
                if tx
                    .send(Found {
                        url: trimmed.to_string(),
                    })
                    .is_err()
                {
                    break;
                }

                if hidden.load(Ordering::Relaxed) {
                    // Best effort: works on linux and macos, and is silently
                    // dropped by windows for an app it did not install.
                    notify_found(trimmed);
                    if restore_on_link.load(Ordering::Relaxed) {
                        crate::window::restore();
                    }
                }
                repaint();
            }
        }
    });
}

/// Tell the user about a copied link without stealing focus from whatever they
/// are doing. Returns false when the desktop refused to show it.
pub fn notify_found(url: &str) -> bool {
    let short: String = if url.chars().count() > 70 {
        format!("{}...", url.chars().take(67).collect::<String>())
    } else {
        url.to_string()
    };

    let mut notification = notify_rust::Notification::new();
    notification
        .summary("Snag")
        .body(&format!(
            "Copied a link. Open Snag to download it.\n{short}"
        ))
        .appname("Snag");

    // Windows attributes toasts to a registered app id; ours is the Start Menu
    // shortcut the installer creates.
    #[cfg(windows)]
    notification.app_id("EmreO33.Snag");

    notification.show().is_ok()
}
