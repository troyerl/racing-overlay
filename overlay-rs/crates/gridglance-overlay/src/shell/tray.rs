//! System tray menu (Settings / Track Scan / Start-Stop / Edit / Updates / Quit).

use std::sync::mpsc::{self, Receiver, TryRecvError};

use tray_icon::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{TrayIcon, TrayIconBuilder};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayCommand {
    Settings,
    TrackScan,
    ToggleOverlay,
    ToggleEdit,
    CheckUpdates,
    Quit,
}

/// Spawn the tray icon. Returns a receiver for menu clicks + the icon handle.
pub fn spawn() -> anyhow::Result<(Receiver<TrayCommand>, TrayIcon)> {
    let (tx, rx) = mpsc::channel();

    let menu = Menu::new();
    let settings = MenuItem::new("Settings", true, None);
    let track_scan = MenuItem::new("Track Scan", true, None);
    let toggle = MenuItem::new("Start / Stop overlay", true, None);
    let edit = MenuItem::new("Edit layout", true, None);
    let updates = MenuItem::new("Check for updates", true, None);
    let quit = MenuItem::new("Quit", true, None);
    menu.append(&settings)?;
    menu.append(&track_scan)?;
    menu.append(&PredefinedMenuItem::separator())?;
    menu.append(&toggle)?;
    menu.append(&edit)?;
    menu.append(&PredefinedMenuItem::separator())?;
    menu.append(&updates)?;
    menu.append(&PredefinedMenuItem::separator())?;
    menu.append(&quit)?;

    let id_settings = settings.id().clone();
    let id_scan = track_scan.id().clone();
    let id_toggle = toggle.id().clone();
    let id_edit = edit.id().clone();
    let id_updates = updates.id().clone();
    let id_quit = quit.id().clone();

    let menu_rx = MenuEvent::receiver();
    std::thread::spawn(move || {
        while let Ok(ev) = menu_rx.recv() {
            let cmd = if ev.id == id_settings {
                Some(TrayCommand::Settings)
            } else if ev.id == id_scan {
                Some(TrayCommand::TrackScan)
            } else if ev.id == id_toggle {
                Some(TrayCommand::ToggleOverlay)
            } else if ev.id == id_edit {
                Some(TrayCommand::ToggleEdit)
            } else if ev.id == id_updates {
                Some(TrayCommand::CheckUpdates)
            } else if ev.id == id_quit {
                Some(TrayCommand::Quit)
            } else {
                None
            };
            if let Some(cmd) = cmd {
                if tx.send(cmd).is_err() {
                    break;
                }
            }
        }
    });

    let icon = crate::app_icon::tray_icon();
    let tray = TrayIconBuilder::new()
        .with_menu(Box::new(menu))
        .with_tooltip("GridGlance")
        .with_icon(icon)
        .build()?;
    Ok((rx, tray))
}

/// Non-blocking poll of tray menu commands.
pub fn poll_events(rx: &Receiver<TrayCommand>) -> Vec<TrayCommand> {
    let mut out = Vec::new();
    loop {
        match rx.try_recv() {
            Ok(c) => out.push(c),
            Err(TryRecvError::Empty) => break,
            Err(TryRecvError::Disconnected) => break,
        }
    }
    out
}
