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

/// Whether closing the window can put Snag in the tray: out of sight, and
/// still running.
///
/// Always on Windows and macOS. On Linux only on X11, which Snag asks for
/// when background mode is on (see `prefer_x11`): a Wayland window cannot be
/// hidden, only minimised, and a minimised Wayland window gets no frames, so
/// a Snag hidden that way would stop downloading and stop answering its tray.
pub fn can_hide() -> bool {
    #[cfg(target_os = "linux")]
    {
        linux::ON_X11.load(std::sync::atomic::Ordering::Relaxed)
    }
    #[cfg(not(target_os = "linux"))]
    true
}

/// On Linux, decide between X11 and Wayland before the window exists.
/// Returns whether to ask winit for X11 when it would otherwise pick Wayland.
///
/// X11 (XWayland, on a Wayland desktop) only when Snag should be able to
/// hide in the tray, since that is the one thing Wayland cannot do; the rest
/// of the time Snag gets whatever the desktop prefers.
#[cfg(target_os = "linux")]
pub fn prefer_x11(background: bool) -> bool {
    let set = |name| std::env::var_os(name).is_some_and(|v| !v.is_empty());
    let (wayland, x11) = (set("WAYLAND_DISPLAY"), set("DISPLAY"));
    let force = background && wayland && x11;
    linux::ON_X11.store(
        x11 && (force || !wayland),
        std::sync::atomic::Ordering::Relaxed,
    );
    force
}

/// On Linux, have the window start minimised and off the taskbar, for a
/// launch that belongs in the tray. Returns whether it worked.
///
/// eframe shows its window once the first frame is drawn, whatever it was
/// built as, so hiding it afterwards meant a window that flashed up and
/// vanished. Told before that first appearance, the window manager puts it
/// straight away instead: minimised, which on X11 still gets frames, and
/// with no taskbar button or pager entry for it. `back_in_view` undoes
/// both, the first time the window is asked for.
#[cfg(target_os = "linux")]
pub fn start_out_of_sight(window: &impl raw_window_handle::HasWindowHandle) -> bool {
    use raw_window_handle::RawWindowHandle;
    let id = match window.window_handle().map(|h| h.as_raw()) {
        Ok(RawWindowHandle::Xlib(h)) => h.window as u32,
        Ok(RawWindowHandle::Xcb(h)) => h.window.get(),
        _ => return false,
    };
    let done = linux::start_out_of_sight(id).is_some();
    if done {
        linux::OUT_OF_SIGHT.store(id, std::sync::atomic::Ordering::Relaxed);
    }
    done
}

/// Undo `start_out_of_sight`, if it was done and not yet undone, so the
/// window shows as any other: on the taskbar, and not minimised the next
/// time it is mapped. Call before asking for the window to be shown.
pub fn back_in_view() {
    #[cfg(target_os = "linux")]
    {
        let id = linux::OUT_OF_SIGHT.swap(0, std::sync::atomic::Ordering::Relaxed);
        if id != 0 {
            let _ = linux::back_in_view(id);
        }
    }
}

#[cfg(target_os = "linux")]
mod linux {
    use std::sync::atomic::{AtomicBool, AtomicU32};
    use x11rb::connection::Connection;
    use x11rb::protocol::xproto::{
        AtomEnum, ClientMessageEvent, ConnectionExt as _, EventMask, PropMode, Window,
    };
    use x11rb::rust_connection::RustConnection;
    use x11rb::wrapper::ConnectionExt as _;

    /// Whether the window is on X11, where it can be hidden.
    pub static ON_X11: AtomicBool = AtomicBool::new(false);

    /// The window `start_out_of_sight` put away, until `back_in_view`.
    pub static OUT_OF_SIGHT: AtomicU32 = AtomicU32::new(0);

    /// WM_HINTS, as ICCCM lays it out: nine 32-bit fields, the flags first
    /// and the initial state third.
    const STATE_HINT: u32 = 1 << 1;
    const NORMAL_STATE: u32 = 1;
    const ICONIC_STATE: u32 = 3;

    fn atom(conn: &RustConnection, name: &str) -> Option<u32> {
        Some(
            conn.intern_atom(false, name.as_bytes())
                .ok()?
                .reply()
                .ok()?
                .atom,
        )
    }

    /// Set the state the window manager gives the window when it is mapped,
    /// keeping whatever else winit put in the hints.
    fn set_initial_state(conn: &RustConnection, window: Window, state: u32) -> Option<()> {
        let current = conn
            .get_property(false, window, AtomEnum::WM_HINTS, AtomEnum::WM_HINTS, 0, 9)
            .ok()?
            .reply()
            .ok()?;
        let mut hints = [0u32; 9];
        for (slot, value) in hints
            .iter_mut()
            .zip(current.value32().into_iter().flatten())
        {
            *slot = value;
        }
        hints[0] |= STATE_HINT;
        hints[2] = state;
        conn.change_property32(
            PropMode::REPLACE,
            window,
            AtomEnum::WM_HINTS,
            AtomEnum::WM_HINTS,
            &hints,
        )
        .ok()?;
        Some(())
    }

    fn skip_atoms(conn: &RustConnection) -> Option<[u32; 2]> {
        Some([
            atom(conn, "_NET_WM_STATE_SKIP_TASKBAR")?,
            atom(conn, "_NET_WM_STATE_SKIP_PAGER")?,
        ])
    }

    /// Before the window is first mapped, so set as plain properties: that
    /// is how a client states them for a window the manager has not seen.
    pub fn start_out_of_sight(window: Window) -> Option<()> {
        let (conn, _) = RustConnection::connect(None).ok()?;
        set_initial_state(&conn, window, ICONIC_STATE)?;
        let state = atom(&conn, "_NET_WM_STATE")?;
        conn.change_property32(
            PropMode::REPLACE,
            window,
            state,
            AtomEnum::ATOM,
            &skip_atoms(&conn)?,
        )
        .ok()?;
        conn.sync().ok()
    }

    /// Once the window manager has the window, its state changes by asking
    /// it, with a message to the root window.
    pub fn back_in_view(window: Window) -> Option<()> {
        let (conn, screen) = RustConnection::connect(None).ok()?;
        set_initial_state(&conn, window, NORMAL_STATE)?;
        let root = conn.setup().roots.get(screen)?.root;
        let state = atom(&conn, "_NET_WM_STATE")?;
        const REMOVE: u32 = 0;
        const FROM_APPLICATION: u32 = 1;
        let [taskbar, pager] = skip_atoms(&conn)?;
        let message = ClientMessageEvent::new(
            32,
            window,
            state,
            [REMOVE, taskbar, pager, FROM_APPLICATION, 0],
        );
        conn.send_event(
            false,
            root,
            EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY,
            message,
        )
        .ok()?;
        conn.sync().ok()
    }
}

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

/// Make sure the app runs a frame soon, from any thread, even while the
/// window sits minimised in the tray.
///
/// `request_repaint` is not enough on its own there. eframe runs the app from
/// redraws, and Windows does not redraw a minimised window, so an idle Snag
/// in the tray slept through whatever had just happened: a tray menu click
/// took effect on the second click, and a finished download started the next
/// one late. An input message is an event eframe answers whatever state the
/// window is in, so a minimised window gets one of those as well.
pub fn wake() {
    #[cfg(windows)]
    imp::wake();
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
        fn PostMessageW(window: isize, message: u32, wparam: usize, lparam: isize) -> i32;
    }

    const WM_MOUSEMOVE: u32 = 0x0200;

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
                    transitions(window, false);
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

    pub fn wake() {
        let window = MAIN_WINDOW.load(Ordering::Relaxed);
        if window == 0 {
            return;
        }
        unsafe {
            if IsIconic(window) != 0 {
                // Nowhere in particular: a minimised window has nothing under
                // the pointer to hover.
                PostMessageW(window, WM_MOUSEMOVE, 0, 0);
            }
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
            // And no minimise animation at all: closing to the tray should
            // look like closing, the window simply gone, the way Discord
            // does it. It is still really a minimise underneath, because a
            // hidden window gets no redraws and Snag runs from redraws.
            transitions(window, false);
            ShowWindow(window, SW_MINIMIZE);
            IsIconic(window) != 0
        }
    }

    /// Turn Windows' minimise and restore animations off or on for this
    /// window alone. Off only across a trip to the tray and back: the
    /// window's own minimise button keeps its usual animation.
    unsafe fn transitions(window: isize, enabled: bool) {
        #[link(name = "dwmapi")]
        extern "system" {
            fn DwmSetWindowAttribute(
                window: isize,
                attribute: u32,
                value: *const std::ffi::c_void,
                size: u32,
            ) -> i32;
        }
        const DWMWA_TRANSITIONS_FORCEDISABLED: u32 = 3;
        let disabled: i32 = if enabled { 0 } else { 1 };
        DwmSetWindowAttribute(
            window,
            DWMWA_TRANSITIONS_FORCEDISABLED,
            &disabled as *const i32 as *const std::ffi::c_void,
            std::mem::size_of::<i32>() as u32,
        );
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
            // Straight back where it was, rather than rising out of the
            // corner of the screen where a taskbar button would have been.
            transitions(window, false);
            ShowWindow(window, SW_SHOW);
            ShowWindow(window, SW_RESTORE);
            transitions(window, true);
            // Windows often refuses this from a background thread, which is
            // fine: the window is back, it just may not be given focus.
            SetForegroundWindow(window);
            IsWindowVisible(window) != 0 && IsIconic(window) == 0
        }
    }
}
