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

# assets/ lives at the repo root (two levels up from overlay/widgets/).
_ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
_FONT_PATH = os.path.join(_ROOT, "assets", "fonts", "fa-solid-900.ttf")

# name -> Font Awesome 6 Free Solid codepoint.
_CODEPOINTS: dict[str, int] = {
    # dash center / bottom metrics
    "speed_kph": 0xF625,      # gauge-high
    "speed_mph": 0xF625,      # gauge-high
    "rpm": 0xF624,            # gauge
    "gear": 0xF013,           # gear (cog)
    "position": 0xF292,       # hashtag
    "lap": 0xF11E,            # flag-checkered
    "fuel": 0xF52F,           # gas-pump
    "last_lap": 0xF017,       # clock
    "best_lap": 0xF091,       # trophy
    "cur_lap": 0xF2F2,        # stopwatch
    "delta": 0xF252,          # hourglass-half
    "incidents": 0xF071,      # triangle-exclamation
    "track_temp": 0xF018,     # road
    "air_temp": 0xF2C9,       # temperature-half
    # table header / footer items
    "sof": 0xF0C0,            # users
    "race_time": 0xF017,      # clock
    "order_pill": 0xF0CB,     # list-ol
    "title": 0xF091,          # trophy
    "count": 0xF0C0,          # users
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
