"""Persist the lap-compare benchmark lap per car+track to a JSON file.

Each entry is keyed by "<track id>::<car id>" so your all-time best reference
lap survives restarts and is restored when you load back into the same car at
the same track.
"""

from __future__ import annotations

import json

from . import paths

STORE_FILE = paths.data_file("lap_compare_best.json")


def load_all() -> dict:
    try:
        with open(STORE_FILE, "r", encoding="utf-8") as fh:
            data = json.load(fh)
        return data if isinstance(data, dict) else {}
    except (OSError, ValueError):
        return {}


def load(key: str):
    return load_all().get(key)


def save(key: str, entry: dict) -> None:
    data = load_all()
    data[key] = entry
    try:
        with open(STORE_FILE, "w", encoding="utf-8") as fh:
            json.dump(data, fh)
    except OSError:
        pass
