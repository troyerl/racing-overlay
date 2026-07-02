#!/usr/bin/env python3
"""Entry point for the multi-widget iRacing overlay.

Usage:
    python3 run.py [--demo] [--demo-track ID] [--tracks-dir PATH]
                   [--no-clickthrough] [--settings] [--dump-config]

See README.md for the full list of flags. The standalone settings editor has
its own entry point:
    python3 -m overlay.config_editor
"""

from overlay.app import main

if __name__ == "__main__":
    raise SystemExit(main())
