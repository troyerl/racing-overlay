# GridGlance Rust overlay

**`gridglance-overlay` is the full app** — race widgets, Settings, Track Scan,
Mongo sync, system tray. No other runtime required.

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

# Import members HTML/SVG → track JSON (headless)
cargo run -p gridglance-overlay -- \
  --import-track path/to/page.html --track-id 123 --name "Spa" --force

# Track Scan (requires GRIDGLANCE_MONGODB_URI write credential)
cargo run -p gridglance-overlay -- --demo --track-scan

# Live iRacing (Windows)
cargo run -p gridglance-overlay --release -- --settings
```

Tray menu: Settings / Track Scan / Start–Stop / Edit layout / Check updates / Quit.
Second launch activates the existing instance. Release builds run without a
console window.

Author / local env (optional `.env` in repo root, next to the binary, or
`%LOCALAPPDATA%\GridGlance\.env` — see [`.env.example`](../.env.example)):

```bash
GRIDGLANCE_MONGODB_READ_URI="mongodb+srv://…"   # optional override of baked read
GRIDGLANCE_MONGODB_URI="mongodb+srv://…"        # Track Scan + cloud upload
```

**Installers:** release CI bakes a read-only URI from the GitHub secret
`GRIDGLANCE_MONGODB_READ_URI`, so cloud track download works with no `.env`.
Local/dev builds without that env at compile time have cloud off until you
set a URI. Write/Track Scan always requires `GRIDGLANCE_MONGODB_URI` at runtime.

IPC listens on `127.0.0.1:19847`. Mutating methods require a `token` field
matching `%LOCALAPPDATA%\GridGlance\ipc_token` (created on first launch).
`ping`, `layout.get`, and map state reads stay public.

## Packaging

Windows installer ([`installer/gridglance.iss`](../installer/gridglance.iss))
ships the release `gridglance-overlay.exe` as `GridGlance.exe` plus
`assets/app.ico`.

CI (`.github/workflows/release.yml`) on push to `main`:

1. Reads version + notes from root `RELEASE.md`
2. Stamps that version into `overlay-rs/Cargo.toml`
3. `cargo build --release -p gridglance-overlay` with `GRIDGLANCE_GITHUB_REPO`
   and secret `GRIDGLANCE_MONGODB_READ_URI` set (update checks + baked cloud read)
4. Builds the Inno Setup installer and publishes a GitHub Release

Repo admins must configure the Actions secret `GRIDGLANCE_MONGODB_READ_URI`
(Atlas **read-only** user) or release builds ship without cloud track sync.