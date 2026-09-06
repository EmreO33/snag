//! Fetches a yt-dlp binary from the project's own GitHub releases.
//!
//! Snag does not ship yt-dlp: it is a separate project on its own release
//! cadence, and bundling a copy would mean shipping a stale one. Instead the
//! user is offered a one-click install of the current release, straight from
//! the upstream source.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::mpsc::Sender;

use crate::util;

/// The standalone yt-dlp build for this platform.
///
/// Note what is deliberately not here: the bare `yt-dlp` asset. That one is a
/// Python zipapp with a `#!/usr/bin/env python3` shebang, so on a machine
/// without Python it fails to exec with ENOENT, which surfaces as a thoroughly
/// misleading "No such file or directory". Every name below is a real
/// self-contained binary.
pub fn asset_name() -> &'static str {
    if cfg!(windows) {
        if cfg!(target_arch = "aarch64") {
            "yt-dlp_arm64.exe"
        } else {
            "yt-dlp.exe"
        }
    } else if cfg!(target_os = "macos") {
        "yt-dlp_macos"
    } else if cfg!(target_arch = "aarch64") {
        "yt-dlp_linux_aarch64"
    } else {
        "yt-dlp_linux"
    }
}

/// The file name we save it under locally.
pub fn local_name() -> &'static str {
    if cfg!(windows) {
        "yt-dlp.exe"
    } else {
        "yt-dlp"
    }
}

pub fn download_url() -> String {
    format!(
        "https://github.com/yt-dlp/yt-dlp/releases/latest/download/{}",
        asset_name()
    )
}

#[derive(Debug, Clone, PartialEq, Default)]
pub enum InstallState {
    #[default]
    Idle,
    Downloading {
        got: u64,
        total: u64,
    },
    Verifying,
    Done {
        path: PathBuf,
        version: String,
    },
    Failed(String),
}

impl InstallState {
    pub fn busy(&self) -> bool {
        matches!(
            self,
            InstallState::Downloading { .. } | InstallState::Verifying
        )
    }
    /// Download progress, or None while the size is still unknown.
    pub fn fraction(&self) -> Option<f32> {
        match self {
            InstallState::Downloading { got, total } if *total > 0 => {
                Some((*got as f32 / *total as f32).clamp(0.0, 1.0))
            }
            _ => None,
        }
    }
}

#[derive(Debug)]
pub enum InstallEvent {
    State(InstallState),
    Log(String),
}

fn download_to(dest: &Path, tx: &Sender<InstallEvent>) -> Result<(), String> {
    let url = download_url();
    let resp = ureq::get(&url)
        .set("User-Agent", "Snag")
        .timeout(std::time::Duration::from_secs(60))
        .call()
        .map_err(|e| format!("could not reach github: {e}"))?;

    let total: u64 = resp
        .header("Content-Length")
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);

    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("could not create {}: {e}", parent.display()))?;
    }

    // Write beside the target and rename at the end, so a failed or cancelled
    // download can never leave a half-written binary in place.
    let temp = dest.with_extension("part");
    let mut file = std::fs::File::create(&temp)
        .map_err(|e| format!("could not write {}: {e}", temp.display()))?;

    let mut reader = resp.into_reader();
    let mut buf = vec![0u8; 64 * 1024];
    let mut got: u64 = 0;

    loop {
        let n = reader
            .read(&mut buf)
            .map_err(|e| format!("download interrupted: {e}"))?;
        if n == 0 {
            break;
        }
        file.write_all(&buf[..n])
            .map_err(|e| format!("could not write {}: {e}", temp.display()))?;
        got += n as u64;
        let _ = tx.send(InstallEvent::State(InstallState::Downloading {
            got,
            total,
        }));
    }

    file.flush().map_err(|e| e.to_string())?;
    drop(file);

    if got == 0 {
        let _ = std::fs::remove_file(&temp);
        return Err("github returned an empty file".into());
    }

    let _ = std::fs::remove_file(dest);
    std::fs::rename(&temp, dest)
        .map_err(|e| format!("could not move the binary into place: {e}"))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(dest, std::fs::Permissions::from_mode(0o755));
    }

    Ok(())
}

/// Download yt-dlp into `dir` in the background and report progress.
pub fn install(dir: PathBuf, tx: Sender<InstallEvent>, repaint: impl Fn() + Send + 'static) {
    std::thread::spawn(move || {
        let dest = dir.join(local_name());
        let _ = tx.send(InstallEvent::Log(format!(
            "source:      {}",
            download_url()
        )));
        let _ = tx.send(InstallEvent::Log(format!(
            "destination: {}",
            dest.display()
        )));
        let _ = tx.send(InstallEvent::State(InstallState::Downloading {
            got: 0,
            total: 0,
        }));
        repaint();

        if let Err(e) = download_to(&dest, &tx) {
            let _ = tx.send(InstallEvent::State(InstallState::Failed(e)));
            repaint();
            return;
        }

        let _ = tx.send(InstallEvent::State(InstallState::Verifying));
        repaint();

        // A binary that cannot report its own version is not one we should use.
        match util::run_capture(&dest.display().to_string(), &["--version"]) {
            Ok(version) => {
                let version = version.trim().to_string();
                let _ = tx.send(InstallEvent::Log(format!("verified:    yt-dlp {version}")));
                let _ = tx.send(InstallEvent::State(InstallState::Done {
                    path: dest,
                    version,
                }));
            }
            Err(e) => {
                // ENOENT here almost never means the file is missing: it means
                // the kernel could not exec it. Say something useful.
                let hint = if cfg!(unix) && e.contains("No such file or directory") {
                    " (the file is there, but it would not execute. this build should be self-contained, so it may have downloaded incompletely: try again.)"
                } else {
                    ""
                };
                let _ = tx.send(InstallEvent::State(InstallState::Failed(format!(
                    "downloaded, but it would not run: {e}{hint}"
                ))));
            }
        }
        repaint();
    });
}

/// Look for a usable yt-dlp: the configured path first, then PATH, then any
/// copy Snag installed previously.
pub fn detect(configured: &str) -> Option<(String, String)> {
    let mut candidates: Vec<String> = Vec::new();
    let configured = configured.trim();
    if !configured.is_empty() {
        candidates.push(configured.to_string());
    }
    candidates.push("yt-dlp".to_string());
    candidates.push(
        crate::bootstrap::managed_bin_dir()
            .join(local_name())
            .display()
            .to_string(),
    );

    for bin in candidates {
        if let Ok(v) = util::run_capture(&bin, &["--version"]) {
            let v = v.trim().to_string();
            if !v.is_empty() {
                return Some((bin, v));
            }
        }
    }
    None
}

/// Same idea for ffmpeg, which Snag needs but does not install.
pub fn detect_ffmpeg(configured: &str) -> Option<String> {
    let configured = configured.trim();
    let candidates = if configured.is_empty() {
        vec!["ffmpeg".to_string()]
    } else {
        vec![configured.to_string(), "ffmpeg".to_string()]
    };

    for bin in candidates {
        if let Ok(out) = util::run_capture(&bin, &["-version"]) {
            if let Some(first) = out.lines().next() {
                return Some(first.trim().to_string());
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    /// The bare `yt-dlp` asset is a Python zipapp. Downloading it onto a machine
    /// without Python produces a baffling ENOENT, so it must never be chosen.
    #[test]
    fn picks_a_self_contained_ytdlp_build() {
        let name = super::asset_name();
        assert_ne!(name, "yt-dlp", "that asset needs a python interpreter");
        assert!(
            [
                "yt-dlp.exe",
                "yt-dlp_arm64.exe",
                "yt-dlp_macos",
                "yt-dlp_linux",
                "yt-dlp_linux_aarch64",
            ]
            .contains(&name),
            "unexpected asset name: {name}"
        );
    }
}
