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
