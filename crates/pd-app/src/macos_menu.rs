//! The macOS menu bar (SPEC-Rust §15.1): the standard application menu, with
//! About, Preferences and Quit where Mac users look for them, and a Window
//! menu. egui draws the app's other menus inside the window.
//!
//! Quit is the app's own item, not the system's: the system's ends the app at
//! once, while this one asks about unsaved changes, as closing the window
//! does. There is no native Edit menu: its key equivalents (Cmd+C, Cmd+V,
//! Cmd+Z, Cmd+A) would be taken by the menu before egui saw them.

use std::sync::Mutex;
use std::sync::mpsc::{Receiver, channel};

use eframe::egui;
use muda::accelerator::{Accelerator, Code, Modifiers};
use muda::{Menu, MenuEvent, MenuId, MenuItem, PredefinedMenuItem, Submenu};

/// What the menu bar asks the app to do.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuCommand {
    About,
    Preferences,
    Quit,
}

pub struct MacMenu {
    /// Kept alive for as long as the menu is shown.
    _bar: Menu,
    events: Receiver<MenuEvent>,
    about: MenuId,
    preferences: MenuId,
    quit: MenuId,
}

impl MacMenu {
    /// Builds the menu bar and gives it to the application. `None` if that
    /// fails; the in-window menus still work.
    pub fn install(ctx: &egui::Context) -> Option<Self> {
        let cmd = |code| Some(Accelerator::new(Modifiers::META, code));
        let about = MenuItem::new("About PathwayDesigner", true, None);
        let preferences = MenuItem::new("Settings…", true, cmd(Code::Comma));
        let quit = MenuItem::new("Quit PathwayDesigner", true, cmd(Code::KeyQ));
        let app = Submenu::new("PathwayDesigner", true);
        app.append_items(&[
            &about,
            &PredefinedMenuItem::separator(),
            &preferences,
            &PredefinedMenuItem::separator(),
            &PredefinedMenuItem::services(None),
            &PredefinedMenuItem::separator(),
            &PredefinedMenuItem::hide(None),
            &PredefinedMenuItem::hide_others(None),
            &PredefinedMenuItem::show_all(None),
            &PredefinedMenuItem::separator(),
            &quit,
        ])
        .ok()?;
        let window = Submenu::new("Window", true);
        window
            .append_items(&[
                &PredefinedMenuItem::minimize(None),
                &PredefinedMenuItem::maximize(None),
                &PredefinedMenuItem::separator(),
                &PredefinedMenuItem::fullscreen(None),
            ])
            .ok()?;
        let bar = Menu::new();
        bar.append_items(&[&app, &window]).ok()?;
        bar.init_for_nsapp();
        window.set_as_windows_menu_for_nsapp();

        // Menu events arrive between frames: pass them on, and wake egui.
        let (tx, rx) = channel();
        let tx = Mutex::new(tx);
        let ctx = ctx.clone();
        MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
            if let Ok(tx) = tx.lock() {
                let _ = tx.send(event);
            }
            ctx.request_repaint();
        }));
        Some(MacMenu {
            _bar: bar,
            events: rx,
            about: about.id().clone(),
            preferences: preferences.id().clone(),
            quit: quit.id().clone(),
        })
    }

    /// The next command chosen from the menu bar, if any.
    pub fn poll(&self) -> Option<MenuCommand> {
        while let Ok(event) = self.events.try_recv() {
            let id = event.id();
            if *id == self.about {
                return Some(MenuCommand::About);
            }
            if *id == self.preferences {
                return Some(MenuCommand::Preferences);
            }
            if *id == self.quit {
                return Some(MenuCommand::Quit);
            }
        }
        None
    }
}
