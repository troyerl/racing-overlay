# GridGlance

Native multi-widget iRacing HUD. **Rust-only** (`gridglance-overlay`): race
widgets, Settings, Track Scan, Mongo sync, and system tray.

## Requirements

- **Windows** (iRacing telemetry shared memory is Windows-only)
- Rust toolchain (to build from source; CI builds the installer)
- iRacing running (the overlay shows disconnected otherwise)

## Build & run

```powershell
cargo build --release -p gridglance-overlay

# Demo telemetry + Settings
cargo run -p gridglance-overlay -- --demo --settings

# Import members HTML/SVG → track JSON (headless)
cargo run -p gridglance-overlay -- `
  --import-track path/to/page.html --track-id 123 --name "Spa" --force

# Track Scan (requires GRIDGLANCE_MONGODB_URI write credential)
cargo run -p gridglance-overlay -- --demo --track-scan

# Live iRacing
cargo run -p gridglance-overlay --release -- --settings
```

Binary: `target/release/gridglance-overlay.exe`

Tray menu: Settings / Track Scan / Start–Stop / Edit layout / Check updates /
Quit. Second launch activates the existing instance. Release builds run without
a console window.

Author / local env (optional `.env` in repo root, next to the binary, or
`%LOCALAPPDATA%\GridGlance\.env` — see [`.env.example`](.env.example)):

```bash
GRIDGLANCE_MONGODB_READ_URI="mongodb+srv://…"   # optional override of baked read
GRIDGLANCE_MONGODB_URI="mongodb+srv://…"        # Track Scan + cloud upload
```

**Installers:** release CI bakes a read-only URI from the GitHub secret
`GRIDGLANCE_MONGODB_READ_URI`, so cloud track download works with no `.env`.
Local/dev builds without that env at compile time have cloud off until you set
a URI. Write/Track Scan always requires `GRIDGLANCE_MONGODB_URI` at runtime.

IPC listens on `127.0.0.1:19847`. Mutating methods require a `token` field
matching `%LOCALAPPDATA%\GridGlance\ipc_token` (created on first launch).
`ping`, `layout.get`, and map state reads stay public.

## Project layout

```
crates/gridglance-overlay   # egui HUD + Settings + tray + Track Scan
crates/gridglance-ipc       # shared RPC types (optional external control)
assets/                     # app icon + bundled fonts
installer/                  # Inno Setup script
tracks/                     # track shape files keyed by iRacing TrackID
RELEASE.md                  # version + changelog (CI source of truth)
```

## Installer & releases

Pushing to `main`/`master` runs [`.github/workflows/release.yml`](.github/workflows/release.yml):

1. Reads version and notes from the topmost section of [`RELEASE.md`](RELEASE.md)
2. Stamps that version into root `Cargo.toml`
3. Runs tests, then builds `gridglance-overlay` and packages `GridGlance-Setup-<version>.exe`
4. Creates git tag `v<version>` and a GitHub Release

Pull requests run [`.github/workflows/ci.yml`](.github/workflows/ci.yml)
(`cargo fmt`, `clippy`, `test`).

To cut a release, add a new `## <version> - <date>` section at the top of
`RELEASE.md` and push.

The installed app can check GitHub for updates (tray / Settings). Update checks
need `GRIDGLANCE_GITHUB_REPO` set at build time (CI does this). Cloud track
download uses a read-only Mongo URI baked from the Actions secret
`GRIDGLANCE_MONGODB_READ_URI` (no `.env` for end users).

Repo admins must configure the Actions secret `GRIDGLANCE_MONGODB_READ_URI`
(Atlas **read-only** user) or release builds ship without cloud track sync.

## License / fonts

MIT — see [`LICENSE`](LICENSE). Bundled Font Awesome fonts are under their
respective OFL licenses in `assets/fonts/`.
