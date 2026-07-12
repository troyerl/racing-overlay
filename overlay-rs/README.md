# GridGlance Rust overlay

Hybrid architecture: **race widgets run here**; **settings / Track Scan stay in Python**.

## Build

```bash
export RUSTUP_HOME="$PWD/../.rustup"   # if using workspace-local toolchain
export CARGO_HOME="$PWD/../.cargo"
export PATH="$CARGO_HOME/bin:$PATH"
cargo build --release
```

Binary: `target/release/gridglance-overlay`

## Run (demo)

```bash
cargo run -p gridglance-overlay -- --demo --no-clickthrough
```

IPC listens on `127.0.0.1:19847` (newline-delimited JSON-RPC).

## Run with live iRacing (Windows)

1. Build a release binary on Windows:

```bash
cd overlay-rs
cargo build --release -p gridglance-overlay
```

2. Start iRacing (enable memory telemetry: `irsdkEnableMem=1` in `app.ini` if needed).

3. From the repo root, launch the hybrid stack **without** `--demo`:

```bash
python run.py --rust --start
```

Python settings + Rust widgets; live IRSDK fills the dash. Widget `show` flags come from your preset (full `CFG` is pushed over IPC on launch).

The overlay uses one transparent, always-on-top window covering the virtual desktop (Glow renderer). Rounded-card alpha and radar/map with `show_panel` off should see through to the sim. Rebuild after pulling this change:

```bash
cargo build --release -p gridglance-overlay
```

Demo feed (no sim):

```bash
python run.py --rust --demo
```

## Launch from Python

```bash
# Uses Rust overlay when the binary is present (Phase 3 default)
python3 run.py --demo

# Force Python PyQt overlay
python3 run.py --python --demo

# Force Rust
python3 run.py --rust --demo
# or: GRIDGLANCE_BACKEND=rust python3 run.py --demo
```

Settings-only against an already-running overlay:

```bash
python3 -m overlay.config_editor --rust-overlay
```

## Crates

| Crate | Role |
|-------|------|
| `gridglance-ipc` | Shared request/response method names |
| `gridglance-overlay` | egui multi-viewport host + widgets + IPC server |
