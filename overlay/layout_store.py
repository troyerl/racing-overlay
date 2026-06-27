"""Persist per-panel window geometry to a JSON file next to the scripts."""

from __future__ import annotations

import json

from . import paths

LAYOUT_FILE = paths.data_file("overlay_layout.json")


def load_layout() -> dict:
    try:
        with open(LAYOUT_FILE, "r", encoding="utf-8") as fh:
            data = json.load(fh)
        return data if isinstance(data, dict) else {}
    except (OSError, ValueError):
        return {}


def save_layout(layout: dict) -> None:
    try:
        with open(LAYOUT_FILE, "w", encoding="utf-8") as fh:
            json.dump(layout, fh, indent=2)
    except OSError:
        pass
