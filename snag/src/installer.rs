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

/// yt-dlp's unpacked build, where Snag uses it: a folder holding yt-dlp.exe
/// and the `_internal` folder it runs from.
///
/// On Windows the single-file yt-dlp.exe unpacks itself into a temporary
/// folder every time it runs, before doing anything else. Measured with
/// 2026.10.26, `--version` took 3.2 to 3.4 seconds that way and 0.29 to 0.31
/// unpacked, and every link preview and every download paid the difference.
/// The single file is still what the other platforms get.
fn unpacked_asset() -> Option<&'static str> {
    if cfg!(all(windows, target_arch = "x86_64")) {
        Some("yt-dlp_win.zip")
    } else if cfg!(all(windows, target_arch = "aarch64")) {
        Some("yt-dlp_win_arm64.zip")
    } else {
        None
    }
}

/// What Snag downloads to install yt-dlp: the unpacked build where there is
/// one for this platform, the single file otherwise.
fn source_asset() -> &'static str {
    unpacked_asset().unwrap_or(asset_name())
}

fn source_url() -> String {
    format!(
        "https://github.com/yt-dlp/yt-dlp/releases/latest/download/{}",
        source_asset()
    )
}

/// The folder an unpacked yt-dlp keeps beside its exe.
const UNPACKED_DIR: &str = "_internal";

/// Unpack `zip` into a fresh `staging` folder, refusing any entry that would
/// land outside it or is a link.
fn extract_all(zip: &Path, staging: &Path) -> Result<(), String> {
    let _ = std::fs::remove_dir_all(staging);
    let file = std::fs::File::open(zip).map_err(|e| e.to_string())?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| format!("bad zip: {e}"))?;
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(|e| format!("bad zip: {e}"))?;
        let name = entry.name().map(|n| n.into_owned()).unwrap_or_default();
        let Some(relative) = entry.enclosed_name() else {
            return Err(format!("the archive has an unsafe path in it: {name}"));
        };
        if entry.is_symlink() {
            return Err(format!("the archive has a link in it: {name}"));
        }
        let out = staging.join(relative);
        if entry.is_dir() {
            std::fs::create_dir_all(&out).map_err(|e| e.to_string())?;
            continue;
        }
        if let Some(parent) = out.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let mut written = std::fs::File::create(&out).map_err(|e| e.to_string())?;
        std::io::copy(&mut entry, &mut written).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Put an unpacked yt-dlp from `zip` into `dir`, replacing whichever build
/// was there, so that `dir` ends up with the old one or the new one whole.
/// The unpacked yt-dlp is run as `--version` before it replaces anything.
fn install_unpacked(zip: &Path, dir: &Path) -> Result<(), String> {
    let staging = dir.join("yt-dlp.new");
    let result = (|| {
        extract_all(zip, &staging)?;
        let (new_exe, new_internal) = (staging.join(local_name()), staging.join(UNPACKED_DIR));
        if !new_exe.is_file() || !new_internal.is_dir() {
            return Err("the archive is not the yt-dlp build it should be".to_string());
        }
        ytdlp_version_of(&new_exe)?;

        let (exe, internal) = (dir.join(local_name()), dir.join(UNPACKED_DIR));
        let (old_exe, old_internal) = (
            dir.join(format!("{}.old", local_name())),
            dir.join(format!("{UNPACKED_DIR}.old")),
        );
        let _ = std::fs::remove_file(&old_exe);
        let _ = std::fs::remove_dir_all(&old_internal);

        // The old build is moved aside rather than deleted, so a failure
        // part way can put it back. Windows refuses to move it while a
        // download is using it.
        let busy = "yt-dlp is in use: let the running downloads finish, then try again";
        if internal.exists() {
            std::fs::rename(&internal, &old_internal).map_err(|_| busy.to_string())?;
        }
        if exe.exists() && std::fs::rename(&exe, &old_exe).is_err() {
            let _ = std::fs::rename(&old_internal, &internal);
            return Err(busy.to_string());
        }
        let moved = std::fs::rename(&new_internal, &internal)
            .and_then(|()| std::fs::rename(&new_exe, &exe));
        if let Err(e) = moved {
            let _ = std::fs::remove_dir_all(&internal);
            let _ = std::fs::remove_file(&exe);
            let _ = std::fs::rename(&old_internal, &internal);
            let _ = std::fs::rename(&old_exe, &exe);
            return Err(format!("could not move yt-dlp into place: {e}"));
        }
        // Whatever cannot go yet goes on the next install.
        let _ = std::fs::remove_file(&old_exe);
        let _ = std::fs::remove_dir_all(&old_internal);
        Ok(())
    })();
    let _ = std::fs::remove_dir_all(&staging);
    result
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

/// The SHA-256 a project published for `asset`, from the checksum file at
/// `sums_url`.
///
/// yt-dlp publishes one file for every asset, a `hash  name` line each.
/// Deno publishes one file per asset, and on Windows it is PowerShell's
/// `Hash : HASH` layout with no name in it, so a file holding exactly one
/// hash is taken to be this asset's.
fn published_sha256(sums_url: &str, asset: &str) -> Result<String, String> {
    let body = crate::net::get(sums_url)
        .and_then(|r| r.into_string().map_err(|e| e.to_string()))
        .map_err(|e| format!("could not fetch the checksums: {e}"))?;
    sha256_for(&body, asset).ok_or_else(|| format!("the checksums list nothing for {asset}"))
}

pub(crate) fn sha256_for(body: &str, asset: &str) -> Option<String> {
    let is_hash = |w: &str| w.len() == 64 && w.chars().all(|c| c.is_ascii_hexdigit());
    for line in body.lines() {
        let mut words = line.split_whitespace();
        if let (Some(hash), Some(name)) = (words.next(), words.next()) {
            // `*name` is the binary-mode marker some sha256sum output uses.
            if is_hash(hash) && name.trim_start_matches('*') == asset {
                return Some(hash.to_ascii_lowercase());
            }
        }
    }
    let hashes: Vec<&str> = body
        .split(|c: char| !c.is_ascii_hexdigit())
        .filter(|w| is_hash(w))
        .collect();
    match hashes.as_slice() {
        [only] => Some(only.to_ascii_lowercase()),
        _ => None,
    }
}

/// Download `url` to `dest`, refusing it unless its SHA-256 is `expected`.
///
/// What is downloaded here is run straight afterwards, to ask its version,
/// so it is checked against its project's published checksum first.
fn download_to(
    url: &str,
    dest: &Path,
    expected: &str,
    tx: &Sender<InstallEvent>,
) -> Result<(), String> {
    let resp = crate::net::download(url).map_err(|e| format!("could not reach github: {e}"))?;
    let total = crate::net::declared_length(&resp);

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

    let bytes = std::fs::read(&temp).map_err(|e| e.to_string())?;
    if crate::selfupdate::sha256_hex(&bytes) != expected {
        let _ = std::fs::remove_file(&temp);
        return Err(
            "the download did not match its published checksum, so it was discarded".into(),
        );
    }
    let _ = tx.send(InstallEvent::Log(
        "checksum:    matches the published one".into(),
    ));

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

/// Where the yt-dlp being installed comes from.
#[derive(Debug, Clone, PartialEq)]
pub enum Source {
    /// The current release, downloaded from the yt-dlp project on GitHub.
    Github,
    /// A copy the user downloaded themselves (in a browser, when Snag cannot
    /// reach GitHub) and picked: yt-dlp's own binary, or on Windows its
    /// unpacked build's zip.
    File(PathBuf),
}

/// Install yt-dlp into `dir` in the background and report progress.
pub fn install(
    dir: PathBuf,
    source: Source,
    tx: Sender<InstallEvent>,
    repaint: impl Fn() + Send + 'static,
) {
    std::thread::spawn(move || {
        install_now(&dir, &source, &tx, &repaint);
    });
}

/// Install yt-dlp into `dir`, check it runs, and put deno beside it, on the
/// calling thread. The answer arrives as the final `InstallEvent::State`.
pub fn install_now(dir: &Path, source: &Source, tx: &Sender<InstallEvent>, repaint: &dyn Fn()) {
    // A folder this user may not write to (a portable copy in Program Files,
    // a settings folder chosen somewhere protected) is installed into with
    // administrator permission, asked for once for the whole install.
    if !writable(dir) {
        if ELEVATION_ALLOWED.load(std::sync::atomic::Ordering::Relaxed) {
            install_elevated(dir, source, tx, repaint);
        } else {
            let _ = tx.send(InstallEvent::State(InstallState::Failed(format!(
                "cannot write to {}. run this as administrator, or choose another folder.",
                dir.display()
            ))));
            repaint();
        }
        return;
    }
    let result = match source {
        Source::Github => from_github(dir, tx, repaint),
        Source::File(file) => from_file(dir, file, tx, repaint),
    };
    if let Err(e) = result {
        let _ = tx.send(InstallEvent::State(InstallState::Failed(e)));
    }
    repaint();
}

/// yt-dlp's latest release, for downloading it by hand in a browser.
pub const RELEASES_PAGE: &str = "https://github.com/yt-dlp/yt-dlp/releases/latest";

const SUMS_URL: &str = "https://github.com/yt-dlp/yt-dlp/releases/latest/download/SHA2-256SUMS";

/// What to tell someone whose network does not let Snag reach GitHub. The
/// technical reason goes in the log beneath it.
fn unreachable_message() -> String {
    if crate::net::settings_proxy().is_empty() {
        "snag could not reach github.com, where yt-dlp is published. a firewall, a network filter or no internet connection is likely in the way. your browser may still get through.".into()
    } else {
        "snag could not reach github.com through the proxy in settings > network. check the proxy there.".into()
    }
}

fn from_github(dir: &Path, tx: &Sender<InstallEvent>, repaint: &dyn Fn()) -> Result<(), String> {
    let dest = dir.join(local_name());
    let _ = tx.send(InstallEvent::Log(format!("source:      {}", source_url())));
    let _ = tx.send(InstallEvent::Log(format!("destination: {}", dest.display())));
    let _ = tx.send(InstallEvent::State(InstallState::Downloading { got: 0, total: 0 }));
    repaint();

    // The checksum list is the first thing fetched, so it is also what finds
    // out whether GitHub can be reached at all. Directly first (or through the
    // proxy in settings, which is never gone around). When that fails and
    // settings name no proxy, through each proxy the system is set to use: on
    // a network that lets nothing out except through its proxy, the browser
    // works and Snag, knowing nothing of the proxy, did not.
    let asset = source_asset();
    let (expected, route) = match published_sha256(SUMS_URL, asset) {
        Ok(hash) => (hash, None),
        Err(first) if first.starts_with("could not fetch") => {
            let _ = tx.send(InstallEvent::Log(format!("direct:      {first}")));
            let mut reached = None;
            if crate::net::settings_proxy().is_empty() {
                for proxy in crate::net::system_proxies() {
                    match crate::net::through(&proxy, || published_sha256(SUMS_URL, asset)) {
                        Ok(hash) => {
                            let _ = tx.send(InstallEvent::Log(format!(
                                "proxy:       reached github through the system proxy {proxy}"
                            )));
                            reached = Some((hash, Some(proxy)));
                            break;
                        }
                        Err(e) => {
                            let _ = tx.send(InstallEvent::Log(format!("proxy:       {proxy}: {e}")));
                        }
                    }
                }
            }
            reached.ok_or_else(unreachable_message)?
        }
        Err(other) => return Err(other),
    };

    let rest = || -> Result<(), String> {
        if unpacked_asset().is_none() {
            download_to(&source_url(), &dest, &expected, tx)?;
        } else {
            let zip = dir.join("yt-dlp-download.zip");
            let unpacked = download_to(&source_url(), &zip, &expected, tx)
                .and_then(|()| install_unpacked(&zip, dir));
            let _ = std::fs::remove_file(&zip);
            unpacked?;
        }
        finish(dir, dest.clone(), tx, repaint)
    };
    match route {
        Some(proxy) => crate::net::through(&proxy, rest),
        None => rest(),
    }
}

/// Install a yt-dlp the user downloaded themselves. It cannot be checked
/// against GitHub's checksum (GitHub being what could not be reached), so it
/// has to show it is yt-dlp by running as one before it replaces anything.
fn from_file(
    dir: &Path,
    file: &Path,
    tx: &Sender<InstallEvent>,
    repaint: &dyn Fn(),
) -> Result<(), String> {
    let dest = dir.join(local_name());
    let _ = tx.send(InstallEvent::Log(format!("source:      {} (picked by you)", file.display())));
    let _ = tx.send(InstallEvent::Log(format!("destination: {}", dest.display())));
    let _ = tx.send(InstallEvent::State(InstallState::Verifying));
    repaint();

    let is_zip = file
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("zip"));
    if is_zip {
        if unpacked_asset().is_none() {
            return Err(format!(
                "that is a zip. pick {}, yt-dlp's single-file build for this system.",
                asset_name()
            ));
        }
        install_unpacked(file, dir)?;
    } else {
        // Tried out under a name of its own beside the real one, then renamed
        // over it, so a file that turns out not to be yt-dlp replaces nothing.
        let trial = dir.join(if cfg!(windows) { "yt-dlp-check.exe" } else { "yt-dlp-check" });
        std::fs::copy(file, &trial).map_err(|e| format!("could not copy {}: {e}", file.display()))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&trial, std::fs::Permissions::from_mode(0o755));
        }
        if let Err(e) = ytdlp_version_of(&trial) {
            let _ = std::fs::remove_file(&trial);
            return Err(e);
        }
        let _ = std::fs::remove_file(&dest);
        std::fs::rename(&trial, &dest).map_err(|e| {
            let _ = std::fs::remove_file(&trial);
            format!("could not move yt-dlp into place: {e}")
        })?;
    }
    let _ = tx.send(InstallEvent::Log(
        "checksum:    not checked, since github could not be reached. snag ran it to make sure it is yt-dlp.".into(),
    ));
    finish(dir, dest, tx, repaint)
}

/// The version an executable reports, provided it reports one the way yt-dlp
/// does (a date, 2026.10.26), so that a wrong file is caught before use.
fn ytdlp_version_of(exe: &Path) -> Result<String, String> {
    let out = util::run_capture(&exe.display().to_string(), &["--version"])
        .map_err(|e| {
            let short: String = e.trim().chars().take(120).collect();
            if short.is_empty() {
                "that file is not yt-dlp, or it would not run".to_string()
            } else {
                format!("that file is not yt-dlp, or it would not run ({short})")
            }
        })?;
    let version = out.trim().to_string();
    if looks_like_ytdlp_version(&version) {
        Ok(version)
    } else {
        let short: String = version.chars().take(60).collect();
        Err(format!("that file is not yt-dlp (it says its version is \"{short}\")"))
    }
}

fn looks_like_ytdlp_version(v: &str) -> bool {
    let first = v.split_whitespace().next().unwrap_or("");
    let parts: Vec<&str> = first.split('.').collect();
    parts.len() >= 3
        && parts[0].len() == 4
        && parts.iter().all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()))
}

/// The last steps of every install: yt-dlp runs and says its version, and
/// deno comes along beside it.
fn finish(
    dir: &Path,
    dest: PathBuf,
    tx: &Sender<InstallEvent>,
    repaint: &dyn Fn(),
) -> Result<(), String> {
    let _ = tx.send(InstallEvent::State(InstallState::Verifying));
    repaint();

    // A binary that cannot report its own version is not one we should use.
    let version = match util::run_capture(&dest.display().to_string(), &["--version"]) {
        Ok(v) => v.trim().to_string(),
        Err(e) => {
            // ENOENT here almost never means the file is missing: it means
            // the kernel could not exec it. Say something useful.
            let hint = if cfg!(unix) && e.contains("No such file or directory") {
                " (the file is there, but it would not execute. this build should be self-contained, so it may have downloaded incompletely: try again.)"
            } else {
                ""
            };
            return Err(format!("downloaded, but it would not run: {e}{hint}"));
        }
    };
    let _ = tx.send(InstallEvent::Log(format!("verified:    yt-dlp {version}")));

    // yt-dlp wants a javascript runtime for youtube and has deprecated
    // working without one, so it comes along with yt-dlp rather than being a
    // second thing to know about. Not having it is not yet fatal, so a
    // failure here is logged and yt-dlp still counts as installed.
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
    let _ = tx.send(InstallEvent::State(InstallState::Done { path: dest, version }));
    Ok(())
}

/// Whether an install into a protected folder may ask for administrator
/// permission. The window may; `--install-ytdlp` may not, since it is what the
/// window runs with that permission, and asking again from there would only
/// ask again.
pub static ELEVATION_ALLOWED: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(true);

/// Whether this process can put files in `dir`, creating it if need be.
/// Asked by trying, since permissions on Windows are too tangled to read.
pub fn writable(dir: &Path) -> bool {
    if std::fs::create_dir_all(dir).is_err() {
        return false;
    }
    let probe = dir.join(format!(".snag-write-test-{}", std::process::id()));
    let ok = std::fs::write(&probe, b"").is_ok();
    let _ = std::fs::remove_file(&probe);
    ok
}

/// Install into a folder that needs administrator permission: Snag runs
/// itself as `--install-ytdlp <dir>` with that permission, which Windows asks
/// the user for once, and waits for it. Nowhere else does a desktop app get to
/// ask, so elsewhere it says what is wrong instead.
fn install_elevated(dir: &Path, source: &Source, tx: &Sender<InstallEvent>, repaint: &dyn Fn()) {
    let fail = |e: String| {
        let _ = tx.send(InstallEvent::State(InstallState::Failed(e)));
        repaint();
    };
    let _ = tx.send(InstallEvent::Log(format!(
        "destination: {} (needs administrator permission, so windows asks once)",
        dir.display()
    )));
    let _ = tx.send(InstallEvent::State(InstallState::Downloading {
        got: 0,
        total: 0,
    }));
    repaint();

    #[cfg(windows)]
    {
        let exe = match std::env::current_exe() {
            Ok(exe) => exe,
            Err(e) => {
                return fail(format!(
                    "cannot find snag itself to ask for permission: {e}"
                ))
            }
        };
        let mut args = format!("--install-ytdlp \"{}\"", dir.display());
        if let Source::File(file) = source {
            args.push_str(&format!(" --from \"{}\"", file.display()));
        }
        match util::run_elevated(&exe, &args) {
            Ok(0) => {}
            Ok(code) => {
                return fail(format!(
                    "the install with administrator permission failed (exit code {code}). try again, or choose a folder you can write to."
                ))
            }
            Err(e) => return fail(e),
        }
        let _ = tx.send(InstallEvent::State(InstallState::Verifying));
        repaint();
        let dest = dir.join(local_name());
        match util::run_capture(&dest.display().to_string(), &["--version"]) {
            Ok(version) => {
                let version = version.trim().to_string();
                let _ = tx.send(InstallEvent::Log(format!("verified:    yt-dlp {version}")));
                let _ = tx.send(InstallEvent::State(InstallState::Done {
                    path: dest,
                    version,
                }));
                repaint();
            }
            Err(e) => fail(format!("installed, but it would not run: {e}")),
        }
    }
    #[cfg(not(windows))]
    {
        let _ = source;
        fail(format!(
            "snag cannot write to {}. choose a folder you own in setup, or fix its permissions.",
            dir.display()
        ));
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
    let expected = published_sha256(
        &format!("{}.sha256sum", deno_download_url()),
        deno_asset_name(),
    )?;
    download_to(&deno_download_url(), &zip_path, &expected, tx)?;

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
    use super::sha256_for;

    #[test]
    fn what_counts_as_a_ytdlp_version() {
        use super::looks_like_ytdlp_version as v;
        assert!(v("2026.10.26"));
        assert!(v("2026.10.26.233012")); // nightly builds
        assert!(!v("Python 3.13.1"));
        assert!(!v("deno 2.5.0"));
        assert!(!v("26.10.26"));
        assert!(!v(""));
    }

    /// A file picked by hand that is not yt-dlp is refused, and whatever
    /// yt-dlp was there before is left as it was.
    #[test]
    fn a_wrong_file_replaces_nothing() {
        let dir = std::env::temp_dir().join(format!("snag-from-file-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let existing = dir.join(super::local_name());
        std::fs::write(&existing, b"the old one").unwrap();

        // Something that runs, but says it is something else.
        let picked = std::env::current_exe().unwrap();
        let (tx, rx) = std::sync::mpsc::channel();
        super::install_now(&dir, &super::Source::File(picked), &tx, &|| {});
        drop(tx);
        let last = rx
            .iter()
            .filter_map(|e| match e {
                super::InstallEvent::State(s) => Some(s),
                _ => None,
            })
            .last();
        assert!(
            matches!(&last, Some(super::InstallState::Failed(e)) if e.contains("not yt-dlp")),
            "{last:?}"
        );
        assert_eq!(std::fs::read(&existing).unwrap(), b"the old one");
        let left: Vec<_> = std::fs::read_dir(&dir).unwrap().flatten().map(|e| e.file_name()).collect();
        assert_eq!(left.len(), 1, "{left:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn finds_the_published_hash_in_each_checksum_layout() {
        let ytdlp = "e51e69e89489f199530f13d32a4342da3ddb2cf34842f364603a066db05afbf3  yt-dlp\n\
                     f5389d3698ecde4ef06f4e6b1ce0294fe9ac1396228c8990b8d3416d6d177a7d  yt-dlp.exe\n";
        assert_eq!(
            sha256_for(ytdlp, "yt-dlp.exe").as_deref(),
            Some("f5389d3698ecde4ef06f4e6b1ce0294fe9ac1396228c8990b8d3416d6d177a7d")
        );
        assert_eq!(
            sha256_for(ytdlp, "yt-dlp_linux"),
            None,
            "never another asset's"
        );

        // Deno on Windows: PowerShell's Get-FileHash output, no name in it.
        let powershell = "\r\nAlgorithm : SHA256\r\n\
                          Hash      : CBA84B26E0DF5AE7F74FC66B86A95B866872A86292EBD8C1D9C3D4E5F6A7B8C9\r\n\
                          Path      : C:\\a\\deno.zip\r\n";
        assert_eq!(
            sha256_for(powershell, "deno-x86_64-pc-windows-msvc.zip").as_deref(),
            Some("cba84b26e0df5ae7f74fc66b86a95b866872a86292ebd8c1d9c3d4e5f6a7b8c9")
        );

        // Deno on Linux: one sha256sum line.
        let unix = "e890975bc2549807bbcc0b584ffafda72cc7c93349dd99c80483dd784f6c4609  deno-x86_64-unknown-linux-gnu.zip\n";
        assert!(sha256_for(unix, "deno-x86_64-unknown-linux-gnu.zip").is_some());
    }

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
        install(dir.clone(), Source::Github, tx, || {});
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
