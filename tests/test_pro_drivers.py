"""Professional driver matching and app-settings normalization."""

from __future__ import annotations

from overlay.track_store import is_pro_driver, normalize_pro_drivers


def test_normalize_pro_drivers_dedupes_aliases():
    out = normalize_pro_drivers([
        {"name": "Max Verstappen", "aliases": ["M Verstappen", "max verstappen", ""]},
        "Lewis Hamilton",
    ])
    assert out[0]["name"] == "Max Verstappen"
    assert out[0]["aliases"] == ["M Verstappen"]
    assert out[1] == {"name": "Lewis Hamilton", "aliases": []}


def test_is_pro_driver_matches_name_and_alias():
    drivers = [{"name": "Max Verstappen", "aliases": ["M Verstappen"]}]
    assert is_pro_driver("Max Verstappen", drivers)
    assert is_pro_driver("m verstappen", drivers)
    assert not is_pro_driver("Charles Leclerc", drivers)
    assert not is_pro_driver("", drivers)
    assert not is_pro_driver("Max Verstappen", [])
