//! Desktop notifications, and the registration Windows demands before it
//! will show any.
//!
//! On Linux and macOS a notification is a message to the desktop and the
//! desktop shows it. Windows is different: a toast is attributed to an app
//! id, and a toast from an id Windows has never heard of is accepted, logged,
//! and never drawn. The call reports success, so from inside the app nothing
//! looks wrong, which is how this feature shipped broken and stayed that way.
//!
//! An app installed from the Store is registered by the Store. Everything
//! else registers itself, either by stamping the id onto a Start Menu
//! shortcut or by writing one key under the user's own registry hive, which
//! is what Spotify, Proton VPN and the rest do. Snag does the latter at
//! startup, since a portable copy, a Scoop copy and a winget copy have no
//! shortcut of Snag's making to stamp.

/// The id Windows knows Snag by. The installer stamps it onto the shortcuts
/// it creates, and the process claims it at startup, so a pinned taskbar
/// button, the running window and the toast all agree on who they are.
pub const APP_ID: &str = "EmreO33.Snag";

/// Show a notification. Returns whether the desktop accepted it, which on
/// Windows means only that it was handed over: whether it is then shown is
/// the registration's doing.
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
/// Idempotent and cheap, so it is done on every launch: a key someone
/// cleaned out of the registry comes back, and an icon moved with the config
/// directory is written again where the key now points. Elsewhere it does
/// nothing.
pub fn register() {
    #[cfg(windows)]
    imp::register();
}

/// Take the registration back out, for someone who has turned notifications
/// off and would rather not leave a trace of them. Elsewhere it does nothing.
pub fn unregister() {
    #[cfg(windows)]
    imp::unregister();
}

#[cfg(windows)]
mod imp {
    use super::APP_ID;
    use std::ffi::c_void;

    const HKEY_CURRENT_USER: isize = 0x80000001u32 as i32 as isize;
    const KEY_WRITE: u32 = 0x20006;
    const REG_SZ: u32 = 1;
    const REG_OPTION_NON_VOLATILE: u32 = 0;
    const ERROR_SUCCESS: i32 = 0;

    #[link(name = "advapi32")]
    extern "system" {
        fn RegCreateKeyExW(
            key: isize,
            sub_key: *const u16,
            reserved: u32,
            class: *const u16,
            options: u32,
            desired: u32,
            security: *const c_void,
            result: *mut isize,
            disposition: *mut u32,
        ) -> i32;
        fn RegSetValueExW(
            key: isize,
            name: *const u16,
            reserved: u32,
            kind: u32,
            data: *const u8,
            size: u32,
        ) -> i32;
        fn RegCloseKey(key: isize) -> i32;
        fn RegDeleteTreeW(key: isize, sub_key: *const u16) -> i32;
    }

    #[link(name = "shell32")]
    extern "system" {
        fn SetCurrentProcessExplicitAppUserModelID(id: *const u16) -> i32;
    }

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    fn sub_key() -> Vec<u16> {
        wide(&format!("Software\\Classes\\AppUserModelId\\{APP_ID}"))
    }

    /// The icon the toast wears. Windows wants a file, so the embedded one
    /// is written out beside the settings, where a portable copy keeps it
    /// inside its own folder.
    fn icon_path() -> Option<std::path::PathBuf> {
        let path = crate::bootstrap::config_dir().join("snag-icon.png");
        if !path.is_file() {
            std::fs::write(&path, crate::icon::ICON_PNG).ok()?;
        }
        Some(path)
    }

    pub fn register() {
        // The process side, so the taskbar groups this window under the
        // same id the shortcut and the toast use. Harmless if it fails.
        unsafe { SetCurrentProcessExplicitAppUserModelID(wide(APP_ID).as_ptr()) };

        let mut key: isize = 0;
        let created = unsafe {
            RegCreateKeyExW(
                HKEY_CURRENT_USER,
                sub_key().as_ptr(),
                0,
                std::ptr::null(),
                REG_OPTION_NON_VOLATILE,
                KEY_WRITE,
                std::ptr::null(),
                &mut key,
                std::ptr::null_mut(),
            )
        };
        if created != ERROR_SUCCESS {
            return;
        }

        let set = |name: &str, value: &str| {
            let data = wide(value);
            unsafe {
                RegSetValueExW(
                    key,
                    wide(name).as_ptr(),
                    0,
                    REG_SZ,
                    data.as_ptr() as *const u8,
                    (data.len() * 2) as u32,
                )
            };
        };
        set("DisplayName", "Snag");
        if let Some(icon) = icon_path() {
            set("IconUri", &icon.display().to_string());
        }
        unsafe { RegCloseKey(key) };
    }

    pub fn unregister() {
        unsafe { RegDeleteTreeW(HKEY_CURRENT_USER, sub_key().as_ptr()) };
        let _ = std::fs::remove_file(crate::bootstrap::config_dir().join("snag-icon.png"));
    }
}
