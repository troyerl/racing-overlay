"""Parse iRacing event_result JSON and merge driver display names into lists."""

from __future__ import annotations

import json
from pathlib import Path
from typing import Any

from .track_store import normalize_pro_drivers


def _unwrap_payload(payload: Any) -> dict:
    if not isinstance(payload, dict):
        return {}
    if isinstance(payload.get("data"), dict):
        return payload["data"]
    return payload


def _iter_result_rows(data: dict):
    sessions = data.get("session_results") or data.get("sessionResults") or []
    if not isinstance(sessions, list):
        return
    for sess in sessions:
        if not isinstance(sess, dict):
            continue
        rows = sess.get("results") or sess.get("Results") or []
        if not isinstance(rows, list):
            continue
        for row in rows:
            if isinstance(row, dict):
                yield row


def parse_event_result_names(source: Any) -> list[str]:
    """Return ordered unique display names from an event_result payload or path.

    Accepts a filesystem path, Path, raw JSON string, or already-parsed dict.
    Skips AI entries and empty names; dedupes by casefold (first-seen wins).
    """
    payload: Any = source
    if isinstance(source, Path):
        payload = json.loads(source.read_text(encoding="utf-8"))
    elif isinstance(source, str):
        stripped = source.lstrip()
        if stripped.startswith(("{", "[")):
            payload = json.loads(source)
        else:
            payload = json.loads(Path(source).read_text(encoding="utf-8"))
    elif isinstance(source, (bytes, bytearray)):
        payload = json.loads(source.decode("utf-8"))

    data = _unwrap_payload(payload)
    out: list[str] = []
    seen: set[str] = set()
    for row in _iter_result_rows(data):
        if row.get("ai"):
            continue
        name = str(row.get("display_name") or row.get("displayName") or "").strip()
        if not name:
            continue
        key = name.casefold()
        if key in seen:
            continue
        seen.add(key)
        out.append(name)
    return out


def _occupied_keys(entries: list[dict]) -> set[str]:
    keys: set[str] = set()
    for entry in entries:
        name = str(entry.get("name") or "").strip()
        if name:
            keys.add(name.casefold())
        for alias in entry.get("aliases") or []:
            s = str(alias or "").strip()
            if s:
                keys.add(s.casefold())
    return keys


def merge_driver_entries(
    existing, names: list[str],
) -> tuple[list[dict], int, int]:
    """Append new names that do not match existing name/alias (casefold).

    Returns ``(merged, added_count, skipped_count)``.
    """
    merged = normalize_pro_drivers(existing)
    occupied = _occupied_keys(merged)
    added = 0
    skipped = 0
    for raw in names:
        name = str(raw or "").strip()
        if not name:
            skipped += 1
            continue
        key = name.casefold()
        if key in occupied:
            skipped += 1
            continue
        merged.append({"name": name, "aliases": []})
        occupied.add(key)
        added += 1
    return normalize_pro_drivers(merged), added, skipped
