//! Windows shortcuts (.lnk), enough of them for the two things Snag needs:
//! an app id stamped on a Start Menu shortcut, which is the only way Windows
//! will show a notification from an unpackaged app, and a Startup folder
//! shortcut, which is how it runs at login.
//!
//! The shell's link object is COM, and it is reached here through its vtables
//! by hand rather than through a bindings crate: the crate would be the
//! largest dependency in the tree for half a dozen method calls. Everything
//! is best effort. A shortcut that cannot be written costs a notification or
//! an autostart, and neither is worth refusing to run over.

#![cfg_attr(
    not(windows),
    allow(dead_code, reason = "shortcuts are a windows idea")
)]

use std::path::{Path, PathBuf};

/// Create (or overwrite) a shortcut at `path` pointing at `target`.
///
/// `args` is the command line the shortcut passes, empty for none, and
/// `app_id` is stamped into the shortcut's property store when given.
pub fn create(path: &Path, target: &Path, args: &str, app_id: Option<&str>) -> bool {
    #[cfg(windows)]
    {
        imp::create(path, target, args, app_id)
    }
    #[cfg(not(windows))]
    {
        let _ = (path, target, args, app_id);
        false
    }
}

/// Where a shortcut points, without resolving or repairing it.
pub fn target_of(path: &Path) -> Option<PathBuf> {
    #[cfg(windows)]
    {
        imp::target_of(path)
    }
    #[cfg(not(windows))]
    {
        let _ = path;
        None
    }
}

/// Put `app_id` on a shortcut that already exists, unless it is there.
/// Returns whether the shortcut ended up carrying it.
pub fn stamp_app_id(path: &Path, app_id: &str) -> bool {
    #[cfg(windows)]
    {
        imp::stamp_app_id(path, app_id)
    }
    #[cfg(not(windows))]
    {
        let _ = (path, app_id);
        false
    }
}

/// True when two paths name the same file, falling back to comparing the
/// text when neither can be resolved.
pub fn same_file(a: &Path, b: &Path) -> bool {
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(a), Ok(b)) => a == b,
        _ => a
            .to_string_lossy()
            .eq_ignore_ascii_case(&b.to_string_lossy()),
    }
}

/// `%APPDATA%\Microsoft\Windows\Start Menu\Programs`.
pub fn programs_dir() -> Option<PathBuf> {
    let appdata = std::env::var_os("APPDATA")?;
    Some(PathBuf::from(appdata).join("Microsoft\\Windows\\Start Menu\\Programs"))
}

/// The all-users Start Menu, where a machine-wide install puts its shortcut.
pub fn common_programs_dir() -> Option<PathBuf> {
    let data = std::env::var_os("ProgramData")?;
    Some(PathBuf::from(data).join("Microsoft\\Windows\\Start Menu\\Programs"))
}

/// The Startup folder: everything in it runs at login.
pub fn startup_dir() -> Option<PathBuf> {
    programs_dir().map(|d| d.join("Startup"))
}

/// Every shortcut under `dir`, `depth` levels down.
pub fn shortcuts_under(dir: &Path, depth: u8, out: &mut Vec<PathBuf>) {
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

#[cfg(windows)]
mod imp {
    use std::ffi::c_void;
    use std::path::{Path, PathBuf};

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
        fn SHGetPropertyStoreFromParsingName(
            path: *const u16,
            bind: *const c_void,
            flags: u32,
            iid: *const Guid,
            out: *mut ComObject,
        ) -> i32;
    }

    const COINIT_APARTMENTTHREADED: u32 = 2;
    const CLSCTX_INPROC_SERVER: u32 = 1;
    const GPS_READWRITE: u32 = 2;
    const SLGP_RAWPATH: u32 = 4;

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

    /// COM, initialised for as long as this lives. Apartment threaded, which
    /// is what the shell wants.
    struct Com(bool);

    impl Com {
        fn init() -> Self {
            Com(unsafe { CoInitializeEx(std::ptr::null(), COINIT_APARTMENTTHREADED) } >= 0)
        }
    }

    impl Drop for Com {
        fn drop(&mut self) {
            if self.0 {
                unsafe { CoUninitialize() };
            }
        }
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

    /// Stamp an app id into a property store and commit it.
    unsafe fn write_app_id(store: ComObject, app_id: &str) -> bool {
        let id = wide(app_id);
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

    pub fn target_of(path: &Path) -> Option<PathBuf> {
        let _com = Com::init();
        unsafe {
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
    }

    pub fn stamp_app_id(path: &Path, app_id: &str) -> bool {
        let _com = Com::init();
        unsafe {
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
                Some(id) if id == app_id => true,
                _ => write_app_id(store, app_id),
            };
            release(store);
            ok
        }
    }

    pub fn create(path: &Path, target: &Path, args: &str, app_id: Option<&str>) -> bool {
        let _com = Com::init();
        unsafe {
            let Some(link) = new_shell_link() else {
                return false;
            };
            let ok = (|| {
                // IShellLinkW::SetPath is slot 20, SetArguments slot 11.
                let set = vcall!(
                    link,
                    20,
                    unsafe extern "system" fn(ComObject, *const u16) -> i32,
                    wide_path(target).as_ptr()
                );
                if set < 0 {
                    return None;
                }
                if !args.is_empty() {
                    let set = vcall!(
                        link,
                        11,
                        unsafe extern "system" fn(ComObject, *const u16) -> i32,
                        wide(args).as_ptr()
                    );
                    if set < 0 {
                        return None;
                    }
                }
                // The working directory decides where a relative path in a
                // setting would land, so it is the exe's folder rather than
                // whatever the shell felt like.
                if let Some(dir) = target.parent() {
                    vcall!(
                        link,
                        9,
                        unsafe extern "system" fn(ComObject, *const u16) -> i32,
                        wide_path(dir).as_ptr()
                    );
                }
                if let Some(id) = app_id {
                    let store = query(link, &IID_IPROPERTY_STORE)?;
                    let stamped = write_app_id(store, id);
                    release(store);
                    if !stamped {
                        return None;
                    }
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
    }
}
