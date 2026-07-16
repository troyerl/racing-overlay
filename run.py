#!/usr/bin/env python3
"""Legacy entry point — GridGlance is now the Rust binary.

Prefer::

    cargo run -p gridglance-overlay --release
    # or the installed gridglance-overlay / GridGlance.exe

This script only tries to locate and spawn the Rust overlay binary so older
shortcuts keep working. Track Scan, Settings, Mongo, and the tray all live in
Rust now; the Python ``overlay/`` package is no longer required at runtime.
"""

from __future__ import annotations

import os
import shutil
import subprocess
import sys


def _find_binary() -> str | None:
    env = os.environ.get("GRIDGLANCE_OVERLAY_BIN")
    if env and os.path.isfile(env):
        return env
    which = shutil.which("gridglance-overlay")
    if which:
        return which
    here = os.path.dirname(os.path.abspath(__file__))
    for rel in (
        os.path.join("overlay-rs", "target", "release", "gridglance-overlay"),
        os.path.join("overlay-rs", "target", "debug", "gridglance-overlay"),
        os.path.join("overlay-rs", "target", "release", "gridglance-overlay.exe"),
        os.path.join("overlay-rs", "target", "debug", "gridglance-overlay.exe"),
    ):
        path = os.path.join(here, rel)
        if os.path.isfile(path):
            return path
    return None


def main() -> int:
    bin_path = _find_binary()
    if not bin_path:
        print(
            "GridGlance is Rust-only now.\n"
            "  Build:  cd overlay-rs && cargo build -p gridglance-overlay --release\n"
            "  Run:    cargo run -p gridglance-overlay -- --settings\n"
            "Or set GRIDGLANCE_OVERLAY_BIN to the binary path.",
            file=sys.stderr,
        )
        return 1
    # Forward CLI args; default to opening Settings for desk-launch parity.
    args = list(sys.argv[1:])
    if not args:
        args = ["--settings"]
    return subprocess.call([bin_path, *args])


if __name__ == "__main__":
    raise SystemExit(main())
