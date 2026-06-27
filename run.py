#!/usr/bin/env python3
"""Entry point for the multi-widget iRacing overlay.

Usage:
    python3 run.py [--demo] [--no-clickthrough] [--settings] [--dump-config]

See README.md for the full list of flags. The simple Fuel/Delta HUD and the
standalone settings editor have their own entry points:
    python3 -m overlay.widgets.light_hud [--demo] [--gallons]
    python3 -m overlay.config_editor
"""

from overlay.app import main

if __name__ == "__main__":
    raise SystemExit(main())
