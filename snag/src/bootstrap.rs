//! Where Snag keeps its own files.
//!
//! The settings file's location is itself configurable, which is a small
//! chicken-and-egg problem: we cannot read the choice out of the settings we
//! have not located yet. So a one-line pointer file lives at the platform
//! default location and names the directory the user actually picked. No
//! pointer means the default is in use.
//!
//! A portable build overrides all of that: it keeps everything in a `data`
//! folder beside the executable and touches nothing else on the machine.

use std::path::{Path, PathBuf};
use std::sync::RwLock;

static CONFIG_DIR: RwLock<Option<PathBuf>> = RwLock::new(None);

const POINTER_FILE: &str = "config-location.txt";

/// Dropping this file next to the executable turns the build portable. The
/// portable zip ships one; the installer deliberately does not.
const PORTABLE_MARKER: &str = "portable.txt";

/// The folder the running executable sits in.
pub fn exe_dir() -> Option<PathBuf> {
    std::env::current_exe()
        .ok()?
        .parent()
        .map(|p| p.to_path_buf())
}

/// True when this copy of Snag should keep everything beside the executable,
/// either because the marker file is present or `--portable` was passed.
pub fn is_portable() -> bool {
    if std::env::args().any(|a| a == "--portable") {
        return true;
    }
    exe_dir()
        .map(|d| d.join(PORTABLE_MARKER).is_file())
        .unwrap_or(false)
}

/// Where a portable copy keeps its settings: `<exe dir>/data`.
pub fn portable_data_dir() -> Option<PathBuf> {
    exe_dir().map(|d| d.join("data"))
}

/// The OS-appropriate config directory: `%APPDATA%\Snag` on Windows.
pub fn platform_config_dir() -> PathBuf {
    directories::ProjectDirs::from("", "", "Snag")
        .map(|d| d.config_dir().to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."))
}

fn pointer_path() -> PathBuf {
    platform_config_dir().join(POINTER_FILE)
}

/// Read the pointer file, ignoring it if it names somewhere unusable.
fn read_pointer() -> Option<PathBuf> {
    let raw = std::fs::read_to_string(pointer_path()).ok()?;
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    let path = PathBuf::from(trimmed);
    path.is_dir().then_some(path)
}

/// The directory Snag reads and writes its settings in, resolved once per run.
pub fn config_dir() -> PathBuf {
    if let Some(cached) = CONFIG_DIR.read().ok().and_then(|g| g.clone()) {
        return cached;
    }
    // A portable copy is fixed to its own folder: no pointer, nothing in AppData.
    let resolved = match is_portable().then(portable_data_dir).flatten() {
        Some(dir) => dir,
        None => read_pointer().unwrap_or_else(platform_config_dir),
    };
    if let Ok(mut guard) = CONFIG_DIR.write() {
        *guard = Some(resolved.clone());
    }
    resolved
}

/// Point Snag at a different config directory from now on.
pub fn set_config_dir(dir: &Path) -> Result<(), String> {
    if is_portable() {
        // Writing a pointer into AppData would defeat the whole point.
        return Err("this is a portable copy, so its settings stay beside the executable".into());
    }
    std::fs::create_dir_all(dir).map_err(|e| format!("could not create {}: {e}", dir.display()))?;

    let default = platform_config_dir();
    if dir == default {
        // Back to the default: drop the pointer rather than pointing at ourselves.
        let _ = std::fs::remove_file(pointer_path());
    } else {
        std::fs::create_dir_all(&default)
            .map_err(|e| format!("could not create {}: {e}", default.display()))?;
        std::fs::write(pointer_path(), dir.display().to_string())
            .map_err(|e| format!("could not record the config location: {e}"))?;
    }

    if let Ok(mut guard) = CONFIG_DIR.write() {
        *guard = Some(dir.to_path_buf());
    }
    Ok(())
}

/// Where a Snag-managed yt-dlp binary is kept.
pub fn managed_bin_dir() -> PathBuf {
    config_dir().join("bin")
}
