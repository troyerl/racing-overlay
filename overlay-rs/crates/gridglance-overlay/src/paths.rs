//! Per-user data paths matching Python `overlay.paths`.

use std::path::PathBuf;

const APP_DIR: &str = "GridGlance";

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

pub fn data_dir() -> PathBuf {
    let d = user_base().join(APP_DIR);
    let _ = std::fs::create_dir_all(&d);
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
