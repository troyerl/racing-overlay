//! Branded app icon for taskbar, Settings window, and system tray.

use once_cell::sync::OnceCell;
use std::path::{Path, PathBuf};

static ICON: OnceCell<Option<AppIconRgba>> = OnceCell::new();

pub struct AppIconRgba {
    pub rgba: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

const ASSET_NAMES: &[&str] = &[
    "assets/app.ico",
    "assets/app.png",
    "assets/icon.png",
    "app.ico",
    "app.png",
];

/// Windows taskbar grouping — must run before any window is created.
#[cfg(windows)]
pub fn set_windows_app_user_model_id() {
    use windows::core::w;
    use windows::Win32::UI::Shell::SetCurrentProcessExplicitAppUserModelID;
    unsafe {
        let _ = SetCurrentProcessExplicitAppUserModelID(w!("GridGlance.App"));
    }
}

#[cfg(not(windows))]
pub fn set_windows_app_user_model_id() {}

fn search_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            roots.push(dir.to_path_buf());
            // Installed layout may ship assets/ under the app dir.
            roots.push(dir.join("assets"));
            // Dev: overlay-rs/target/{debug,release} → repo root is ../../../
            roots.push(dir.join("..").join("..").join(".."));
        }
    }
    if let Ok(cwd) = std::env::current_dir() {
        roots.push(cwd.clone());
        for anc in cwd.ancestors().take(6) {
            roots.push(anc.to_path_buf());
        }
    }
    // Crate dir → repo root (overlay-rs/crates/gridglance-overlay → ../../../)
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    roots.push(manifest.join("..").join("..").join(".."));
    roots
}

fn find_asset_path() -> Option<PathBuf> {
    for root in search_roots() {
        for rel in ASSET_NAMES {
            let p = if rel.starts_with("assets/") {
                root.join(rel)
            } else {
                root.join(rel)
            };
            if p.is_file() {
                return Some(p);
            }
        }
    }
    None
}

fn decode_file(path: &Path) -> anyhow::Result<AppIconRgba> {
    let img = image::open(path)?;
    let rgba = img.to_rgba8();
    let (width, height) = rgba.dimensions();
    Ok(AppIconRgba {
        rgba: rgba.into_raw(),
        width,
        height,
    })
}

fn load_inner() -> Option<AppIconRgba> {
    let path = find_asset_path()?;
    match decode_file(&path) {
        Ok(icon) => Some(icon),
        Err(e) => {
            eprintln!(
                "[gridglance] app icon {}: {e}",
                path.display()
            );
            None
        }
    }
}

pub fn load() -> Option<&'static AppIconRgba> {
    ICON.get_or_init(load_inner).as_ref()
}

pub fn egui_icon() -> Option<egui::IconData> {
    let icon = load()?;
    Some(egui::IconData {
        rgba: icon.rgba.clone(),
        width: icon.width,
        height: icon.height,
    })
}

pub fn tray_icon() -> tray_icon::Icon {
    if let Some(icon) = load() {
        if let Ok(t) = tray_icon::Icon::from_rgba(icon.rgba.clone(), icon.width, icon.height) {
            return t;
        }
    }
    fallback_tray_icon()
}

fn fallback_tray_icon() -> tray_icon::Icon {
    let size = 32u32;
    let mut rgba = vec![0u8; (size * size * 4) as usize];
    for y in 0..size {
        for x in 0..size {
            let i = ((y * size + x) * 4) as usize;
            let edge = x < 2 || y < 2 || x >= size - 2 || y >= size - 2;
            if edge {
                rgba[i] = 0x0d;
                rgba[i + 1] = 0x0f;
                rgba[i + 2] = 0x12;
                rgba[i + 3] = 255;
            } else {
                rgba[i] = 0x46;
                rgba[i + 1] = 0xdf;
                rgba[i + 2] = 0x7a;
                rgba[i + 3] = 255;
            }
        }
    }
    tray_icon::Icon::from_rgba(rgba, size, size).expect("fallback tray icon")
}
