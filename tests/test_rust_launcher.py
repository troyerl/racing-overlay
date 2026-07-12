"""Smoke tests for Rust overlay launcher helpers."""

from __future__ import annotations

from pathlib import Path

from overlay import rust_launcher


def test_overlay_binary_candidates_include_dev_paths():
    paths = rust_launcher.overlay_binary_candidates()
    assert any("overlay-rs" in str(p) for p in paths)


def test_find_overlay_binary_when_built():
    # Release or debug binary from a local cargo build.
    found = rust_launcher.find_overlay_binary()
    if found is None:
        # Not built in this environment — still a valid outcome.
        return
    assert Path(found).is_file()
