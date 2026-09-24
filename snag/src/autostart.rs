//! Starting with the computer, so a Snag that lives in the tray is there
//! when you copy your first link of the day.
//!
//! On Windows this is a shortcut in the Startup folder, which is the polite
//! way to do it: it is visible in Task Manager's startup list, the user can
//! delete it in Explorer without touching a registry key they cannot find,
//! and it needs no elevation. The shortcut passes `--tray`, so an autostarted
//! Snag goes straight to the tray rather than opening a window at anyone.
//!
//! Elsewhere this does nothing and says so: the tray, which is the whole
//! point of starting at login, is Windows and macOS only, and a macOS launch
//! agent is not something worth writing untested.

/// Whether this build can start itself with the computer.
pub fn supported() -> bool {
    cfg!(windows)
}

/// The command line an autostarted Snag gets.
pub const TRAY_ARG: &str = "--tray";

/// Is Snag set to start at login, pointing at this copy of it?
pub fn enabled() -> bool {
    #[cfg(windows)]
    {
        let Some(path) = imp::link_path() else {
            return false;
        };
        let Ok(exe) = std::env::current_exe() else {
            return false;
        };
        crate::shortcut::target_of(&path).is_some_and(|t| crate::shortcut::same_file(&t, &exe))
    }
    #[cfg(not(windows))]
    false
}

/// Start with the computer, or stop doing so. Returns whether the state
/// asked for is the state now, so a failure can be reported rather than
/// leaving a switch that lies.
pub fn set(on: bool) -> bool {
    #[cfg(windows)]
    {
        let Some(path) = imp::link_path() else {
            return !on;
        };
        if on {
            let Ok(exe) = std::env::current_exe() else {
                return false;
            };
            // Stamped with the app id like every other shortcut to Snag, so
            // an autostarted copy is the same app to the taskbar and to
            // notifications as one started by hand.
            crate::shortcut::create(&path, &exe, TRAY_ARG, Some(crate::notify::APP_ID))
        } else {
            let _ = std::fs::remove_file(&path);
            !path.exists()
        }
    }
    #[cfg(not(windows))]
    {
        let _ = on;
        false
    }
}

#[cfg(windows)]
mod imp {
    use std::path::PathBuf;

    pub fn link_path() -> Option<PathBuf> {
        crate::shortcut::startup_dir().map(|d| d.join("Snag.lnk"))
    }
}
