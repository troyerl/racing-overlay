"""
Font Awesome 6 (Free, Solid) icon support.

Loads the bundled TTF once and exposes:
  * icon_font(px): a QFont sized in pixels (scaled by config.text_scale),
  * GLYPHS: metric/label-name -> single-character glyph string,
  * glyph(name): the glyph for a name (or "" if unknown / font missing),
  * available(): whether the icon font loaded.

Icons are referenced by the same metric/label names the widgets already use
(speed_kph, rpm, track_temp, sof, lap, ...), so a widget can swap a text label
for an icon just by looking the name up here.
"""

from __future__ import annotations

import os

from PyQt6.QtGui import QFont, QFontDatabase

from .. import config

# assets/ is bundled alongside the app (repo root in dev, _MEIPASS when frozen).
from .. import paths
_FONT_PATH = paths.resource_file("assets", "fonts", "fa-solid-900.ttf")

# name -> Font Awesome 6 Free Solid codepoint.
_CODEPOINTS: dict[str, int] = {
    # dash speed / engine metrics
    "speed": 0xF625,          # gauge-high
    "speed_kph": 0xF625,      # gauge-high
    "speed_mph": 0xF625,      # gauge-high
    "rpm": 0xF624,            # gauge
    "gear": 0xF013,           # gear (cog)
    "position": 0xF292,       # hashtag
    # lap / fuel / tire metrics
    "lap": 0xF11E,            # flag-checkered
    "lap_count": 0xF11E,      # flag-checkered
    "laps_left": 0xF11E,      # flag-checkered
    "fuel": 0xF52F,           # gas-pump
    "fuel_laps": 0xF52F,      # gas-pump
    "fuel_stack": 0xF52F,     # gas-pump
    "tires": 0xF1CD,          # life-ring (tire-like)
    # timing metrics
    "last_lap": 0xF2F2,       # stopwatch
    "best_lap": 0xF091,       # trophy
    "cur_lap": 0xF2F2,        # stopwatch
    "delta": 0xF252,          # hourglass-half
    "incidents": 0xF071,      # triangle-exclamation
    # environment
    "track_temp": 0xF2C9,     # temperature-half (thermometer)
    "air_temp": 0xF72E,       # wind
    # decorative
    "sparkle": 0xF005,        # star
    # table header / footer items
    "sof": 0xF0C0,            # users
    "class_sof": 0xF0C0,      # users
    "class_position": 0xF292, # hashtag
    "session_time": 0xF017,   # clock
    "race_time": 0xF017,      # clock
    "session_best": 0xF091,   # trophy
    "track_name": 0xF018,     # road
    "local_time": 0xF017,     # clock
    "sim_time": 0xF185,       # sun
    "cpu": 0xF2DB,            # microchip
    "mem": 0xF538,            # memory
    "order_pill": 0xF0CB,     # list-ol
    "title": 0xF091,          # trophy
    "count": 0xF0C0,          # users
    "speaking": 0xF028,       # volume-high
    "irating_up": 0xF062,     # arrow-up
    "irating_down": 0xF063,   # arrow-down
}

_family: str | None = None
_glyphs: dict[str, str] = {}


def _load() -> None:
    global _family, _glyphs
    if _family is not None or not os.path.exists(_FONT_PATH):
        return
    fid = QFontDatabase.addApplicationFont(_FONT_PATH)
    fams = QFontDatabase.applicationFontFamilies(fid) if fid != -1 else []
    if not fams:
        _family = ""  # mark as attempted-but-failed
        return
    _family = fams[0]
    _glyphs = {name: chr(cp) for name, cp in _CODEPOINTS.items()}


def available() -> bool:
    _load()
    return bool(_family)


_ICON_FONT_CACHE: dict = {}


def icon_font(px: float) -> QFont:
    _load()
    pxi = max(6, int(round(px * config.text_scale_for())))
    key = (_family, pxi)
    f = _ICON_FONT_CACHE.get(key)
    if f is None:
        f = QFont(_family or "")
        f.setPixelSize(pxi)
        if len(_ICON_FONT_CACHE) > 512:
            _ICON_FONT_CACHE.clear()
        _ICON_FONT_CACHE[key] = f
    return f


def glyph(name: str) -> str:
    _load()
    return _glyphs.get(name, "")


def has(name: str) -> bool:
    """True when an icon exists for this name and the font is available."""
    _load()
    return bool(_family) and name in _glyphs


# Expose the raw map for tooling/tests.
GLYPHS = _glyphs
