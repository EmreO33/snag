//! Restoring the window from the tray.
//!
//! eframe's viewport commands turned out not to be dependable for this:
//! hiding a viewport loses its geometry and it returns a few pixels square,
//! and `Minimized(false)` does not un-minimise at all. Since background mode
//! is worthless if the window cannot be got back, this drives the platform
//! window directly instead.
//!
//! It also means the clipboard watcher can raise the window from its own
//! thread, without depending on the UI loop running while minimised.

/// Remember the main window so it can be restored later.
pub fn remember_main_window() {
    #[cfg(windows)]
    imp::remember();
}

/// Un-minimise the window and bring it forward. Returns whether there is
/// now a window on screen, so a caller can fall back to asking eframe.
pub fn restore() -> bool {
    #[cfg(windows)]
    {
        imp::restore()
    }
    #[cfg(not(windows))]
    false
}

/// Put the window into the tray before anyone sees it, from a thread of its
/// own, started before the window exists.
///
/// Starting in the tray used to mean letting the window appear and then
/// minimising it on the first frame, which is a flash of a window nobody
/// asked for. The window cannot simply be created hidden instead: a hidden
/// window gets no redraws, eframe runs the app from redraws, and a Snag that
/// never redraws never starts its downloads or answers its tray. So it is
/// created invisible, and this watches for it to exist and shows it
/// minimised, which is a state that both stays out of sight and keeps the
/// app running.
pub fn park_in_tray() {
    #[cfg(windows)]
    std::thread::spawn(imp::park_in_tray);
}

/// Put the window away: off the screen and out of the taskbar, so closing
/// to the tray looks like closing rather than like minimising.
///
/// Note what this deliberately does *not* do, which is hide the window.
/// A hidden window gets no redraws, and eframe only runs the app from a
/// redraw, so a hidden Snag stops pumping its queue, stops noticing copied
/// links, and stops answering its own tray menu, while looking perfectly
/// alive in Task Manager. Measured, not assumed. So the window is minimised,
/// which keeps the loop running, and the taskbar button is taken away
/// separately, which is the part the user actually sees.
///
/// Returns whether the window really is out of the way now. That matters at
/// startup: for the first frame or two there is no window yet, and for a few
/// frames after that winit shows it again as it finishes setting it up, so
/// one request is not enough and the caller has to keep asking.
pub fn to_tray() -> bool {
    #[cfg(windows)]
    {
        imp::remember();
        imp::to_tray()
    }
    #[cfg(not(windows))]
    false
}

#[cfg(windows)]
mod imp {
    use std::sync::atomic::{AtomicIsize, Ordering};

    /// The window handle, found once and reused. Zero means "not found yet".
    static MAIN_WINDOW: AtomicIsize = AtomicIsize::new(0);

    const SW_RESTORE: i32 = 9;
    const SW_MINIMIZE: i32 = 6;
    const SW_SHOW: i32 = 5;
    const SW_SHOWMINNOACTIVE: i32 = 7;

    #[link(name = "user32")]
    extern "system" {
        fn EnumWindows(callback: extern "system" fn(isize, isize) -> i32, param: isize) -> i32;
        fn GetWindowThreadProcessId(window: isize, process: *mut u32) -> u32;
        fn ShowWindow(window: isize, command: i32) -> i32;
        fn SetForegroundWindow(window: isize) -> i32;
        fn GetWindowPlacement(window: isize, placement: *mut WindowPlacement) -> i32;
        fn IsWindow(window: isize) -> i32;
        fn GetWindow(window: isize, command: u32) -> isize;
        fn IsWindowVisible(window: isize) -> i32;
        fn GetWindowTextLengthW(window: isize) -> i32;
        fn IsIconic(window: isize) -> i32;
    }

    #[repr(C)]
    #[derive(Default)]
    struct Rect {
        left: i32,
        top: i32,
        right: i32,
        bottom: i32,
    }

    #[repr(C)]
    #[derive(Default)]
    struct Point {
        x: i32,
        y: i32,
    }

    /// Where a window sits, including where it would sit if it were not
    /// minimised, which is the part that matters here.
    #[repr(C)]
    #[derive(Default)]
    struct WindowPlacement {
        length: u32,
        flags: u32,
        show_command: u32,
        min_position: Point,
        max_position: Point,
        normal_position: Rect,
    }

    #[link(name = "kernel32")]
    extern "system" {
        fn GetCurrentProcessId() -> u32;
    }

    extern "system" fn visit(window: isize, _param: isize) -> i32 {
        let mut owner = 0u32;
        unsafe { GetWindowThreadProcessId(window, &mut owner) };
        if owner != unsafe { GetCurrentProcessId() } {
            return 1; // keep looking
        }

        // Only top-level windows: the tray icon quietly creates a helper
        // window in this same process, and picking that one would be useless.
        const GW_OWNER: u32 = 4;
        if unsafe { GetWindow(window, GW_OWNER) } != 0 {
            return 1;
        }

        // The real window has a title. Without this the search lands on one
        // of the helper windows that the tray icon and the graphics stack
        // create in this same process.
        //
        // Being on screen is deliberately not required: a window waiting to
        // be parked in the tray has not been shown yet, and that is exactly
        // when it most needs finding.
        if unsafe { GetWindowTextLengthW(window) } == 0 {
            return 1;
        }

        // ...and only one big enough to be the real thing.
        //
        // The size has to come from the window's placement rather than from
        // where it is right now: a minimised window reports itself as a
        // 237x39 strip parked at -32000,-32000, so asking where it is would
        // reject the very window we are looking for, and "show snag" would
        // then quietly do nothing. Measured, after exactly that happened.
        let mut placement = WindowPlacement {
            length: std::mem::size_of::<WindowPlacement>() as u32,
            ..Default::default()
        };
        if unsafe { GetWindowPlacement(window, &mut placement) } == 0 {
            return 1;
        }
        let normal = &placement.normal_position;
        if normal.right - normal.left < 200 || normal.bottom - normal.top < 200 {
            return 1;
        }

        MAIN_WINDOW.store(window, Ordering::Relaxed);
        0 // found it, stop
    }

    pub fn remember() {
        // A handle that no longer names a window is worse than none: it
        // makes every attempt to show the window look like it worked.
        let known = MAIN_WINDOW.load(Ordering::Relaxed);
        if known != 0 && unsafe { IsWindow(known) } != 0 {
            return;
        }
        MAIN_WINDOW.store(0, Ordering::Relaxed);
        unsafe { EnumWindows(visit, 0) };
    }

    /// Wait for the window to exist, then show it minimised and take its
    /// taskbar button away. Gives up after a while rather than spinning
    /// forever if something went wrong with the window entirely.
    pub fn park_in_tray() {
        for _ in 0..600 {
            remember();
            let window = MAIN_WINDOW.load(Ordering::Relaxed);
            if window != 0 {
                unsafe {
                    taskbar(window, false);
                    // Minimised and not activated: it never takes focus from
                    // whatever the user is doing, which matters most when
                    // this is running at login.
                    ShowWindow(window, SW_SHOWMINNOACTIVE);
                    if IsIconic(window) != 0 {
                        return;
                    }
                }
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
    }

    pub fn to_tray() -> bool {
        let window = MAIN_WINDOW.load(Ordering::Relaxed);
        if window == 0 {
            return false;
        }
        unsafe {
            // Button first, then the window, so nothing is seen flying down
            // to a taskbar slot that is about to disappear.
            taskbar(window, false);
            ShowWindow(window, SW_MINIMIZE);
            IsIconic(window) != 0
        }
    }

    /// Add or remove this window's taskbar button, through the shell's own
    /// interface for it. Failure is not worth reporting: the window is
    /// already minimised either way, and a leftover button is a cosmetic
    /// problem rather than a broken app.
    unsafe fn taskbar(window: isize, show: bool) {
        use std::ffi::c_void;

        #[repr(C)]
        struct Guid(u32, u16, u16, [u8; 8]);
        const CLSID_TASKBAR_LIST: Guid = Guid(
            0x56FDF344,
            0xFD6D,
            0x11d0,
            [0x95, 0x8A, 0x00, 0x60, 0x97, 0xC9, 0xA0, 0x90],
        );
        const IID_ITASKBAR_LIST: Guid = Guid(
            0x56FDF342,
            0xFD6D,
            0x11d0,
            [0x95, 0x8A, 0x00, 0x60, 0x97, 0xC9, 0xA0, 0x90],
        );
        const CLSCTX_INPROC_SERVER: u32 = 1;
        const COINIT_APARTMENTTHREADED: u32 = 2;

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
        }

        let init = CoInitializeEx(std::ptr::null(), COINIT_APARTMENTTHREADED);
        let mut list: ComObject = std::ptr::null_mut();
        if CoCreateInstance(
            &CLSID_TASKBAR_LIST,
            std::ptr::null(),
            CLSCTX_INPROC_SERVER,
            &IID_ITASKBAR_LIST,
            &mut list,
        ) >= 0
            && !list.is_null()
        {
            let table = *list as *const *const c_void;
            // HrInit is slot 3, AddTab 4, DeleteTab 5.
            let hr_init: unsafe extern "system" fn(ComObject) -> i32 =
                std::mem::transmute(*table.add(3));
            hr_init(list);
            let slot = if show { 4 } else { 5 };
            let tab: unsafe extern "system" fn(ComObject, isize) -> i32 =
                std::mem::transmute(*table.add(slot));
            tab(list, window);
            let release: unsafe extern "system" fn(ComObject) -> u32 =
                std::mem::transmute(*table.add(2));
            release(list);
        }
        if init >= 0 {
            CoUninitialize();
        }
    }

    pub fn restore() -> bool {
        // Look again rather than give up: whoever is asking wants the window
        // back, and the last search may have run at an awkward moment.
        remember();
        let window = MAIN_WINDOW.load(Ordering::Relaxed);
        if window == 0 {
            return false;
        }
        unsafe {
            // The button comes back before the window does, so it is there
            // to be clicked the moment the window is.
            taskbar(window, true);
            ShowWindow(window, SW_SHOW);
            ShowWindow(window, SW_RESTORE);
            // Windows often refuses this from a background thread, which is
            // fine: the window is back, it just may not be given focus.
            SetForegroundWindow(window);
            IsWindowVisible(window) != 0 && IsIconic(window) == 0
        }
    }
}
