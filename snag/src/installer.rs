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

fn download_to(url: &str, dest: &Path, tx: &Sender<InstallEvent>) -> Result<(), String> {
    let resp = ureq::get(url)
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
        install_now(&dir, &tx, &repaint);
    });
}

/// Download yt-dlp into `dir`, check it runs, and put deno beside it, on the
/// calling thread. The answer arrives as the final `InstallEvent::State`.
pub fn install_now(dir: &Path, tx: &Sender<InstallEvent>, repaint: &dyn Fn()) {
    {
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

        if let Err(e) = download_to(&download_url(), &dest, tx) {
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

                // yt-dlp wants a javascript runtime for youtube and has
                // deprecated working without one, so it comes along with
                // yt-dlp rather than being a second thing to know about. Not
                // having it is not yet fatal, so a failure here is logged
                // and yt-dlp still counts as installed.
                let _ = tx.send(InstallEvent::State(InstallState::Verifying));
                match ensure_deno(dir, tx) {
                    Ok(v) => {
                        let _ = tx.send(InstallEvent::Log(format!("verified:    deno {v}")));
                    }
                    Err(e) => {
                        let _ = tx.send(InstallEvent::Log(format!(
                            "deno:        not installed ({e}). youtube still works today, but yt-dlp has deprecated running without it."
                        )));
                    }
                }

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
    }
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

// --- deno -------------------------------------------------------------------
//
// yt-dlp solves youtube's player challenges in javascript and needs a runtime
// to run it. Without one it falls back to a path it has already deprecated
// and warns on every download. Deno is the runtime it enables by default, it
// ships as a single static binary, and its releases are on github like
// yt-dlp's, so it is fetched the same way and kept in the same folder.

/// The deno release asset for this platform.
pub fn deno_asset_name() -> &'static str {
    if cfg!(windows) {
        // Deno publishes no arm64 windows build; the x86_64 one runs there
        // under emulation, which is slow but is what there is.
        "deno-x86_64-pc-windows-msvc.zip"
    } else if cfg!(target_os = "macos") {
        if cfg!(target_arch = "aarch64") {
            "deno-aarch64-apple-darwin.zip"
        } else {
            "deno-x86_64-apple-darwin.zip"
        }
    } else if cfg!(target_arch = "aarch64") {
        "deno-aarch64-unknown-linux-gnu.zip"
    } else {
        "deno-x86_64-unknown-linux-gnu.zip"
    }
}

pub fn deno_local_name() -> &'static str {
    if cfg!(windows) {
        "deno.exe"
    } else {
        "deno"
    }
}

pub fn deno_download_url() -> String {
    format!(
        "https://github.com/denoland/deno/releases/latest/download/{}",
        deno_asset_name()
    )
}

/// Where a snag-installed deno lives, if it is there.
///
/// Only snag's own copy is reported: a deno on PATH is found by yt-dlp on
/// its own and needs no pointing at.
pub fn managed_deno() -> Option<PathBuf> {
    let path = crate::bootstrap::managed_bin_dir().join(deno_local_name());
    path.is_file().then_some(path)
}

/// The version of a deno binary, or why it could not be asked.
pub fn deno_version(bin: &Path) -> Result<String, String> {
    let out = util::run_capture(&bin.display().to_string(), &["--version"])?;
    // "deno 2.9.7 (stable, release, x86_64-pc-windows-msvc)" on the first line.
    out.lines()
        .next()
        .and_then(|l| l.split_whitespace().nth(1))
        .map(str::to_string)
        .ok_or_else(|| "deno gave no version".to_string())
}

/// Put a deno into `dir` unless one is already there, and return its version.
///
/// Downloads the release zip, pulls the one binary out of it, and checks it
/// runs. The zip is written beside the target so nothing half-extracted can be
/// mistaken for a runtime.
pub fn ensure_deno(dir: &Path, tx: &Sender<InstallEvent>) -> Result<String, String> {
    let dest = dir.join(deno_local_name());
    if dest.is_file() {
        if let Ok(v) = deno_version(&dest) {
            return Ok(v);
        }
        // Present but broken: fall through and replace it.
    }

    let _ = tx.send(InstallEvent::Log(format!(
        "source:      {}",
        deno_download_url()
    )));
    // Not "deno.zip.part": download_to makes its own temp name by swapping
    // the extension for .part, and that name must not collide with this one.
    let zip_path = dir.join("deno-download.zip");
    download_to(&deno_download_url(), &zip_path, tx)?;

    let extracted = (|| -> Result<(), String> {
        let file = std::fs::File::open(&zip_path).map_err(|e| e.to_string())?;
        let mut archive = zip::ZipArchive::new(file).map_err(|e| format!("bad zip: {e}"))?;
        let wanted = deno_local_name();
        let mut entry = archive
            .by_name(wanted)
            .map_err(|_| format!("{wanted} is not in the archive"))?;

        let temp = dest.with_extension("part");
        let mut out = std::fs::File::create(&temp).map_err(|e| e.to_string())?;
        std::io::copy(&mut entry, &mut out).map_err(|e| e.to_string())?;
        drop(out);
        let _ = std::fs::remove_file(&dest);
        std::fs::rename(&temp, &dest).map_err(|e| e.to_string())?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&dest, std::fs::Permissions::from_mode(0o755));
        }
        Ok(())
    })();
    let _ = std::fs::remove_file(&zip_path);
    extracted?;

    let _ = tx.send(InstallEvent::Log(format!(
        "destination: {}",
        dest.display()
    )));
    deno_version(&dest)
}

// --- ffmpeg -----------------------------------------------------------------
//
// ffmpeg is handled differently from yt-dlp. The project publishes no official
// binaries, so rather than picking a third-party build on the user's behalf,
// Snag defers to the platform's own package manager. On Windows that can be
// driven from here without elevation; elsewhere it needs root, which a desktop
// app has no business asking for, so the user is handed the exact command.

/// The winget package: a versioned ffmpeg release, installed per-user.
pub const FFMPEG_WINGET_ID: &str = "Gyan.FFmpeg";

#[derive(Debug, Clone, PartialEq)]
pub enum FfmpegPlan {
    /// Snag can run this itself.
    Automatic { command: String },
    /// The user has to run it, usually because it needs root.
    Manual { command: String },
}

/// The install command for whichever package manager this machine has.
fn package_manager_command() -> String {
    if cfg!(target_os = "macos") {
        return "brew install ffmpeg".to_string();
    }
    for (tool, command) in [
        ("apt-get", "sudo apt install ffmpeg"),
        // Fedora's own archive carries it as ffmpeg-free; plain "ffmpeg"
        // only exists once RPM Fusion has been added, so it fails on a
        // stock install.
        ("dnf", "sudo dnf install ffmpeg-free"),
        ("pacman", "sudo pacman -S ffmpeg"),
        ("zypper", "sudo zypper install ffmpeg"),
        ("apk", "sudo apk add ffmpeg"),
    ] {
        if util::run_capture(tool, &["--version"]).is_ok() {
            return command.to_string();
        }
    }
    "sudo apt install ffmpeg".to_string()
}

/// How ffmpeg should be installed on this machine.
///
/// Written with `cfg!` rather than `#[cfg]` so every branch is compiled on
/// every platform: both variants stay live, and a mistake in the one that does
/// not apply here still fails the build.
pub fn ffmpeg_plan() -> FfmpegPlan {
    if cfg!(windows) {
        FfmpegPlan::Automatic {
            command: format!("winget install --id {FFMPEG_WINGET_ID}"),
        }
    } else {
        FfmpegPlan::Manual {
            command: package_manager_command(),
        }
    }
}

/// Install ffmpeg. Only Windows can actually do this without asking for root,
/// so elsewhere this reports the command to run instead.
///
/// Defined for every platform on purpose: gating it behind cfg leaves the code
/// that calls it conditionally dead, which the compiler rightly complains
/// about on the platforms where it is never reached.
pub fn install_ffmpeg(tx: Sender<InstallEvent>, repaint: impl Fn() + Send + 'static) {
    if !cfg!(windows) {
        if let FfmpegPlan::Manual { command } = ffmpeg_plan() {
            let _ = tx.send(InstallEvent::State(InstallState::Failed(format!(
                "installing ffmpeg here needs root, which snag will not ask for. run: {command}"
            ))));
        }
        repaint();
        return;
    }
    std::thread::spawn(move || {
        let _ = tx.send(InstallEvent::Log(format!(
            "running:     winget install --id {FFMPEG_WINGET_ID}"
        )));
        // winget redraws its progress on one line, so there is nothing useful to
        // turn into a percentage: show an indeterminate bar instead.
        let _ = tx.send(InstallEvent::State(InstallState::Downloading {
            got: 0,
            total: 0,
        }));
        repaint();

        let result = util::command("winget")
            .args([
                "install",
                "--id",
                FFMPEG_WINGET_ID,
                "-e",
                "--accept-package-agreements",
                "--accept-source-agreements",
                "--disable-interactivity",
            ])
            .output();

        let output = match result {
            Ok(o) => o,
            Err(e) => {
                let _ = tx.send(InstallEvent::State(InstallState::Failed(format!(
                    "could not run winget: {e}. install ffmpeg yourself from ffmpeg.org."
                ))));
                repaint();
                return;
            }
        };

        let text = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        let tail: Vec<&str> = text
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .collect();
        for line in tail.iter().rev().take(4).rev() {
            let _ = tx.send(InstallEvent::Log((*line).to_string()));
        }

        let _ = tx.send(InstallEvent::State(InstallState::Verifying));
        repaint();

        let state = match detect_ffmpeg("") {
            Some(version) => {
                let _ = tx.send(InstallEvent::Log(format!("verified:    {version}")));
                InstallState::Done {
                    path: std::path::PathBuf::from("ffmpeg"),
                    version,
                }
            }
            None if output.status.success() => InstallState::Failed(
                "winget reported success, but ffmpeg is still not on PATH. restarting snag usually picks it up."
                    .into(),
            ),
            None => InstallState::Failed(
                tail.last()
                    .copied()
                    .unwrap_or("winget could not install ffmpeg")
                    .to_string(),
            ),
        };
        let _ = tx.send(InstallEvent::State(state));
        repaint();
    });
}

/// Same idea for ffmpeg, which Snag needs but does not install.
/// The hardware h264 encoders this ffmpeg was built with.
///
/// Built with is not the same as usable: a build can carry h264_amf on a
/// machine with no amd card. That failure only shows at encode time, and is
/// explained then. What this rules out is offering an encoder the binary
/// cannot even name.
pub fn ffmpeg_hw_encoders(bin: &str) -> Vec<String> {
    let Ok(out) = util::run_capture(bin, &["-hide_banner", "-encoders"]) else {
        return Vec::new();
    };
    ["h264_nvenc", "h264_qsv", "h264_amf"]
        .iter()
        .filter(|name| {
            out.lines()
                .any(|line| line.split_whitespace().nth(1) == Some(name))
        })
        .map(|name| name.to_string())
        .collect()
}

/// Just the number out of ffmpeg's banner line, which otherwise reads
/// "ffmpeg version 9.0.1-full_build-www.gyan.dev Copyright (c) 2000-2026 the
/// FFmpeg developers" and takes two lines of a card to say "9.0.1".
fn ffmpeg_version_number(banner: &str) -> String {
    let after = banner
        .split_once("version ")
        .map(|(_, rest)| rest)
        .unwrap_or(banner);
    let token = after.split_whitespace().next().unwrap_or(after);
    // The number ends where the build's own tag begins: "9.0.1-full_build",
    // "n7.1", "N-118000-gabcdef" are all things ffmpeg calls a version.
    let number: String = token
        .trim_start_matches('n')
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == '.')
        .collect();
    if number.is_empty() {
        token.to_string()
    } else {
        number
    }
}

/// Where ffmpeg was last found, for everything that has to actually run it.
///
/// Detection used to answer only "yes, version 9.0.1" and leave running it
/// to `ffmpeg` on PATH. That is not the same question: winget's ffmpeg puts
/// its own folder on PATH, and PATH is inherited, so a Snag started by
/// something that predates the install (an installer, say) has a version it
/// can see and a binary it cannot run. Remembering the path fixes that, and
/// costs nothing when ffmpeg is on PATH anyway.
/// Outer None means "nobody has looked yet", which is a different answer
/// from "looked and it is not there".
static FOUND_FFMPEG: std::sync::RwLock<Option<Option<String>>> = std::sync::RwLock::new(None);

pub fn found_ffmpeg() -> Option<String> {
    if let Ok(known) = FOUND_FFMPEG.read() {
        if let Some(answer) = known.as_ref() {
            return answer.clone();
        }
    }
    // Nobody has looked yet, so look now rather than answering "ffmpeg" and
    // leaving a download to fail at the merge. This happens on a job's own
    // thread: the UI has its own probe running by then, and whichever
    // finishes first saves the other the work.
    detect_ffmpeg("");
    FOUND_FFMPEG.read().ok().and_then(|k| k.clone().flatten())
}

/// Every place ffmpeg might be, best first.
fn ffmpeg_candidates(configured: &str) -> Vec<String> {
    let mut candidates = Vec::new();
    let configured = configured.trim();
    if !configured.is_empty() {
        candidates.push(configured.to_string());
    }
    // On PATH, which is the normal case and the cheapest to try.
    candidates.push("ffmpeg".to_string());

    let exe_name = if cfg!(windows) {
        "ffmpeg.exe"
    } else {
        "ffmpeg"
    };
    // Beside snag's own settings, where a portable copy is told to put it.
    candidates.push(
        crate::bootstrap::config_dir()
            .join(exe_name)
            .display()
            .to_string(),
    );
    candidates.push(
        crate::bootstrap::managed_bin_dir()
            .join(exe_name)
            .display()
            .to_string(),
    );

    #[cfg(windows)]
    if let Some(local) = std::env::var_os("LOCALAPPDATA") {
        let winget = std::path::PathBuf::from(local)
            .join("Microsoft")
            .join("WinGet");
        // Packages that publish a shim land here.
        candidates.push(
            winget
                .join("Links")
                .join("ffmpeg.exe")
                .display()
                .to_string(),
        );
        // The ffmpeg winget offers does not: it unpacks a build and puts its
        // own bin folder on PATH, so the binary has to be looked for.
        // <Packages>/<Gyan.FFmpeg...>/<ffmpeg-9.0.1-full_build>/bin/ffmpeg.exe
        if let Ok(packages) = std::fs::read_dir(winget.join("Packages")) {
            for package in packages.flatten() {
                let name = package.file_name().to_string_lossy().to_lowercase();
                if !name.contains("ffmpeg") {
                    continue;
                }
                let direct = package.path().join("bin").join("ffmpeg.exe");
                if direct.is_file() {
                    candidates.push(direct.display().to_string());
                }
                if let Ok(inner) = std::fs::read_dir(package.path()) {
                    for build in inner.flatten() {
                        let path = build.path().join("bin").join("ffmpeg.exe");
                        if path.is_file() {
                            candidates.push(path.display().to_string());
                        }
                    }
                }
            }
        }
    }

    candidates
}

/// Find ffmpeg and ask its version. The path is remembered for `ffmpeg_bin`.
pub fn detect_ffmpeg(configured: &str) -> Option<String> {
    for bin in ffmpeg_candidates(configured) {
        let Ok(out) = util::run_capture(&bin, &["-version"]) else {
            continue;
        };
        let Some(first) = out.lines().next() else {
            continue;
        };
        if let Ok(mut found) = FOUND_FFMPEG.write() {
            *found = Some(Some(bin));
        }
        return Some(ffmpeg_version_number(first));
    }
    if let Ok(mut found) = FOUND_FFMPEG.write() {
        *found = Some(None);
    }
    None
}

#[cfg(test)]
mod tests {
    #[test]
    fn what_is_configured_is_tried_before_anything_else() {
        let list = super::ffmpeg_candidates("  C:/tools/ffmpeg.exe  ");
        assert_eq!(list[0], "C:/tools/ffmpeg.exe", "trimmed, and first");
        assert_eq!(list[1], "ffmpeg", "then whatever is on PATH");
        assert!(list.len() > 2, "and then the places it is usually put");

        // Nothing configured means PATH leads.
        let list = super::ffmpeg_candidates("   ");
        assert_eq!(list[0], "ffmpeg");
    }

    #[test]
    fn ffmpeg_banner_becomes_a_number() {
        use super::ffmpeg_version_number as v;
        assert_eq!(v("ffmpeg version 9.0.1-full_build-www.gyan.dev Copyright (c) 2000-2026 the FFmpeg developers"), "9.0.1");
        assert_eq!(v("ffmpeg version n7.1 Copyright (c) 2000-2024"), "7.1");
        assert_eq!(v("ffmpeg version 6.1.1-3ubuntu5 Copyright"), "6.1.1");
        assert_eq!(
            v("ffmpeg version N-118000-gabcdef Copyright"),
            "N-118000-gabcdef"
        );
    }

    /// The real installer against real github: yt-dlp and then deno into a
    /// scratch folder, both verified by running them. Network, hence ignored.
    #[test]
    #[ignore = "downloads from github"]
    fn installing_ytdlp_brings_deno_with_it() {
        use super::*;
        use std::sync::mpsc::channel;
        let dir = std::env::temp_dir().join("snag-install-test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let (tx, rx) = channel();
        install(dir.clone(), tx, || {});
        let mut done = None;
        let mut log = Vec::new();
        while let Ok(ev) = rx.recv_timeout(std::time::Duration::from_secs(300)) {
            match ev {
                InstallEvent::Log(l) => log.push(l),
                InstallEvent::State(InstallState::Done { .. }) => {
                    done = Some(true);
                    break;
                }
                InstallEvent::State(InstallState::Failed(e)) => panic!("install failed: {e}"),
                _ => {}
            }
        }
        assert_eq!(done, Some(true), "no verdict");
        for l in &log {
            eprintln!("  {l}");
        }

        let deno = dir.join(deno_local_name());
        assert!(deno.is_file(), "deno was not installed alongside");
        let v = deno_version(&deno).unwrap();
        assert!(v.starts_with(char::is_numeric), "odd version: {v}");
        assert!(
            log.iter().any(|l| l.contains("verified:    deno")),
            "{log:?}"
        );

        // Nothing half-done left behind.
        assert!(!dir.join("deno-download.zip").exists());
        assert!(!dir.join("deno-download.part").exists());
        assert!(!dir.join("deno.part").exists());

        let _ = std::fs::remove_dir_all(&dir);
    }

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
