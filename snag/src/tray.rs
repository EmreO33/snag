//! The system tray icon, which is what makes running in the background usable:
//! without it, a hidden window would have no way back.
//!
//! Windows and macOS only. On Linux a tray icon would need
//! libayatana-appindicator present when building, which would add a system
//! dependency to the AppImage for an optional feature, so background mode is
//! simply not offered there.

/// What the user asked for from the tray.
///
/// Nothing constructs these where there is no tray, which is expected rather
/// than an oversight.
#[cfg_attr(
    not(any(windows, target_os = "macos")),
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
            let text = match (pending, active) {
                (true, _) => "Snag - a copied link is waiting".to_string(),
                (false, 0) => "Snag".to_string(),
                (false, 1) => "Snag - 1 download running".to_string(),
                (false, n) => format!("Snag - {n} downloads running"),
            };
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

#[cfg(not(any(windows, target_os = "macos")))]
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

/// Whether this build can put an icon in the tray at all.
pub fn supported() -> bool {
    cfg!(any(windows, target_os = "macos"))
}
