//! Embed `assets/app.ico` into the Windows PE so Explorer / taskbar use the brand icon.

fn main() {
    let icon = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("assets")
        .join("app.ico");
    println!("cargo:rerun-if-changed={}", icon.display());

    #[cfg(windows)]
    {
        if !icon.is_file() {
            println!(
                "cargo:warning=app icon missing at {}; taskbar will use the default EXE icon",
                icon.display()
            );
            return;
        }
        let mut res = winres::WindowsResource::new();
        res.set_icon(icon.to_str().expect("icon path is valid UTF-8"));
        if let Err(e) = res.compile() {
            println!("cargo:warning=winres failed to embed app icon: {e}");
        }
    }
}
