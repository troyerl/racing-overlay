# GridGlance

Native multi-widget iRacing HUD. **Rust-only** (`overlay-rs/` /
`gridglance-overlay`): race widgets, Settings, Track Scan, Mongo sync, and
system tray.

## Requirements

- **Windows** (iRacing telemetry shared memory is Windows-only)
- Rust toolchain (to build from source; CI builds the installer)
- iRacing running (the overlay shows disconnected otherwise)

## Build & run

```powershell
cd overlay-rs
cargo build --release -p gridglance-overlay

# Demo telemetry + Settings
cargo run -p gridglance-overlay -- --demo --settings

# Live iRacing
cargo run -p gridglance-overlay --release -- --settings
```

See [`overlay-rs/README.md`](overlay-rs/README.md) for Track Scan, track
import, IPC, and packaging details.

## Project layout

```
overlay-rs/            # Rust workspace — the full app
  crates/gridglance-overlay   # egui HUD + Settings + tray + Track Scan
  crates/gridglance-ipc       # shared RPC types (optional external control)
assets/                # app icon + bundled fonts
installer/             # Inno Setup script
tracks/                # track shape files keyed by iRacing TrackID
RELEASE.md             # version + changelog (CI source of truth)
```

## Installer & releases

Pushing to `main`/`master` runs [`.github/workflows/release.yml`](.github/workflows/release.yml):

1. Reads version and notes from the topmost section of [`RELEASE.md`](RELEASE.md)
2. Stamps that version into `overlay-rs/Cargo.toml`
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

## License / fonts

MIT — see [`LICENSE`](LICENSE). Bundled Font Awesome fonts are under their
respective OFL licenses in `assets/fonts/`.
