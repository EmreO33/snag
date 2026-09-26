//! Desktop notifications, and the introduction Windows demands before it
//! will show any.
//!
//! On Linux and macOS a notification is a message to the desktop and the
//! desktop shows it. Windows is different: a toast is attributed to an app
//! id, and a toast from an id Windows has never heard of is accepted, filed
//! in the notification centre's database, and never drawn. The call reports
//! success, so from inside the app nothing looks wrong, which is how this
//! feature shipped broken and stayed that way.
//!
//! What Windows accepts as an introduction was found by trying, not by
//! reading, because the documentation offers a registry key that this
//! Windows 11 ignores: the thing that works is a Start Menu shortcut with the
//! app id stamped into its property store, which is what Discord, Chrome and
//! every Electron app keep for themselves. Snag does the same. The installer
//! stamps the shortcuts it makes; for a Scoop copy the shortcut Scoop made is
//! stamped in place; a portable copy gets one of its own, which turning
//! notifications off removes again.
//!
//! One more thing found the hard way: a toast is dropped if the process that
//! sent it exits before it is drawn, so `--notify-test` waits around.

/// The id Windows knows Snag by. The installer stamps it onto the shortcuts
/// it creates, and the process claims it at startup, so a pinned taskbar
/// button, the running window and the toast all agree on who they are.
#[cfg(windows)]
pub const APP_ID: &str = "EmreO33.Snag";

/// Show a notification. Returns whether the desktop accepted it, which on
/// Windows means only that it was handed over: whether it is then shown is
/// the introduction's doing.
///
/// `sound` is for the things worth looking up from something else for: a
/// download that finished while you were away. A copied link is not one of
/// them, since you just did the copying.
pub fn send(title: &str, body: &str, sound: bool) -> bool {
    let mut notification = notify_rust::Notification::new();
    notification.summary(title).body(body).appname("Snag");
    if sound {
        // The desktop's own default sound, whatever it is set to.
        notification.sound_name("Default");
    }

    #[cfg(windows)]
    notification.app_id(APP_ID);

    notification.show().is_ok()
}

/// Make this process one that Windows will show notifications for.
///
/// Idempotent and cheap, so it is done on every launch: a shortcut someone
/// deleted comes back, and one that points at where Snag used to be is
/// pointed at where it is now. Elsewhere it does nothing.
pub fn register() {
    #[cfg(windows)]
    imp::register();
}

/// Take the introduction back out, for someone who has turned notifications
/// off and would rather not leave a trace of them. Only what Snag made for
/// itself is removed; a shortcut the installer or Scoop created is theirs.
/// Elsewhere it does nothing.
pub fn unregister() {
    #[cfg(windows)]
    imp::unregister();
}

#[cfg(windows)]
mod imp {
    use super::APP_ID;
    use crate::shortcut;
    use std::path::PathBuf;

    #[link(name = "shell32")]
    extern "system" {
        fn SetCurrentProcessExplicitAppUserModelID(id: *const u16) -> i32;
    }

    #[link(name = "advapi32")]
    extern "system" {
        fn RegDeleteTreeW(key: isize, sub_key: *const u16) -> i32;
    }

    const HKEY_CURRENT_USER: isize = 0x80000001u32 as i32 as isize;

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    /// The shortcut Snag makes when nobody else has.
    fn own_shortcut() -> Option<PathBuf> {
        shortcut::programs_dir().map(|d| d.join("Snag.lnk"))
    }

    pub fn register() {
        // The process side, so the taskbar groups this window under the
        // same id the shortcut and the toast use. Harmless if it fails.
        unsafe { SetCurrentProcessExplicitAppUserModelID(wide(APP_ID).as_ptr()) };

        let Ok(exe) = std::env::current_exe() else {
            return;
        };
        let Some(programs) = shortcut::programs_dir() else {
            return;
        };

        // A shortcut that already points at this Snag, from the installer,
        // Scoop, or a person, is stamped where it is.
        let mut links = Vec::new();
        shortcut::shortcuts_under(&programs, 2, &mut links);
        let stamped = links.iter().any(|link| {
            shortcut::target_of(link).is_some_and(|t| shortcut::same_file(&t, &exe))
                && shortcut::stamp_app_id(link, APP_ID)
        });

        // A machine-wide install's shortcut is in the all-users Start Menu,
        // already stamped by the installer, and not ours to write to without
        // administrator. It is enough by itself, so a shortcut of Snag's own
        // beside it would only be a second "Snag" in Start. Earlier versions
        // made that second one, so it is taken away again here.
        let installed = shortcut::common_programs_dir().is_some_and(|common| {
            let mut links = Vec::new();
            shortcut::shortcuts_under(&common, 2, &mut links);
            links.iter().any(|link| {
                shortcut::target_of(link).is_some_and(|t| shortcut::same_file(&t, &exe))
            })
        });
        if installed {
            if let Some(own) = own_shortcut() {
                if shortcut::target_of(&own).is_some_and(|t| shortcut::same_file(&t, &exe)) {
                    let _ = std::fs::remove_file(own);
                }
            }
            return;
        }

        // Nobody made one: a portable copy, or a bare exe someone put
        // somewhere. Snag makes its own, and keeps it pointed at itself.
        if !stamped {
            if let Some(own) = own_shortcut() {
                shortcut::create(&own, &exe, "", Some(APP_ID));
            }
        }
    }

    pub fn unregister() {
        // Only the shortcut Snag made for itself; anything else in the
        // Start Menu belongs to whoever put it there. The registry key and
        // icon file an earlier version wrote are cleared too, since they
        // never did anything.
        if let Some(own) = own_shortcut() {
            let _ = std::fs::remove_file(own);
        }
        unsafe {
            RegDeleteTreeW(
                HKEY_CURRENT_USER,
                wide(&format!("Software\\Classes\\AppUserModelId\\{APP_ID}")).as_ptr(),
            );
        }
        let _ = std::fs::remove_file(crate::bootstrap::config_dir().join("snag-icon.png"));
    }
}
