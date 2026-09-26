//! Starting with the computer, so a Snag that lives in the tray is there
//! when you copy your first link of the day.
//!
//! On Windows this is a shortcut in the Startup folder, which is the polite
//! way to do it: it is visible in Task Manager's startup list, the user can
//! delete it in Explorer without touching a registry key they cannot find,
//! and it needs no elevation. The shortcut passes `--tray`, so an autostarted
//! Snag goes straight to the tray rather than opening a window at anyone.
//!
//! On Linux it is the freedesktop equivalent, a desktop entry in
//! `~/.config/autostart`, which every desktop reads at login and most list in
//! their startup settings. The two sandboxes do it their own way: the snap
//! writes the entry where snapd looks for it, and the Flatpak asks the
//! desktop's Background portal, which writes it outside the sandbox.
//!
//! macOS is left out: a launch agent is not something worth writing
//! untested.

/// Whether this build can start itself with the computer.
pub fn supported() -> bool {
    cfg!(any(windows, target_os = "linux"))
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
    #[cfg(target_os = "linux")]
    {
        imp::enabled()
    }
    #[cfg(not(any(windows, target_os = "linux")))]
    false
}

/// Start with the computer, or stop doing so. Returns whether the state
/// asked for is the state now, so a failure can be reported rather than
/// leaving a switch that lies.
///
/// Inside the Flatpak the desktop decides, and may ask the user first, so
/// the question goes off on a thread and this returns true; a refusal turns
/// up later, from `take_refusal`, and `wake` is called to have it noticed.
pub fn set(on: bool, wake: impl Fn() + Send + 'static) -> bool {
    #[cfg(not(target_os = "linux"))]
    let _ = wake;
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
    #[cfg(target_os = "linux")]
    {
        imp::set(on, wake)
    }
    #[cfg(not(any(windows, target_os = "linux")))]
    {
        let _ = on;
        false
    }
}

/// If the Flatpak's last request to change autostart did not happen, and
/// this has not been asked since: whether Snag still starts at login (the
/// opposite of what was asked), and why. Always None elsewhere, where `set`
/// knows the answer straight away.
pub fn take_refusal() -> Option<(bool, &'static str)> {
    #[cfg(target_os = "linux")]
    {
        use std::sync::atomic::Ordering::Relaxed;
        let why = match imp::OUTCOME.swap(imp::NOTHING, Relaxed) {
            imp::REFUSED => "the desktop would not let snag start at login",
            imp::UNAVAILABLE => "this desktop has no way for the flatpak to start at login",
            _ => return None,
        };
        Some((!imp::ASKED_ON.load(Relaxed), why))
    }
    #[cfg(not(target_os = "linux"))]
    None
}

#[cfg(windows)]
mod imp {
    use std::path::PathBuf;

    pub fn link_path() -> Option<PathBuf> {
        crate::shortcut::startup_dir().map(|d| d.join("Snag.lnk"))
    }
}

#[cfg(target_os = "linux")]
mod imp {
    use std::env::var_os;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};

    /// Named after the app id, as the Background portal names the one it
    /// writes, and as snapcraft.yaml's `autostart` expects.
    const FILE: &str = "io.github.EmreO33.Snag.desktop";

    /// How the last portal request went, when it did not, for the app to
    /// put its switch back and say why.
    pub static OUTCOME: AtomicU8 = AtomicU8::new(NOTHING);
    pub const NOTHING: u8 = 0;
    /// The desktop, or the user, said no.
    pub const REFUSED: u8 = 1;
    /// No Background portal to ask: a desktop whose portal backend (GTK's
    /// alone, say) does not offer one.
    pub const UNAVAILABLE: u8 = 2;
    /// What the last request asked for.
    pub static ASKED_ON: AtomicBool = AtomicBool::new(false);

    fn in_flatpak() -> bool {
        var_os("FLATPAK_ID").is_some() || std::path::Path::new("/.flatpak-info").exists()
    }

    /// Where the entry goes. Inside the snap that is the snap's own home,
    /// where snapd looks at login for the file snapcraft.yaml names. Inside
    /// the Flatpak it is the sandbox's own config folder, which nothing
    /// reads: the file there is only Snag's record of what the portal was
    /// asked, since the real entry outside is out of the sandbox's sight.
    fn entry_path() -> Option<PathBuf> {
        if let Some(data) = var_os("SNAP_USER_DATA") {
            return Some(PathBuf::from(data).join(".config/autostart").join(FILE));
        }
        let config = var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
            .or_else(|| var_os("HOME").map(|home| PathBuf::from(home).join(".config")))?;
        Some(config.join("autostart").join(FILE))
    }

    /// The entry's command line.
    fn exec() -> Option<String> {
        // snapd runs the snap's own launcher in place of whatever comes
        // first, and the portal writes its own `flatpak run` line, so both
        // only need the name. Anything else runs this very file: the
        // AppImage itself rather than the copy mounted from it, which is
        // somewhere different every time.
        let program = if var_os("SNAP").is_some() || in_flatpak() {
            "snag".to_string()
        } else {
            let exe = var_os("APPIMAGE")
                .map(PathBuf::from)
                .or_else(|| std::env::current_exe().ok())?;
            quote(exe.to_str()?)
        };
        Some(format!("{program} {}", super::TRAY_ARG))
    }

    fn entry() -> Option<String> {
        Some(format!(
            "[Desktop Entry]\n\
             Type=Application\n\
             Name=Snag\n\
             Comment=Starts Snag in the tray\n\
             Exec={}\n\
             Icon=io.github.EmreO33.Snag\n\
             Terminal=false\n\
             X-GNOME-Autostart-enabled=true\n",
            exec()?
        ))
    }

    pub fn enabled() -> bool {
        let (Some(path), Some(exec)) = (entry_path(), exec()) else {
            return false;
        };
        let wanted = format!("Exec={exec}");
        std::fs::read_to_string(path).is_ok_and(|text| text.lines().any(|l| l.trim() == wanted))
    }

    /// Write the entry, or take it away.
    fn record(on: bool) -> bool {
        let Some(path) = entry_path() else {
            return !on;
        };
        if on {
            let Some(text) = entry() else {
                return false;
            };
            path.parent()
                .is_some_and(|dir| std::fs::create_dir_all(dir).is_ok())
                && std::fs::write(&path, text).is_ok()
        } else {
            let _ = std::fs::remove_file(&path);
            !path.exists()
        }
    }

    pub fn set(on: bool, wake: impl Fn() + Send + 'static) -> bool {
        if !in_flatpak() {
            return record(on);
        }
        ASKED_ON.store(on, Ordering::Relaxed);
        std::thread::spawn(move || {
            let outcome = match portal::request(on) {
                Ok(true) => {
                    record(on);
                    return;
                }
                Ok(false) => REFUSED,
                Err(_) => UNAVAILABLE,
            };
            OUTCOME.store(outcome, Ordering::Relaxed);
            wake();
        });
        true
    }

    /// Quoted as the desktop entry specification asks, when it has to be.
    fn quote(arg: &str) -> String {
        let reserved = |c: char| c.is_whitespace() || "\"'\\><~|&;$*?#()`".contains(c);
        let arg = arg.replace('%', "%%");
        if !arg.contains(reserved) {
            return arg;
        }
        let mut out = String::from("\"");
        for c in arg.chars() {
            if matches!(c, '"' | '`' | '$' | '\\') {
                out.push('\\');
            }
            out.push(c);
        }
        out.push('"');
        out
    }

    mod portal {
        use std::collections::HashMap;
        use zbus::zvariant::{OwnedValue, Value};

        const DESKTOP: &str = "org.freedesktop.portal.Desktop";

        /// Ask the Background portal to start Snag at login, or to stop.
        /// Blocks until the desktop answers, which can mean until the user
        /// has.
        pub fn request(on: bool) -> zbus::Result<bool> {
            let conn = zbus::blocking::Connection::session()?;
            // The portal answers on an object named after this connection
            // and a token of Snag's choosing, and only once: listen there
            // before asking, or the answer can come and go unheard.
            let token = format!("snag_autostart_{}", std::process::id());
            let sender = conn
                .unique_name()
                .map(|name| name.trim_start_matches(':').replace('.', "_"))
                .unwrap_or_default();
            let path = format!("/org/freedesktop/portal/desktop/request/{sender}/{token}");
            let request: zbus::blocking::Proxy = zbus::blocking::proxy::Builder::new(&conn)
                .destination(DESKTOP)?
                .path(path)?
                .interface("org.freedesktop.portal.Request")?
                .cache_properties(zbus::proxy::CacheProperties::No)
                .build()?;
            let mut answers = request.receive_signal("Response")?;

            let options: HashMap<&str, Value> = HashMap::from([
                ("handle_token", Value::from(token.as_str())),
                (
                    "reason",
                    Value::from("Start Snag in the tray when you log in"),
                ),
                ("autostart", Value::from(on)),
                (
                    "commandline",
                    Value::from(vec!["snag", super::super::TRAY_ARG]),
                ),
            ]);
            conn.call_method(
                Some(DESKTOP),
                "/org/freedesktop/portal/desktop",
                Some("org.freedesktop.portal.Background"),
                "RequestBackground",
                &("", options),
            )?;

            let Some(answer) = answers.next() else {
                return Ok(false);
            };
            let (code, results): (u32, HashMap<String, OwnedValue>) =
                answer.body().deserialize()?;
            let autostart = results
                .get("autostart")
                .and_then(|v| bool::try_from(v).ok());
            Ok(code == 0 && autostart == Some(on))
        }
    }

    #[cfg(test)]
    mod tests {
        use super::quote;

        #[test]
        fn quotes_only_what_needs_it() {
            assert_eq!(quote("/usr/bin/snag"), "/usr/bin/snag");
            assert_eq!(
                quote("/home/a b/Snag.AppImage"),
                "\"/home/a b/Snag.AppImage\""
            );
            assert_eq!(quote("/opt/$x/100%"), "\"/opt/\\$x/100%%\"");
        }
    }
}
