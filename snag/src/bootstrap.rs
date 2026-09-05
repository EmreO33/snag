//! Where Snag keeps its own files.
//!
//! The settings file's location is itself configurable, which is a small
//! chicken-and-egg problem: we cannot read the choice out of the settings we
//! have not located yet. So a one-line pointer file lives at the platform
//! default location and names the directory the user actually picked. No
//! pointer means the default is in use.

use std::path::{Path, PathBuf};
use std::sync::RwLock;

static CONFIG_DIR: RwLock<Option<PathBuf>> = RwLock::new(None);

const POINTER_FILE: &str = "config-location.txt";

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
    let resolved = read_pointer().unwrap_or_else(platform_config_dir);
    if let Ok(mut guard) = CONFIG_DIR.write() {
        *guard = Some(resolved.clone());
    }
    resolved
}

/// Point Snag at a different config directory from now on.
pub fn set_config_dir(dir: &Path) -> Result<(), String> {
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
