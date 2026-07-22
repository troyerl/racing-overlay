//! Launch-at-login (Windows Startup shortcut). Other platforms are no-ops.

#[cfg(windows)]
mod win {
    use std::path::PathBuf;
    use std::process::Command;

    const SHORTCUT_NAME: &str = "GridGlance.lnk";

    fn startup_dir() -> Option<PathBuf> {
        let appdata = std::env::var_os("APPDATA")?;
        Some(
            PathBuf::from(appdata)
                .join("Microsoft")
                .join("Windows")
                .join("Start Menu")
                .join("Programs")
                .join("Startup"),
        )
    }

    pub fn shortcut_path() -> Option<PathBuf> {
        Some(startup_dir()?.join(SHORTCUT_NAME))
    }

    #[allow(dead_code)]
    pub fn is_enabled() -> bool {
        shortcut_path().map(|p| p.is_file()).unwrap_or(false)
    }

    pub fn set_enabled(on: bool, arguments: &str) -> anyhow::Result<()> {
        let path = shortcut_path().ok_or_else(|| anyhow::anyhow!("no Startup folder"))?;
        if !on {
            if path.is_file() {
                std::fs::remove_file(&path)?;
            }
            return Ok(());
        }
        let target = std::env::current_exe()?;
        let target_s = target.to_string_lossy().replace('\'', "''");
        let args_s = arguments.replace('\'', "''");
        let path_s = path.to_string_lossy().replace('\'', "''");
        let work = target
            .parent()
            .map(|p| p.to_string_lossy().replace('\'', "''"))
            .unwrap_or_default();
        let ps = format!(
            "$ws = New-Object -ComObject WScript.Shell; \
             $s = $ws.CreateShortcut('{path_s}'); \
             $s.TargetPath = '{target_s}'; \
             $s.Arguments = '{args_s}'; \
             $s.WorkingDirectory = '{work}'; \
             $s.IconLocation = '{target_s}'; \
             $s.Save()"
        );
        let mut cmd = Command::new("powershell");
        cmd.args(["-NoProfile", "-WindowStyle", "Hidden", "-Command", &ps]);
        crate::win_process::no_window(&mut cmd);
        let status = cmd.status()?;
        if !status.success() {
            anyhow::bail!("PowerShell shortcut creation failed");
        }
        Ok(())
    }
}

#[cfg(not(windows))]
mod win {
    #[allow(dead_code)]
    pub fn is_enabled() -> bool {
        false
    }
    pub fn set_enabled(_on: bool, _arguments: &str) -> anyhow::Result<()> {
        Ok(())
    }
}

pub use win::set_enabled;
