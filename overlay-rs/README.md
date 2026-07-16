# GridGlance Rust overlay

**`gridglance-overlay` is the full app** — race widgets, Settings, Track Scan, Mongo sync, system tray. No Python runtime required.

## Build

```bash
cd overlay-rs
cargo build --release -p gridglance-overlay
```

Binary: `target/release/gridglance-overlay`

## Run

```bash
# Demo telemetry + Settings
cargo run -p gridglance-overlay -- --demo --settings

# Import members HTML/SVG → track JSON (headless; replaces tools/svg_layers_to_track_v2.py)
cargo run -p gridglance-overlay -- \
  --import-track path/to/page.html --track-id 123 --name "Spa" --force

# Track Scan (requires GRIDGLANCE_MONGODB_URI write credential)
cargo run -p gridglance-overlay -- --demo --track-scan

# Live iRacing (Windows)
cargo run -p gridglance-overlay --release -- --settings
```

Tray menu: Settings / Track Scan / Start–Stop / Edit layout / Check updates / Quit.
Second launch activates the existing instance.

Author env (optional `.env` in repo root or next to the binary):

```bash
GRIDGLANCE_MONGODB_URI="mongodb+srv://…"   # unlocks Track Scan + cloud upload
```

IPC still listens on `127.0.0.1:19847` for external tools.

## Packaging

Windows installer ([`installer/gridglance.iss`](../installer/gridglance.iss)) ships the
release `gridglance-overlay.exe` as `GridGlance.exe` plus `assets/app.ico`.

CI (`.github/workflows/release.yml`) on push to `main`:

1. Reads version + notes from root `RELEASE.md`
2. Stamps that version into `overlay-rs/Cargo.toml`
3. `cargo build --release -p gridglance-overlay` with `GRIDGLANCE_GITHUB_REPO` set
   (enables in-app update checks)
4. Builds the Inno Setup installer and publishes a GitHub Release

Legacy `run.py` only locates and spawns that binary.
