//! The system tray icon, which is what makes running in the background usable:
//! without it, a hidden window would have no way back.
//!
//! On Windows and macOS through tray-icon. On Linux it is a
//! StatusNotifierItem over D-Bus, which is what KDE, Cinnamon, XFCE, Budgie
//! and GNOME with the AppIndicator extension show; a desktop without any of
//! those (plain GNOME) has no tray, `create` returns None, and Snag keeps its
//! window.

/// What the user asked for from the tray.
///
/// Nothing constructs these where there is no tray, which is expected rather
/// than an oversight.
#[cfg_attr(
    not(any(windows, target_os = "macos", target_os = "linux")),
    allow(dead_code, reason = "no tray on this platform")
)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayCommand {
    Show,
    /// Take whatever link is on the clipboard and download it, without the
    /// window coming up at all. The whole point of living in the tray.
    DownloadCopied,
    Quit,
}

#[cfg(any(windows, target_os = "macos"))]
mod imp {
    use super::TrayCommand;
    use std::sync::mpsc::{channel, Receiver, Sender};
    use tray_icon::menu::{Menu, MenuEvent, MenuItem};
    use tray_icon::{TrayIcon, TrayIconBuilder, TrayIconEvent};

    /// Kept alive for as long as the icon should exist: dropping it removes
    /// the icon from the tray.
    pub struct Tray {
        _icon: TrayIcon,
        /// Commands from the watcher threads, already worked out.
        commands: Receiver<TrayCommand>,
        /// What the tooltip currently says, so it is only rewritten when it
        /// would change rather than on every frame.
        tooltip: std::cell::RefCell<String>,
    }

    /// Menu clicks, turned into commands and handed to the app.
    fn watch_menu(
        tx: Sender<TrayCommand>,
        repaint: impl Fn() + Send + 'static,
        copied: tray_icon::menu::MenuId,
        show: tray_icon::menu::MenuId,
        quit: tray_icon::menu::MenuId,
    ) {
        std::thread::spawn(move || {
            while let Ok(event) = MenuEvent::receiver().recv() {
                let command = if event.id == show {
                    TrayCommand::Show
                } else if event.id == copied {
                    TrayCommand::DownloadCopied
                } else if event.id == quit {
                    TrayCommand::Quit
                } else {
                    // Another tray's menu, or one from a tray that has been
                    // replaced since. Not ours to act on.
                    continue;
                };
                // Bring the window up from here rather than waiting for the
                // app to get round to it: that is the one thing asked for,
                // and it needs nothing from the app.
                if command == TrayCommand::Show {
                    crate::window::restore();
                }
                if tx.send(command).is_err() {
                    return;
                }
                repaint();
            }
        });
    }

    /// Clicks on the icon itself. A left click, once or twice, means "show
    /// me": there is nothing else it could mean. The right button belongs to
    /// the menu.
    fn watch_icon(tx: Sender<TrayCommand>, repaint: impl Fn() + Send + 'static) {
        std::thread::spawn(move || {
            while let Ok(event) = TrayIconEvent::receiver().recv() {
                let wanted = matches!(
                    event,
                    TrayIconEvent::Click {
                        button: tray_icon::MouseButton::Left,
                        button_state: tray_icon::MouseButtonState::Up,
                        ..
                    } | TrayIconEvent::DoubleClick {
                        button: tray_icon::MouseButton::Left,
                        ..
                    }
                );
                if !wanted {
                    continue;
                }
                crate::window::restore();
                if tx.send(TrayCommand::Show).is_err() {
                    return;
                }
                repaint();
            }
        });
    }

    /// Build the tray icon. Returns None if the desktop would not take it,
    /// which is not worth failing over: the window simply stays visible.
    ///
    /// `repaint` is how the tray reaches the app. It has to: clicking a tray
    /// menu item sends nothing the window's event loop listens for, so an
    /// idle Snag would sit there with the click in a queue nobody reads, and
    /// the menu would appear to do nothing at all. That was the bug this
    /// argument exists to fix.
    pub fn create(repaint: impl Fn() + Send + Clone + 'static) -> Option<Tray> {
        let (width, height, rgba) = crate::icon::tray_rgba()?;
        let image = tray_icon::Icon::from_rgba(rgba, width, height).ok()?;

        let menu = Menu::new();
        let copied = MenuItem::new("Download what I copied", true, None);
        let show = MenuItem::new("Show Snag", true, None);
        let quit = MenuItem::new("Quit", true, None);
        menu.append(&copied).ok()?;
        menu.append(&show).ok()?;
        menu.append(&tray_icon::menu::PredefinedMenuItem::separator())
            .ok()?;
        menu.append(&quit).ok()?;

        let icon = TrayIconBuilder::new()
            .with_tooltip("Snag")
            // A left click shows Snag, as it does for Discord and most tray
            // apps; the menu is on the right button. tray-icon opens the
            // menu on either button unless told otherwise, so a left click
            // never reached the "show me" it was meant to be.
            .with_menu_on_left_click(false)
            .with_icon(image)
            .with_menu(Box::new(menu))
            .build()
            .ok()?;

        // Two threads, each asleep on one of the tray's channels until
        // something happens, turning it into a command and waking the app.
        // They stop when the app stops listening, which is what dropping the
        // tray does.
        let (tx, commands) = channel();
        watch_menu(
            tx.clone(),
            repaint.clone(),
            copied.id().clone(),
            show.id().clone(),
            quit.id().clone(),
        );
        watch_icon(tx, repaint);

        Some(Tray {
            _icon: icon,
            commands,
            tooltip: std::cell::RefCell::new("Snag".to_string()),
        })
    }

    impl Tray {
        /// Say what Snag is up to, since a hidden window cannot: a copied
        /// link waiting to be taken, or downloads in flight.
        pub fn set_status(&self, pending: bool, active: usize) {
            let text = super::status_text(pending, active);
            let mut current = self.tooltip.borrow_mut();
            if *current != text {
                let _ = self._icon.set_tooltip(Some(&text));
                *current = text;
            }
        }

        /// Non-blocking: whatever the tray asked for since the last frame.
        pub fn poll(&self) -> Option<TrayCommand> {
            self.commands.try_recv().ok()
        }
    }
}

#[cfg(target_os = "linux")]
mod imp {
    use super::TrayCommand;
    use ksni::blocking::{Handle, TrayMethods};
    use std::sync::mpsc::{channel, Receiver, Sender};

    /// The item the desktop's tray asks about over D-Bus. It lives on
    /// ksni's own thread, and answers clicks there.
    struct Item {
        tx: Sender<TrayCommand>,
        repaint: Box<dyn Fn() + Send>,
        icon: Vec<ksni::Icon>,
        status: String,
    }

    impl Item {
        /// Hand a command to the app and wake it: a hidden window has no
        /// other reason to run a frame.
        fn send(&self, command: TrayCommand) {
            if self.tx.send(command).is_ok() {
                (self.repaint)();
            }
        }
    }

    impl ksni::Tray for Item {
        fn id(&self) -> String {
            "io.github.EmreO33.Snag".into()
        }

        fn title(&self) -> String {
            "Snag".into()
        }

        fn icon_pixmap(&self) -> Vec<ksni::Icon> {
            self.icon.clone()
        }

        fn tool_tip(&self) -> ksni::ToolTip {
            ksni::ToolTip {
                title: self.status.clone(),
                ..Default::default()
            }
        }

        /// A left click on the icon: show Snag, as on Windows.
        fn activate(&mut self, _x: i32, _y: i32) {
            self.send(TrayCommand::Show);
        }

        fn menu(&self) -> Vec<ksni::MenuItem<Self>> {
            use ksni::menu::StandardItem;
            let item = |label: &str, command: TrayCommand| {
                StandardItem {
                    label: label.into(),
                    activate: Box::new(move |this: &mut Self| this.send(command)),
                    ..Default::default()
                }
                .into()
            };
            vec![
                item("Download what I copied", TrayCommand::DownloadCopied),
                item("Show Snag", TrayCommand::Show),
                ksni::MenuItem::Separator,
                item("Quit", TrayCommand::Quit),
            ]
        }
    }

    /// Kept alive for as long as the icon should exist: dropping it takes
    /// the icon out of the tray.
    pub struct Tray {
        handle: Handle<Item>,
        commands: Receiver<TrayCommand>,
        tooltip: std::cell::RefCell<String>,
    }

    /// The icon in the sizes a tray draws at, largest last. A tray given only
    /// the 256 pixel original scales it down itself, some of them badly.
    fn icons() -> Option<Vec<ksni::Icon>> {
        let (width, height, rgba) = crate::icon::tray_rgba()?;
        let full = image::RgbaImage::from_raw(width, height, rgba)?;
        let icons = [22, 32, 48, 64, width]
            .into_iter()
            .filter(|&size| size <= width)
            .map(|size| {
                let image = if size == width {
                    full.clone()
                } else {
                    image::imageops::resize(
                        &full,
                        size,
                        size * height / width,
                        image::imageops::FilterType::Lanczos3,
                    )
                };
                let (w, h) = image.dimensions();
                let mut data = image.into_raw();
                // RGBA to the ARGB the specification asks for.
                for pixel in data.as_chunks_mut::<4>().0 {
                    pixel.rotate_right(1);
                }
                ksni::Icon {
                    width: w as i32,
                    height: h as i32,
                    data,
                }
            })
            .collect();
        Some(icons)
    }

    /// Build the tray icon. None when the desktop has no tray to put it in,
    /// in which case Snag keeps its window.
    pub fn create(repaint: impl Fn() + Send + Clone + 'static) -> Option<Tray> {
        let (tx, commands) = channel();
        let item = Item {
            tx,
            repaint: Box::new(repaint),
            icon: icons()?,
            status: "Snag".into(),
        };
        // Inside the Flatpak the item goes by its connection's own name
        // rather than claiming the specification's well-known one, which
        // the sandbox does not allow. Every tray host accepts either.
        let in_flatpak = std::env::var_os("FLATPAK_ID").is_some();
        let handle = item.disable_dbus_name(in_flatpak).spawn().ok()?;
        Some(Tray {
            handle,
            commands,
            tooltip: std::cell::RefCell::new("Snag".into()),
        })
    }

    impl Tray {
        pub fn set_status(&self, pending: bool, active: usize) {
            let text = super::status_text(pending, active);
            let mut current = self.tooltip.borrow_mut();
            if *current != text {
                self.handle.update(|item| item.status = text.clone());
                *current = text;
            }
        }

        pub fn poll(&self) -> Option<TrayCommand> {
            self.commands.try_recv().ok()
        }
    }

    impl Drop for Tray {
        fn drop(&mut self) {
            // Unlike the other platforms' icon, this one outlives its handle
            // unless it is told to go.
            self.handle.shutdown().wait();
        }
    }
}

#[cfg(not(any(windows, target_os = "macos", target_os = "linux")))]
mod imp {
    use super::TrayCommand;

    /// Stand-in so the rest of the app needs no platform branches.
    pub struct Tray;

    pub fn create(_repaint: impl Fn() + Send + Clone + 'static) -> Option<Tray> {
        None
    }

    impl Tray {
        pub fn set_status(&self, _pending: bool, _active: usize) {}

        pub fn poll(&self) -> Option<TrayCommand> {
            None
        }
    }
}

pub use imp::{create, Tray};

/// Whether this build can put an icon in the tray at all. On Linux that is
/// only a maybe: whether the desktop shows one is known once `create` tries.
pub fn supported() -> bool {
    cfg!(any(windows, target_os = "macos", target_os = "linux"))
}

/// The tooltip: a copied link waiting to be taken, or downloads in flight.
#[cfg_attr(
    not(any(windows, target_os = "macos", target_os = "linux")),
    allow(dead_code, reason = "no tray on this platform")
)]
fn status_text(pending: bool, active: usize) -> String {
    match (pending, active) {
        (true, _) => "Snag - a copied link is waiting".to_string(),
        (false, 0) => "Snag".to_string(),
        (false, 1) => "Snag - 1 download running".to_string(),
        (false, n) => format!("Snag - {n} downloads running"),
    }
}
