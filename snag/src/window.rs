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

/// Un-minimise the window and bring it forward. Does nothing if there is
/// nothing to restore.
pub fn restore() {
    #[cfg(windows)]
    imp::restore();
}

#[cfg(windows)]
mod imp {
    use std::sync::atomic::{AtomicIsize, Ordering};

    /// The window handle, found once and reused. Zero means "not found yet".
    static MAIN_WINDOW: AtomicIsize = AtomicIsize::new(0);

    const SW_RESTORE: i32 = 9;

    #[link(name = "user32")]
    extern "system" {
        fn EnumWindows(callback: extern "system" fn(isize, isize) -> i32, param: isize) -> i32;
        fn GetWindowThreadProcessId(window: isize, process: *mut u32) -> u32;
        fn ShowWindow(window: isize, command: i32) -> i32;
        fn SetForegroundWindow(window: isize) -> i32;
        fn GetWindowRect(window: isize, rect: *mut Rect) -> i32;
        fn GetWindow(window: isize, command: u32) -> isize;
        fn IsWindowVisible(window: isize) -> i32;
        fn GetWindowTextLengthW(window: isize) -> i32;
    }

    #[repr(C)]
    #[derive(Default)]
    struct Rect {
        left: i32,
        top: i32,
        right: i32,
        bottom: i32,
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

        // The real window is on screen and has a title. Without both of these
        // the search lands on one of the invisible helper windows that the
        // tray icon and the graphics stack create in this same process.
        if unsafe { IsWindowVisible(window) } == 0 {
            return 1;
        }
        if unsafe { GetWindowTextLengthW(window) } == 0 {
            return 1;
        }

        // ...and only one big enough to be the real thing.
        let mut rect = Rect::default();
        if unsafe { GetWindowRect(window, &mut rect) } == 0 {
            return 1;
        }
        if rect.right - rect.left < 200 || rect.bottom - rect.top < 200 {
            return 1;
        }

        MAIN_WINDOW.store(window, Ordering::Relaxed);
        0 // found it, stop
    }

    pub fn remember() {
        if MAIN_WINDOW.load(Ordering::Relaxed) == 0 {
            unsafe { EnumWindows(visit, 0) };
        }
    }

    pub fn restore() {
        let window = MAIN_WINDOW.load(Ordering::Relaxed);
        if window == 0 {
            return;
        }
        unsafe {
            ShowWindow(window, SW_RESTORE);
            // Windows often refuses this from a background thread, which is
            // fine: the window is back, it just may not be given focus.
            SetForegroundWindow(window);
        }
    }
}
