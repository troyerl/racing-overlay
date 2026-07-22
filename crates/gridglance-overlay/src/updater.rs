//! App version + GitHub release updater (Python `version` / `updater` parity).

use serde::Deserialize;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// `"owner/name"` — empty disables update checks (dev builds).
/// Packaged/CI builds can stamp via `GRIDGLANCE_GITHUB_REPO=owner/name`.
pub const GITHUB_REPO: &str = match option_env!("GRIDGLANCE_GITHUB_REPO") {
    Some(r) => r,
    None => "",
};

#[derive(Debug, Clone, Deserialize)]
pub struct ReleaseInfo {
    pub version: String,
    pub url: Option<String>,
    #[allow(dead_code)]
    pub notes: String,
    #[allow(dead_code)]
    pub name: String,
}

pub fn is_newer(remote: &str, current: &str) -> bool {
    let parse = |v: &str| -> Vec<u64> {
        v.split(|c: char| !c.is_ascii_digit())
            .filter(|s| !s.is_empty())
            .filter_map(|s| s.parse().ok())
            .collect()
    };
    let r = parse(remote);
    let c = parse(current);
    if !r.is_empty() && !c.is_empty() {
        return r > c;
    }
    !remote.is_empty() && remote != current
}

pub fn fetch_latest(timeout_secs: u64) -> anyhow::Result<Option<ReleaseInfo>> {
    if GITHUB_REPO.is_empty() {
        return Ok(None);
    }
    let agent = ureq::builder()
        .timeout(std::time::Duration::from_secs(timeout_secs))
        .user_agent("GridGlance-Updater")
        .build();
    let base = format!("https://api.github.com/repos/{GITHUB_REPO}");
    let data: serde_json::Value = match agent
        .get(&format!("{base}/releases/latest"))
        .set("Accept", "application/vnd.github+json")
        .call()
    {
        Ok(resp) => resp.into_json()?,
        Err(ureq::Error::Status(404, _)) => {
            let list: Vec<serde_json::Value> = agent
                .get(&format!("{base}/releases"))
                .set("Accept", "application/vnd.github+json")
                .call()?
                .into_json()?;
            let published = list
                .into_iter()
                .find(|r| r.get("draft").and_then(|d| d.as_bool()) != Some(true));
            match published {
                Some(d) => d,
                None => return Ok(None),
            }
        }
        Err(e) => return Err(e.into()),
    };
    Ok(Some(release_info(&data)))
}

fn release_info(data: &serde_json::Value) -> ReleaseInfo {
    let tag = data
        .get("tag_name")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim_start_matches(['v', 'V'])
        .to_string();
    let mut asset_url = None;
    if let Some(assets) = data.get("assets").and_then(|a| a.as_array()) {
        for a in assets {
            let name = a.get("name").and_then(|n| n.as_str()).unwrap_or("");
            if name.to_ascii_lowercase().ends_with(".exe") {
                asset_url = a
                    .get("browser_download_url")
                    .and_then(|u| u.as_str())
                    .map(|s| s.to_string());
                break;
            }
        }
    }
    ReleaseInfo {
        version: tag,
        url: asset_url,
        notes: data
            .get("body")
            .and_then(|b| b.as_str())
            .unwrap_or("")
            .to_string(),
        name: data
            .get("name")
            .and_then(|n| n.as_str())
            .unwrap_or("")
            .to_string(),
    }
}

pub fn download_installer(url: &str) -> anyhow::Result<std::path::PathBuf> {
    let tmp = std::env::temp_dir().join(format!("GridGlanceSetup-{}.exe", std::process::id()));
    let resp = ureq::get(url)
        .set("User-Agent", "GridGlance-Updater")
        .call()?;
    let mut reader = resp.into_reader();
    let mut file = std::fs::File::create(&tmp)?;
    std::io::copy(&mut reader, &mut file)?;
    Ok(tmp)
}

/// Launch Inno uninstaller when installed; no-op in a cargo run checkout.
pub fn launch_uninstaller() -> anyhow::Result<()> {
    #[cfg(windows)]
    {
        if let Ok(exe) = std::env::current_exe() {
            if let Some(dir) = exe.parent() {
                let unins = dir.join("unins000.exe");
                if unins.is_file() {
                    std::process::Command::new(unins).spawn()?;
                    return Ok(());
                }
            }
        }
        anyhow::bail!("Uninstaller not found (dev build?)");
    }
    #[cfg(not(windows))]
    {
        anyhow::bail!("Uninstall is only available on Windows installs");
    }
}
