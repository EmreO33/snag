//! yt-dlp version checking and updating. Checks are read-only (GitHub releases
//! API); installing shells out to `yt-dlp -U` so the binary updates itself.

use std::sync::mpsc::Sender;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::util;

const LATEST_RELEASE_API: &str = "https://api.github.com/repos/yt-dlp/yt-dlp/releases/latest";

#[derive(Debug, Clone, PartialEq, Default)]
pub enum UpdateState {
    #[default]
    Unknown,
    Missing(String),
    Checking,
    UpToDate,
    Available {
        latest: String,
    },
    Installing,
    Installed {
        version: String,
    },
    Error(String),
}

impl UpdateState {
    pub fn busy(&self) -> bool {
        matches!(self, UpdateState::Checking | UpdateState::Installing)
    }
}

#[derive(Debug)]
pub enum UpdateEvent {
    Current(String),
    State(UpdateState),
    Log(String),
}

pub fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Read the installed yt-dlp version, or explain why we cannot.
pub fn current_version(bin: &str) -> Result<String, String> {
    util::run_capture(bin, &["--version"]).map(|v| v.trim().to_string())
}

fn latest_version() -> Result<String, String> {
    let resp = ureq::get(LATEST_RELEASE_API)
        .set("User-Agent", "Snag")
        .set("Accept", "application/vnd.github+json")
        .timeout(std::time::Duration::from_secs(15))
        .call()
        .map_err(|e| format!("could not reach github: {e}"))?;

    let json: serde_json::Value = resp
        .into_json()
        .map_err(|e| format!("unexpected response from github: {e}"))?;

    json.get("tag_name")
        .and_then(|v| v.as_str())
        .map(|s| s.trim_start_matches('v').to_string())
        .ok_or_else(|| "github response had no release tag".to_string())
}

/// Check for a newer yt-dlp in the background. If `auto_install`, install it too.
pub fn check(
    bin: String,
    auto_install: bool,
    tx: Sender<UpdateEvent>,
    repaint: impl Fn() + Send + 'static,
) {
    std::thread::spawn(move || {
        let _ = tx.send(UpdateEvent::State(UpdateState::Checking));
        repaint();

        let current = match current_version(&bin) {
            Ok(v) => v,
            Err(e) => {
                let _ = tx.send(UpdateEvent::State(UpdateState::Missing(e)));
                repaint();
                return;
            }
        };
        let _ = tx.send(UpdateEvent::Current(current.clone()));

        match latest_version() {
            Ok(latest) => {
                if util::version_is_newer(&latest, &current) {
                    let _ = tx.send(UpdateEvent::State(UpdateState::Available {
                        latest: latest.clone(),
                    }));
                    repaint();
                    if auto_install {
                        install_blocking(&bin, &tx);
                    }
                } else {
                    let _ = tx.send(UpdateEvent::State(UpdateState::UpToDate));
                }
            }
            Err(e) => {
                let _ = tx.send(UpdateEvent::State(UpdateState::Error(e)));
            }
        }
        repaint();
    });
}

/// Install the latest yt-dlp in the background.
pub fn install(bin: String, tx: Sender<UpdateEvent>, repaint: impl Fn() + Send + 'static) {
    std::thread::spawn(move || {
        install_blocking(&bin, &tx);
        repaint();
    });
}

fn install_blocking(bin: &str, tx: &Sender<UpdateEvent>) {
    let _ = tx.send(UpdateEvent::State(UpdateState::Installing));

    let result = util::command(bin)
        .arg("-U")
        .output()
        .map_err(|e| format!("could not run {bin}: {e}"));

    match result {
        Ok(out) => {
            let text = format!(
                "{}{}",
                String::from_utf8_lossy(&out.stdout),
                String::from_utf8_lossy(&out.stderr)
            );
            for line in text.lines().filter(|l| !l.trim().is_empty()) {
                let _ = tx.send(UpdateEvent::Log(line.trim().to_string()));
            }

            if out.status.success() {
                match current_version(bin) {
                    Ok(v) => {
                        let _ = tx.send(UpdateEvent::Current(v.clone()));
                        let _ = tx.send(UpdateEvent::State(UpdateState::Installed { version: v }));
                    }
                    Err(e) => {
                        let _ = tx.send(UpdateEvent::State(UpdateState::Error(e)));
                    }
                }
            } else {
                // The usual cause is a package-manager install that cannot self-update.
                let hint = if text.contains("not directly") || text.contains("pip") {
                    " (this yt-dlp was installed by a package manager, so update it there)"
                } else {
                    ""
                };
                let last = text
                    .lines()
                    .rev()
                    .find(|l| !l.trim().is_empty())
                    .unwrap_or("update failed")
                    .trim()
                    .to_string();
                let _ = tx.send(UpdateEvent::State(UpdateState::Error(format!(
                    "{last}{hint}"
                ))));
            }
        }
        Err(e) => {
            let _ = tx.send(UpdateEvent::State(UpdateState::Error(e)));
        }
    }
}
