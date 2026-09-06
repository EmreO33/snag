//! The system tray icon, which is what makes running in the background usable:
//! without it, a hidden window would have no way back.
//!
//! Windows and macOS only. On Linux a tray icon would need
//! libayatana-appindicator present when building, which would add a system
//! dependency to the AppImage for an optional feature, so background mode is
//! simply not offered there.

/// What the user asked for from the tray.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayCommand {
    Show,
    Quit,
}

#[cfg(any(windows, target_os = "macos"))]
mod imp {
    use super::TrayCommand;
    use tray_icon::menu::{Menu, MenuEvent, MenuItem};
    use tray_icon::{TrayIcon, TrayIconBuilder, TrayIconEvent};

    /// Kept alive for as long as the icon should exist: dropping it removes
    /// the icon from the tray.
    pub struct Tray {
        _icon: TrayIcon,
        show_id: tray_icon::menu::MenuId,
        quit_id: tray_icon::menu::MenuId,
    }

    /// Build the tray icon. Returns None if the desktop would not take it,
    /// which is not worth failing over: the window simply stays visible.
    pub fn create() -> Option<Tray> {
        let (width, height, rgba) = crate::icon::tray_rgba()?;
        let image = tray_icon::Icon::from_rgba(rgba, width, height).ok()?;

        let menu = Menu::new();
        let show = MenuItem::new("Show Snag", true, None);
        let quit = MenuItem::new("Quit", true, None);
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

        Some(Tray {
            _icon: icon,
            show_id: show.id().clone(),
            quit_id: quit.id().clone(),
        })
    }

    impl Tray {
        /// Say in the tooltip that something is waiting, since a hidden window
        /// cannot.
        pub fn set_pending(&self, pending: bool) {
            let text = if pending {
                "Snag - a copied link is waiting"
            } else {
                "Snag"
            };
            let _ = self._icon.set_tooltip(Some(text));
        }

        /// Non-blocking: drains whatever the tray has reported since last frame.
        pub fn poll(&self) -> Option<TrayCommand> {
            // A click on the icon itself means "show me", which is what people
            // expect without going through the menu.
            while let Ok(event) = TrayIconEvent::receiver().try_recv() {
                if let TrayIconEvent::DoubleClick { .. } = event {
                    return Some(TrayCommand::Show);
                }
            }
            while let Ok(event) = MenuEvent::receiver().try_recv() {
                if event.id == self.show_id {
                    return Some(TrayCommand::Show);
                }
                if event.id == self.quit_id {
                    return Some(TrayCommand::Quit);
                }
            }
            None
        }
    }
}

#[cfg(not(any(windows, target_os = "macos")))]
mod imp {
    use super::TrayCommand;

    /// Stand-in so the rest of the app needs no platform branches.
    pub struct Tray;

    pub fn create() -> Option<Tray> {
        None
    }

    impl Tray {
        pub fn set_pending(&self, _pending: bool) {}

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
