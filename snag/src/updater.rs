//! yt-dlp version checking and updating. Checks are read-only (GitHub releases
//! API); installing shells out to `yt-dlp -U` so the binary updates itself,
//! or, for a copy that cannot write to itself, installs Snag's own.

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
        // Generous for a check nobody is waiting on: a slow network or a
        // vpn can take most of 15s just to connect.
        .timeout(std::time::Duration::from_secs(30))
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

/// Whether `bin` can update itself: it is Snag's own copy, or a file this
/// user can write to. Asked by opening it for writing, which changes nothing
/// and is the one test that also sees read-only mounts.
fn updates_in_place(bin: &str) -> bool {
    let Some(path) = resolve(bin) else {
        // Not found at all: running -U will say so better than a guess.
        return true;
    };
    if path.starts_with(crate::bootstrap::managed_bin_dir()) {
        return true;
    }
    std::fs::OpenOptions::new().append(true).open(&path).is_ok()
}

/// The file a bare command name would run, found the way the shell would.
fn resolve(bin: &str) -> Option<std::path::PathBuf> {
    let direct = std::path::Path::new(bin);
    if direct.components().count() > 1 {
        return direct.is_file().then(|| direct.to_path_buf());
    }
    let names: Vec<String> = if cfg!(windows) && !bin.to_lowercase().ends_with(".exe") {
        vec![format!("{bin}.exe"), bin.to_string()]
    } else {
        vec![bin.to_string()]
    };
    std::env::split_paths(&std::env::var_os("PATH")?)
        .flat_map(|dir| names.iter().map(move |n| dir.join(n)))
        .find(|p| p.is_file())
}

/// Install Snag's own yt-dlp and report it as the update.
fn install_managed(tx: &Sender<UpdateEvent>) {
    let _ = tx.send(UpdateEvent::Log(
        "this yt-dlp cannot update itself, so snag is installing its own copy".into(),
    ));
    let (itx, irx) = std::sync::mpsc::channel();
    crate::installer::install_now(&crate::bootstrap::managed_bin_dir(), &itx, &|| {});
    let mut outcome = Err("the install ended without saying how".to_string());
    for ev in irx.try_iter() {
        match ev {
            crate::installer::InstallEvent::Log(l) => {
                let _ = tx.send(UpdateEvent::Log(l));
            }
            crate::installer::InstallEvent::State(crate::installer::InstallState::Done {
                version,
                ..
            }) => outcome = Ok(version),
            crate::installer::InstallEvent::State(crate::installer::InstallState::Failed(e)) => {
                outcome = Err(e)
            }
            _ => {}
        }
    }
    let _ = tx.send(UpdateEvent::State(match outcome {
        Ok(version) => {
            let _ = tx.send(UpdateEvent::Current(version.clone()));
            UpdateState::Installed { version }
        }
        Err(e) => UpdateState::Error(e),
    }));
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

    // `yt-dlp -U` rewrites the file it runs from, which only works when that
    // file is ours to write. A yt-dlp from apt, pacman or a Flatpak's
    // read-only /app is not, and used to end in an error telling the user
    // to go and update it somewhere else. Snag installs its own current
    // copy instead, which it prefers from then on.
    if !updates_in_place(bin) {
        install_managed(tx);
        return;
    }

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
                // A yt-dlp that snag manages gets its javascript runtime kept
                // alongside, so a copy installed before deno came with it
                // picks one up here. A yt-dlp from elsewhere is left alone.
                let managed = crate::bootstrap::managed_bin_dir();
                if std::path::Path::new(bin).starts_with(&managed) {
                    let (itx, irx) = std::sync::mpsc::channel();
                    let result = crate::installer::ensure_deno(&managed, &itx);
                    for ev in irx.try_iter() {
                        if let crate::installer::InstallEvent::Log(l) = ev {
                            let _ = tx.send(UpdateEvent::Log(l));
                        }
                    }
                    let _ = tx.send(UpdateEvent::Log(match result {
                        Ok(v) => format!("deno {v} is alongside yt-dlp"),
                        Err(e) => format!("deno could not be installed: {e}"),
                    }));
                }
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

#[cfg(test)]
mod tests {
    use super::updates_in_place;

    #[test]
    fn a_yt_dlp_this_user_can_write_updates_itself() {
        let file = std::env::temp_dir().join(format!("snag-ytdlp-test-{}", std::process::id()));
        std::fs::write(&file, b"#!/bin/sh\n").unwrap();
        assert!(updates_in_place(&file.display().to_string()));
        let _ = std::fs::remove_file(&file);
    }

    /// What apt, pacman and a Flatpak's /app leave: a file only root, or
    /// nobody, can write. Skipped when the tests run as root.
    #[cfg(unix)]
    #[test]
    fn a_yt_dlp_owned_by_the_system_does_not() {
        let running_as_root = std::env::var("USER").is_ok_and(|u| u == "root");
        if !running_as_root {
            assert!(!updates_in_place("/bin/sh"));
        }
    }
}
