//! Load `.env` into the process environment (dev convenience).

use std::fs;
use std::path::PathBuf;

/// Populate `std::env` from local `.env` files; existing vars win.
pub fn load_dotenv() {
    let mut candidates = Vec::new();
    // Packaged installs: `%LOCALAPPDATA%\GridGlance\.env`
    candidates.push(crate::paths::data_dir().join(".env"));
    if let Ok(cwd) = std::env::current_dir() {
        candidates.push(cwd.join(".env"));
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            candidates.push(dir.join(".env"));
            // repo root when running from target/debug
            candidates.push(dir.join("../../../.env"));
            candidates.push(dir.join("../../../../.env"));
        }
    }
    // Walk up from CWD looking for repo `.env`.
    if let Ok(cwd) = std::env::current_dir() {
        for anc in cwd.ancestors().take(6) {
            candidates.push(anc.join(".env"));
        }
    }

    let mut seen = std::collections::HashSet::<PathBuf>::new();
    for path in candidates {
        let Ok(canon) = path.canonicalize() else {
            if !path.is_file() {
                continue;
            }
            // uncanonicalized but exists
            apply_env_file(&path);
            continue;
        };
        if !seen.insert(canon.clone()) {
            continue;
        }
        if canon.is_file() {
            apply_env_file(&canon);
        }
    }
}

fn apply_env_file(path: &std::path::Path) {
    let Ok(text) = fs::read_to_string(path) else {
        return;
    };
    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') || !line.contains('=') {
            continue;
        }
        let mut parts = line.splitn(2, '=');
        let mut key = parts.next().unwrap_or("").trim();
        if let Some(rest) = key.strip_prefix("export ") {
            key = rest.trim();
        }
        let mut val = parts.next().unwrap_or("").trim();
        if (val.starts_with('"') && val.ends_with('"'))
            || (val.starts_with('\'') && val.ends_with('\''))
        {
            val = &val[1..val.len() - 1];
        }
        if key.is_empty() {
            continue;
        }
        if std::env::var_os(key).is_none() {
            std::env::set_var(key, val);
        }
    }
}
