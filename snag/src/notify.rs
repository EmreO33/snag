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
    use std::ffi::c_void;
    use std::path::{Path, PathBuf};

    // --- the little of COM this needs ----------------------------------
    //
    // Three interfaces on the shell's link object, called through their
    // vtables by hand rather than through a bindings crate: the crate would
    // be the largest dependency in the tree for four method calls.

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct Guid(u32, u16, u16, [u8; 8]);

    const CLSID_SHELL_LINK: Guid = Guid(0x00021401, 0, 0, [0xC0, 0, 0, 0, 0, 0, 0, 0x46]);
    const IID_ISHELL_LINK_W: Guid = Guid(0x000214F9, 0, 0, [0xC0, 0, 0, 0, 0, 0, 0, 0x46]);
    const IID_IPERSIST_FILE: Guid = Guid(0x0000010b, 0, 0, [0xC0, 0, 0, 0, 0, 0, 0, 0x46]);
    const IID_IPROPERTY_STORE: Guid = Guid(
        0x886d8eeb,
        0x8cf2,
        0x4446,
        [0x8d, 0x02, 0xcd, 0xba, 0x1d, 0xbd, 0xcf, 0x99],
    );

    /// System.AppUserModel.ID: the property the shell reads the id from.
    #[repr(C)]
    struct PropertyKey {
        fmtid: Guid,
        pid: u32,
    }
    const PKEY_APP_USER_MODEL_ID: PropertyKey = PropertyKey {
        fmtid: Guid(
            0x9F4C2855,
            0x9F79,
            0x4B39,
            [0xA8, 0xD0, 0xE1, 0xD4, 0x2D, 0xE1, 0xD5, 0xF3],
        ),
        pid: 5,
    };

    /// PROPVARIANT, laid out for the one variant type used here.
    #[repr(C)]
    struct PropVariant {
        vt: u16,
        _reserved: [u16; 3],
        pwsz: *mut u16,
        _pad: usize,
    }
    const VT_EMPTY: u16 = 0;
    const VT_LPWSTR: u16 = 31;

    type ComObject = *mut *const c_void;

    #[link(name = "ole32")]
    extern "system" {
        fn CoInitializeEx(reserved: *const c_void, model: u32) -> i32;
        fn CoUninitialize();
        fn CoCreateInstance(
            clsid: *const Guid,
            outer: *const c_void,
            context: u32,
            iid: *const Guid,
            out: *mut ComObject,
        ) -> i32;
        fn CoTaskMemAlloc(size: usize) -> *mut c_void;
        fn PropVariantClear(v: *mut PropVariant) -> i32;
    }

    #[link(name = "shell32")]
    extern "system" {
        fn SetCurrentProcessExplicitAppUserModelID(id: *const u16) -> i32;
        fn SHGetPropertyStoreFromParsingName(
            path: *const u16,
            bind: *const c_void,
            flags: u32,
            iid: *const Guid,
            out: *mut ComObject,
        ) -> i32;
    }

    #[link(name = "advapi32")]
    extern "system" {
        fn RegDeleteTreeW(key: isize, sub_key: *const u16) -> i32;
    }

    const COINIT_APARTMENTTHREADED: u32 = 2;
    const CLSCTX_INPROC_SERVER: u32 = 1;
    const GPS_READWRITE: u32 = 2;
    const SLGP_RAWPATH: u32 = 4;
    const HKEY_CURRENT_USER: isize = 0x80000001u32 as i32 as isize;

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    fn wide_path(p: &Path) -> Vec<u16> {
        wide(&p.display().to_string())
    }

    /// Call slot `index` of an object's vtable.
    macro_rules! vcall {
        ($obj:expr, $index:expr, $sig:ty $(, $arg:expr)*) => {{
            let table = *$obj as *const *const c_void;
            let f: $sig = std::mem::transmute(*table.add($index));
            f($obj $(, $arg)*)
        }};
    }

    unsafe fn release(obj: ComObject) {
        if !obj.is_null() {
            vcall!(obj, 2, unsafe extern "system" fn(ComObject) -> u32);
        }
    }

    unsafe fn query(obj: ComObject, iid: &Guid) -> Option<ComObject> {
        let mut out: ComObject = std::ptr::null_mut();
        let hr = vcall!(
            obj,
            0,
            unsafe extern "system" fn(ComObject, *const Guid, *mut ComObject) -> i32,
            iid,
            &mut out
        );
        (hr >= 0 && !out.is_null()).then_some(out)
    }

    unsafe fn new_shell_link() -> Option<ComObject> {
        let mut link: ComObject = std::ptr::null_mut();
        let hr = CoCreateInstance(
            &CLSID_SHELL_LINK,
            std::ptr::null(),
            CLSCTX_INPROC_SERVER,
            &IID_ISHELL_LINK_W,
            &mut link,
        );
        (hr >= 0 && !link.is_null()).then_some(link)
    }

    /// The app id a property store holds, if any.
    unsafe fn read_app_id(store: ComObject) -> Option<String> {
        let mut value = PropVariant {
            vt: VT_EMPTY,
            _reserved: [0; 3],
            pwsz: std::ptr::null_mut(),
            _pad: 0,
        };
        // IPropertyStore::GetValue is slot 5.
        let hr = vcall!(
            store,
            5,
            unsafe extern "system" fn(ComObject, *const PropertyKey, *mut PropVariant) -> i32,
            &PKEY_APP_USER_MODEL_ID,
            &mut value
        );
        let found = if hr >= 0 && value.vt == VT_LPWSTR && !value.pwsz.is_null() {
            let mut len = 0;
            while *value.pwsz.add(len) != 0 {
                len += 1;
            }
            Some(String::from_utf16_lossy(std::slice::from_raw_parts(
                value.pwsz, len,
            )))
        } else {
            None
        };
        PropVariantClear(&mut value);
        found
    }

    /// Stamp the app id into a property store and commit it.
    unsafe fn write_app_id(store: ComObject) -> bool {
        let id = wide(APP_ID);
        let mem = CoTaskMemAlloc(id.len() * 2) as *mut u16;
        if mem.is_null() {
            return false;
        }
        std::ptr::copy_nonoverlapping(id.as_ptr(), mem, id.len());
        let mut value = PropVariant {
            vt: VT_LPWSTR,
            _reserved: [0; 3],
            pwsz: mem,
            _pad: 0,
        };
        // SetValue is slot 6, Commit slot 7.
        let set = vcall!(
            store,
            6,
            unsafe extern "system" fn(ComObject, *const PropertyKey, *const PropVariant) -> i32,
            &PKEY_APP_USER_MODEL_ID,
            &value
        );
        let committed = vcall!(store, 7, unsafe extern "system" fn(ComObject) -> i32);
        PropVariantClear(&mut value);
        set >= 0 && committed >= 0
    }

    /// Where a shortcut points, read without resolving it.
    unsafe fn link_target(path: &Path) -> Option<PathBuf> {
        let link = new_shell_link()?;
        let result = (|| {
            let file = query(link, &IID_IPERSIST_FILE)?;
            // IPersistFile::Load is slot 5.
            let loaded = vcall!(
                file,
                5,
                unsafe extern "system" fn(ComObject, *const u16, u32) -> i32,
                wide_path(path).as_ptr(),
                0
            );
            release(file);
            if loaded < 0 {
                return None;
            }
            let mut buf = [0u16; 1024];
            // IShellLinkW::GetPath is slot 3.
            let got = vcall!(
                link,
                3,
                unsafe extern "system" fn(ComObject, *mut u16, i32, *mut c_void, u32) -> i32,
                buf.as_mut_ptr(),
                buf.len() as i32,
                std::ptr::null_mut(),
                SLGP_RAWPATH
            );
            if got < 0 {
                return None;
            }
            let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
            Some(PathBuf::from(String::from_utf16_lossy(&buf[..len])))
        })();
        release(link);
        result
    }

    /// Make sure an existing shortcut carries the app id.
    unsafe fn stamp(path: &Path) -> bool {
        let mut store: ComObject = std::ptr::null_mut();
        let hr = SHGetPropertyStoreFromParsingName(
            wide_path(path).as_ptr(),
            std::ptr::null(),
            GPS_READWRITE,
            &IID_IPROPERTY_STORE,
            &mut store,
        );
        if hr < 0 || store.is_null() {
            return false;
        }
        let ok = match read_app_id(store).as_deref() {
            Some(id) if id == APP_ID => true,
            _ => write_app_id(store),
        };
        release(store);
        ok
    }

    /// Create a shortcut to `target` at `path`, carrying the app id.
    unsafe fn create(path: &Path, target: &Path) -> bool {
        let Some(link) = new_shell_link() else {
            return false;
        };
        let ok = (|| {
            // IShellLinkW::SetPath is slot 20.
            let set = vcall!(
                link,
                20,
                unsafe extern "system" fn(ComObject, *const u16) -> i32,
                wide_path(target).as_ptr()
            );
            if set < 0 {
                return None;
            }
            let store = query(link, &IID_IPROPERTY_STORE)?;
            let stamped = write_app_id(store);
            release(store);
            if !stamped {
                return None;
            }
            let file = query(link, &IID_IPERSIST_FILE)?;
            // IPersistFile::Save is slot 6.
            let saved = vcall!(
                file,
                6,
                unsafe extern "system" fn(ComObject, *const u16, i32) -> i32,
                wide_path(path).as_ptr(),
                1
            );
            release(file);
            Some(saved >= 0)
        })()
        .unwrap_or(false);
        release(link);
        ok
    }

    // --- putting it together -----------------------------------------------

    fn programs_dir() -> Option<PathBuf> {
        let appdata = std::env::var_os("APPDATA")?;
        Some(PathBuf::from(appdata).join("Microsoft\\Windows\\Start Menu\\Programs"))
    }

    /// The shortcut Snag makes when nobody else has.
    fn own_shortcut() -> Option<PathBuf> {
        programs_dir().map(|d| d.join("Snag.lnk"))
    }

    /// Every shortcut under the Start Menu, a few levels deep, so the
    /// installer's folder and Scoop's are both looked in.
    fn shortcuts_under(dir: &Path, depth: u8, out: &mut Vec<PathBuf>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                if depth > 0 {
                    shortcuts_under(&path, depth - 1, out);
                }
            } else if path
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("lnk"))
            {
                out.push(path);
            }
        }
    }

    fn same_file(a: &Path, b: &Path) -> bool {
        match (a.canonicalize(), b.canonicalize()) {
            (Ok(a), Ok(b)) => a == b,
            _ => a
                .to_string_lossy()
                .eq_ignore_ascii_case(&b.to_string_lossy()),
        }
    }

    pub fn register() {
        // The process side, so the taskbar groups this window under the
        // same id the shortcut and the toast use. Harmless if it fails.
        unsafe { SetCurrentProcessExplicitAppUserModelID(wide(APP_ID).as_ptr()) };

        let Ok(exe) = std::env::current_exe() else {
            return;
        };
        let Some(programs) = programs_dir() else {
            return;
        };

        unsafe {
            let init = CoInitializeEx(std::ptr::null(), COINIT_APARTMENTTHREADED);
            let mut links = Vec::new();
            shortcuts_under(&programs, 2, &mut links);

            // A shortcut that already points at this Snag, from the
            // installer, Scoop, or a person, is stamped where it is.
            let mut stamped = false;
            for link in &links {
                if link_target(link).is_some_and(|t| same_file(&t, &exe)) && stamp(link) {
                    stamped = true;
                }
            }

            // Nobody made one: a portable copy, or a bare exe someone put
            // somewhere. Snag makes its own, and keeps it pointed at itself.
            if !stamped {
                if let Some(own) = own_shortcut() {
                    create(&own, &exe);
                }
            }

            if init >= 0 {
                CoUninitialize();
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
