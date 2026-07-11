"""Personal driver groups: normalize, match, first-group-wins."""

from __future__ import annotations

from overlay.driver_groups import driver_group_for_name, normalize_driver_groups


def test_normalize_driver_groups():
    out = normalize_driver_groups([
        {
            "name": "My League",
            "icon": "trophy",
            "color": "#ff00aa",
            "members": [
                {"name": "Alice", "aliases": ["A Smith", "alice"]},
                "Bob",
            ],
        },
        {"name": "My League", "members": [{"name": "Dup"}]},  # dedupe by name
        {"name": "", "members": [{"name": "Nope"}]},
        "not-a-dict",
        {
            "name": "Other",
            "icon": "not-real",
            "color": "red",
            "members": [],
        },
    ])
    assert len(out) == 2
    assert out[0]["name"] == "My League"
    assert out[0]["icon"] == "trophy"
    assert out[0]["color"] == "#ff00aa"
    assert out[0]["members"][0]["aliases"] == ["A Smith"]
    assert out[0]["members"][1] == {"name": "Bob", "aliases": []}
    assert out[1]["name"] == "Other"
    assert out[1]["icon"] == "league"
    assert out[1]["color"] == "#5bb8ff"


def test_driver_group_for_name_first_wins():
    groups = [
        {"name": "A", "icon": "flag", "color": "#111111",
         "members": [{"name": "Shared", "aliases": []}]},
        {"name": "B", "icon": "bolt", "color": "#222222",
         "members": [{"name": "Shared", "aliases": ["Alias"]}]},
    ]
    g = driver_group_for_name("Shared", groups)
    assert g is not None and g["name"] == "A"
    g2 = driver_group_for_name("alias", groups)
    assert g2 is not None and g2["name"] == "B"
    assert driver_group_for_name("Nobody", groups) is None
    assert driver_group_for_name("", groups) is None
    assert driver_group_for_name("Shared", []) is None
