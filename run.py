#!/usr/bin/env python3
"""Entry point for the multi-widget iRacing overlay.

Usage:
    python3 run.py [--demo] [--tracks-dir PATH]
                   [--no-clickthrough] [--settings] [--dump-config]
                   [--rust | --python | --backend rust|python]

When ``overlay-rs`` has been built (``cargo build -p gridglance-overlay``),
the default backend is the Rust widget host with Python settings. Force the
legacy PyQt overlay with ``--python``.

    --demo-track ID is deprecated: demo mode uses the shared Community demo
    track from MongoDB (Settings → App) when configured.

See README.md for the full list of flags. The standalone settings editor has
its own entry point:
    python3 -m overlay.config_editor
    python3 -m overlay.config_editor --rust-overlay
"""

from overlay.app import main

if __name__ == "__main__":
    raise SystemExit(main())
