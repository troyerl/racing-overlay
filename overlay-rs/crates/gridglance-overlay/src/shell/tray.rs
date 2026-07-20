//! System tray menu (Settings / Track Scan / Start-Stop / Edit / Updates / Quit).
//!
//! On Windows, tray-icon's built-in `.with_menu()` popup does not deliver
//! `MenuEvent`s (no muda subclass / no TPM_RETURNCMD). We show the menu
//! ourselves via `ContextMenu::show_context_menu_for_hwnd` and poll events
//! on the UI thread.

use tray_icon::menu::{ContextMenu, Menu, MenuEvent, MenuId, MenuItem, PredefinedMenuItem};
use tray_icon::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayCommand {
    Settings,
    TrackScan,
    ToggleOverlay,
    ToggleEdit,
    CheckUpdates,
    Quit,
}

struct MenuIds {
    settings: MenuId,
    scan: MenuId,
    toggle: MenuId,
    edit: MenuId,
    updates: MenuId,
    quit: MenuId,
}

impl MenuIds {
    fn command(&self, id: &MenuId) -> Option<TrayCommand> {
        if id == &self.settings {
            Some(TrayCommand::Settings)
        } else if id == &self.scan {
            Some(TrayCommand::TrackScan)
        } else if id == &self.toggle {
            Some(TrayCommand::ToggleOverlay)
        } else if id == &self.edit {
            Some(TrayCommand::ToggleEdit)
        } else if id == &self.updates {
            Some(TrayCommand::CheckUpdates)
        } else if id == &self.quit {
            Some(TrayCommand::Quit)
        } else {
            None
        }
    }
}

/// Owns the tray icon + menu so they stay alive for the process lifetime.
pub struct TrayHandle {
    _icon: TrayIcon,
    menu: Menu,
    ids: MenuIds,
    /// HWND used as owner for TrackPopupMenu (message-only window on Windows).
    menu_hwnd: isize,
    _items: Vec<MenuItem>,
    _separators: Vec<PredefinedMenuItem>,
}

impl TrayHandle {
    /// Poll tray icon / menu events. Must run on the UI thread (message loop).
    pub fn poll(&self) -> Vec<TrayCommand> {
        let mut out = Vec::new();

        while let Ok(ev) = TrayIconEvent::receiver().try_recv() {
            match &ev {
                TrayIconEvent::DoubleClick {
                    button: MouseButton::Left,
                    ..
                }
                | TrayIconEvent::Click {
                    button: MouseButton::Left,
                    button_state: MouseButtonState::Up,
                    ..
                } => out.push(TrayCommand::Settings),
                TrayIconEvent::Click {
                    button: MouseButton::Right,
                    button_state: MouseButtonState::Down,
                    ..
                } => {
                    // Blocks until the user picks an item or dismisses; MenuEvent
                    // is enqueued synchronously via TPM_RETURNCMD.
                    #[cfg(windows)]
                    unsafe {
                        self.menu
                            .show_context_menu_for_hwnd(self.menu_hwnd, None);
                    }
                    #[cfg(not(windows))]
                    {
                        let _ = self.menu_hwnd;
                    }
                }
                _ => {}
            }
        }

        while let Ok(ev) = MenuEvent::receiver().try_recv() {
            if let Some(cmd) = self.ids.command(&ev.id) {
                out.push(cmd);
            }
        }

        out
    }
}

/// Spawn the tray icon. Returns a handle that must be kept for the process lifetime.
pub fn spawn() -> anyhow::Result<TrayHandle> {
    let menu = Menu::new();
    let settings = MenuItem::new("Settings", true, None);
    let track_scan = MenuItem::new("Track Scan", true, None);
    let toggle = MenuItem::new("Start / Stop overlay", true, None);
    let edit = MenuItem::new("Edit layout", true, None);
    let updates = MenuItem::new("Check for updates", true, None);
    let quit = MenuItem::new("Quit", true, None);

    let sep1 = PredefinedMenuItem::separator();
    let sep2 = PredefinedMenuItem::separator();
    let sep3 = PredefinedMenuItem::separator();

    menu.append(&settings)?;
    menu.append(&track_scan)?;
    menu.append(&sep1)?;
    menu.append(&toggle)?;
    menu.append(&edit)?;
    menu.append(&sep2)?;
    menu.append(&updates)?;
    menu.append(&sep3)?;
    menu.append(&quit)?;

    let ids = MenuIds {
        settings: settings.id().clone(),
        scan: track_scan.id().clone(),
        toggle: toggle.id().clone(),
        edit: edit.id().clone(),
        updates: updates.id().clone(),
        quit: quit.id().clone(),
    };

    let items = vec![settings, track_scan, toggle, edit, updates, quit];
    let separators = vec![sep1, sep2, sep3];

    let menu_hwnd = create_menu_owner_hwnd()?;

    let icon = crate::app_icon::tray_icon();
    // Menu is shown manually on right-click (see `poll`) so MenuEvents fire.
    let tray = TrayIconBuilder::new()
        .with_tooltip("GridGlance")
        .with_icon(icon)
        .with_menu_on_left_click(false)
        .build()?;

    Ok(TrayHandle {
        _icon: tray,
        menu,
        ids,
        menu_hwnd,
        _items: items,
        _separators: separators,
    })
}

#[cfg(windows)]
fn create_menu_owner_hwnd() -> anyhow::Result<isize> {
    use std::sync::OnceLock;
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
    use windows::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DefWindowProcW, RegisterClassW, CS_HREDRAW, CS_VREDRAW, HWND_MESSAGE,
        WNDCLASSW, WS_EX_TOOLWINDOW, WS_POPUP,
    };

    unsafe extern "system" fn wnd_proc(
        hwnd: HWND,
        msg: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
    }

    static CLASS: OnceLock<Vec<u16>> = OnceLock::new();
    let class_name = CLASS.get_or_init(|| {
        let name: Vec<u16> = "GridGlanceTrayMenuOwner\0".encode_utf16().collect();
        let wc = WNDCLASSW {
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(wnd_proc),
            hInstance: windows::Win32::Foundation::HINSTANCE::default(),
            lpszClassName: PCWSTR(name.as_ptr()),
            ..Default::default()
        };
        unsafe {
            let _ = RegisterClassW(&wc);
        }
        name
    });

    unsafe {
        let hwnd = CreateWindowExW(
            WS_EX_TOOLWINDOW,
            PCWSTR(class_name.as_ptr()),
            PCWSTR::null(),
            WS_POPUP,
            0,
            0,
            0,
            0,
            Some(HWND_MESSAGE),
            None,
            None,
            None,
        )?;
        if hwnd == HWND::default() || hwnd.0.is_null() {
            anyhow::bail!("failed to create tray menu owner HWND");
        }
        Ok(hwnd.0 as isize)
    }
}

#[cfg(not(windows))]
fn create_menu_owner_hwnd() -> anyhow::Result<isize> {
    Ok(0)
}
