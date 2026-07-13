//! Per-user data paths matching Python `overlay.paths`.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

const APP_DIR: &str = "GridGlance";
const OLD_APP_DIRS: &[&str] = &["Racing Overlay"];
const LEGACY_FILES: &[&str] = &[
    "overlay_config.json",
    "overlay_layout.json",
    "lap_compare_best.json",
];

static MIGRATED: AtomicBool = AtomicBool::new(false);

fn user_base() -> PathBuf {
    if cfg!(windows) {
        std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .or_else(|| directories::UserDirs::new().map(|u| u.home_dir().to_path_buf()))
            .unwrap_or_else(|| PathBuf::from("."))
    } else if cfg!(target_os = "macos") {
        directories::UserDirs::new()
            .map(|u| u.home_dir().join("Library").join("Application Support"))
            .unwrap_or_else(|| PathBuf::from("."))
    } else {
        std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .or_else(|| {
                directories::UserDirs::new().map(|u| u.home_dir().join(".local").join("share"))
            })
            .unwrap_or_else(|| PathBuf::from("."))
    }
}

fn copy_file_if_missing(old: &Path, new: &Path) {
    if old.is_file() && !new.exists() {
        if let Some(parent) = new.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::copy(old, new);
    }
}

/// One-shot migrate matching Python `_migrate_legacy`.
fn migrate_legacy(dst: &Path) {
    if MIGRATED.swap(true, Ordering::SeqCst) {
        return;
    }

    for old_name in OLD_APP_DIRS {
        let old_dir = user_base().join(old_name);
        if !old_dir.is_dir() {
            continue;
        }
        if old_dir.canonicalize().ok() == dst.canonicalize().ok() {
            continue;
        }
        for name in LEGACY_FILES {
            copy_file_if_missing(&old_dir.join(name), &dst.join(name));
        }
        let old_tracks = old_dir.join("tracks");
        if old_tracks.is_dir() {
            let new_tracks = dst.join("tracks");
            let _ = std::fs::create_dir_all(&new_tracks);
            if let Ok(entries) = std::fs::read_dir(&old_tracks) {
                for entry in entries.flatten() {
                    let src = entry.path();
                    if src.is_file() {
                        if let Some(fname) = src.file_name() {
                            copy_file_if_missing(&src, &new_tracks.join(fname));
                        }
                    }
                }
            }
        }
    }

    // Legacy repo-root files (dev checkout next to scripts).
    if let Ok(cwd) = std::env::current_dir() {
        for ancestor in cwd.ancestors().take(6) {
            if ancestor == dst {
                continue;
            }
            let mut found = false;
            for name in LEGACY_FILES {
                let old = ancestor.join(name);
                if old.is_file() {
                    copy_file_if_missing(&old, &dst.join(name));
                    found = true;
                }
            }
            if found {
                break;
            }
        }
    }
}

pub fn data_dir() -> PathBuf {
    let d = user_base().join(APP_DIR);
    let _ = std::fs::create_dir_all(&d);
    migrate_legacy(&d);
    d
}

pub fn config_path() -> PathBuf {
    data_dir().join("overlay_config.json")
}

pub fn tracks_dir() -> PathBuf {
    let d = data_dir().join("tracks");
    let _ = std::fs::create_dir_all(&d);
    d
}

pub fn layout_path() -> PathBuf {
    data_dir().join("overlay_layout.json")
}
