"""Settings editor nav ordering and map settings skip list."""

from __future__ import annotations

from overlay.config_editor import (
    MAP_SETTINGS_SKIP,
    SECTION_SETTINGS_SKIP,
    WIDGET_NAV_GROUPS,
    ordered_settings_sections,
)


def test_ordered_settings_sections_grouped_not_alphabetical():
    sections = ordered_settings_sections(include_scan=False)
    keys = [k for k, _t, _g in sections if not k.startswith("__")]
    assert keys[0] == "relative"
    assert keys[1] == "standings"
    assert "map" in keys
    assert keys.index("map") > keys.index("dash")
    # Was A-Z: dash before relative; now standings first.
    assert keys.index("relative") < keys.index("dash")


def test_ordered_settings_sections_includes_scan_when_requested():
    sections = ordered_settings_sections(include_scan=True)
    keys = [k for k, _t, _g in sections]
    assert "__scan__" in keys
    assert keys.index("__scan__") < keys.index("relative")


def test_widget_nav_groups_cover_known_widgets():
    grouped = {k for _g, keys in WIDGET_NAV_GROUPS for k in keys}
    assert "map" in grouped
    assert "radar" in grouped
    assert "dash" in grouped


def test_map_settings_skip_keys():
    assert "auto_corners" in MAP_SETTINGS_SKIP
    assert "row_dividers" in MAP_SETTINGS_SKIP
    assert "data_font_bold" in MAP_SETTINGS_SKIP


def test_section_settings_skip_hides_row_dividers_where_no_rows():
    for section in ("radar", "delta_bar", "flags", "inputs", "ers_hybrid",
                    "tire_panel", "sector_timing", "radio_tower"):
        assert "row_dividers" in SECTION_SETTINGS_SKIP[section]


def test_section_settings_skip_row_dividers_not_hidden_for_list_widgets():
    for section in ("leaderboard_strip", "pit_board", "weather_panel",
                    "system_panel"):
        assert section not in SECTION_SETTINGS_SKIP or (
            "row_dividers" not in SECTION_SETTINGS_SKIP.get(section, frozenset())
        )


def test_cloud_tracks_always_enabled():
    from overlay import config

    assert config.cloud_tracks() is True
    config.set_cloud_tracks(False)
    assert config.cloud_tracks() is True
