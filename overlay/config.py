"""
Central configuration for every overlay widget.

Everything visual or behavioral (colors, fonts, sizes, toggles, row counts,
ranges, animation speeds) is defined here as defaults and can be overridden by an
`overlay_config.json` file placed next to the scripts. Only the keys you want to
change need to appear in that file -- it is deep-merged over the defaults.

Generate a full, editable template with:
    python3 sim_hud.py --dump-config        # writes overlay_config.json

Colors accept any of: "#RGB", "#RRGGBB", "#RRGGBBAA", "rgba(r,g,b,a)" or
[r, g, b] / [r, g, b, a] lists.
"""

from __future__ import annotations

import copy
import json
import os

from PyQt6.QtGui import QColor

CONFIG_FILE = os.path.join(
    os.path.dirname(os.path.dirname(os.path.abspath(__file__))), "overlay_config.json"
)

DEFAULTS: dict = {
    "font_family": "Segoe UI",
    # Global multiplier applied to every text size in every widget. Raise it to
    # make all text bigger, lower it to make everything smaller. Each widget also
    # has its own "text_scale" that multiplies on top of this one.
    "text_scale": 1.20,
    # Unit system for speed, temperature and fuel: "metric" (km/h, C, L) or
    # "imperial" (mph, F, gal). Affects the unit-aware "speed", temperature and
    # fuel readouts. (speed_kph / speed_mph stay fixed to their named unit.)
    "units": "metric",
    "table": {
        "corner_radius_frac": 0.03,
        "alt_row_shading": True,
        "font_scale": 0.40,
        "gap_font_scale": 1.12,
        "row_ease_tau": 0.16,
        "fade_ease_tau": 0.12,
        "widths": {  # as multiples of row height
            "badge": 0.95,
            "position": 1.25,
            "gap": 1.70,
            "irating": 2.70,
            "irating_narrow": 1.70,  # iRating pill width when no change arrow shows
            "license": 2.00,
            "pit": 2.10,
            "gutter": 0.18,
        },
        "colors": {
            "bg": "#101319f5",
            "border": "#ffffff1c",
            "cell_dark": "#080a0d",
            "row_alt": "#ffffff0a",
            "player_row": "#967c26b4",
            "threat": "#781a1adc",
            "text": "#f5f6f8",
            "muted": "#878e96",
            "irating_bg": "#eef0f2",
            "irating_text": "#14161a",
            "ir_up": "#22963c",
            "ir_down": "#c83232",
            "badge_player": "#ff8c00",
            "badge_pit_bg": "#ebeef0",
            "badge_pit_text": "#141414",
            "badge_lap": "#7638c4",
            "badge_empty_border": "#ffffff28",
            "badge_empty_fill": "#00000078",
        },
        "license_colors": {
            "R": "#d34a3c",
            "D": "#e0791a",
            "C": "#d6b400",
            "B": "#3a9b3a",
            "A": "#2f6bd8",
            "P": "#1a1a1a",
        },
    },
    "relative": {
        # Per-widget text size, multiplied by the global text_scale.
        "text_scale": 1.0,
        "rows_ahead": 3,
        "rows_behind": 3,
        "center_on_player": True,
        "show_footer": True,
        # pit_mode: one of "laps_since" (laps out since last stop),
        # "time_since" (time out since last stop), "at_lap" (lap they pitted on),
        # "at_time" (race clock when they pitted).
        "pit_mode": "laps_since",
        # When the iRating column is shown, also show the projected change arrow.
        "show_irating_change": True,
        "columns": {
            "badge": True,
            "position": True,
            "stripe": True,
            "name": True,
            "license": True,
            "irating": True,
            "pit": False,
            "gap": True,
        },
        # Header / footer are split into three sections; pick which item goes in
        # each (or "none"). Header items: sof, position. Footer: race_time, lap,
        # incidents.
        "header": {"left": "sof", "center": "none", "right": "position"},
        "footer": {"left": "race_time", "center": "lap", "right": "incidents"},
        # Per-section: show a Font Awesome icon instead of the text label.
        "header_icons": {"left": False, "center": False, "right": False},
        "footer_icons": {"left": False, "center": False, "right": False},
    },
    "standings": {
        # Per-widget text size, multiplied by the global text_scale.
        "text_scale": 1.0,
        "rows": 10,  # how many to show in top-N mode (center_on_player off)
        # When centered, the window is rows_ahead + you + rows_behind.
        "rows_ahead": 4,
        "rows_behind": 5,
        # When true, show a window of the running order centered on the player
        # instead of the top N positions.
        "center_on_player": True,
        "title": "Standings",
        "pit_mode": "laps_since",
        "show_irating_change": True,
        "columns": {
            "badge": True,
            "position": True,
            "stripe": True,
            "name": True,
            "license": True,
            "irating": True,
            "pit": False,
            "gap": True,
        },
        # Three header sections; pick the item for each (or "none").
        # Items: order_pill, title, count.
        "header": {"left": "order_pill", "center": "title", "right": "count"},
        # Per-section: show a Font Awesome icon instead of the text label.
        "header_icons": {"left": False, "center": False, "right": False},
    },
    "radar": {
        "range_pct": 0.03,
        "ease_side_tau": 0.10,
        "ease_glow_tau": 0.13,
        "show_nose": True,
        "show_axis": True,
        "sizes": {
            "car_w": 0.13,
            "car_h": 0.20,
            "bar_h": 0.78,
            "glow_w": 0.17,
            "nose_len": 0.16,
        },
        "colors": {
            "car": "#f5f6f8",
            "red": "#e23b3b",
            "yellow": "#ffd23a",
            "axis": "#ffffff2d",
            "nose": "#ffffffe6",
        },
    },
    "dash": {
        # Per-widget text size, multiplied by the global text_scale.
        "text_scale": 1.0,
        # Number of shift-light dots across the top.
        "shift_lights": 15,
        "show_shift_lights": True,
        # Green ring around the gear number. show_gear_ring toggles it; ring_source
        # picks what it fills with: "rpm" (vs redline), "throttle", or "brake".
        "show_gear_ring": True,
        "ring_source": "rpm",
        # Big position readout on the right (P12).
        "show_position": True,
        # Fractions of the shift range, counted from the top, painted red then
        # yellow (the rest is green): red is the top `shift_red_frac`, yellow the
        # next `shift_yellow_frac`.
        "shift_red_frac": 0.20,
        "shift_yellow_frac": 0.30,
        # The two big center readouts -- pick a metric for each (or "none").
        # Options: speed_kph, speed_mph, rpm, gear, position, lap, fuel,
        # last_lap, best_lap, cur_lap, delta, incidents, track_temp, air_temp.
        "center": {"left": "speed", "right": "rpm"},
        # The three items in the bottom strip -- same metric options as above.
        "bottom": {"left": "track_temp", "center": "air_temp", "right": "cur_lap"},
        # Per-slot: show a Font Awesome icon instead of the text label.
        "center_icons": {"left": False, "right": False},
        "bottom_icons": {"left": True, "center": True, "right": True},
        "colors": {
            "bg_top": "#171b21",
            "bg_bottom": "#0c0e12",
            "ring_active": "#5bd96a",
            "ring_track": "#373c44",
            "gear_text": "#f4f6f8",
            "label": "#8b93a1",
            "value": "#f4f6f8",
            "position": "#ff8c00",
            "shift_on": "#5bd96a",
            "shift_off": "#373c44",
            "shift_yellow": "#ffd23a",
            "shift_red": "#e23b3b",
            "bottom_bg": "#0b0d11ec",
            "bottom_border": "#ffffff22",
            "bottom_value": "#e9ebef",
            "bottom_label": "#8b93a1",
        },
    },
    "map": {
        # Per-widget text size (corner labels, car numbers), x global text_scale.
        "text_scale": 1.0,
        "asphalt_width": 11,
        "outline_width": 2,
        "dot_radius_frac": 0.05,
        "show_infield": True,
        "show_corners": True,
        "show_start_finish": True,
        "colors": {
            "asphalt": "#3a4048",
            "outline": "#7d858f",
            "infield": "#0c1218c8",
            "player": "#ffd400",
            "corner_bg": "#00000096",
            "corner_text": "#dce2e8",
        },
        "palette": [
            "#3aa0ff", "#ff5bac", "#46d27a", "#b06bff", "#ffa23a",
            "#ff5b5b", "#36d6d6", "#d6d636", "#7a8cff", "#ff8cce",
            "#5be0a0", "#c0c0c0",
        ],
    },
    "light_hud": {
        # Per-widget text size, multiplied by the global text_scale.
        "text_scale": 1.0,
        "font_px": 16,
        "colors": {
            "text": "#ffffff",
            "accent": "#00f0ff",
            "accent2": "#00ff66",
            "bg": "#0a0f14d9",
        },
    },
}


def _deep_merge(base: dict, override: dict) -> dict:
    out = copy.deepcopy(base)
    for k, v in (override or {}).items():
        if isinstance(v, dict) and isinstance(out.get(k), dict):
            out[k] = _deep_merge(out[k], v)
        else:
            out[k] = v
    return out


def load() -> dict:
    """Load defaults merged with overlay_config.json (if present)."""
    try:
        with open(CONFIG_FILE, "r", encoding="utf-8") as fh:
            user = json.load(fh)
    except (OSError, ValueError):
        user = {}
    return _deep_merge(DEFAULTS, user)


# The live, merged configuration. Reloadable via reload().
CFG: dict = load()

# Callbacks invoked whenever the live config changes (e.g. from the editor UI),
# so the running overlay can repaint immediately.
_listeners: list = []


def on_change(callback) -> None:
    """Register a callback(cfg) fired whenever the live config is replaced."""
    _listeners.append(callback)


# The widget section currently painting. Shared font helpers use it to apply the
# right per-widget text_scale without every call site passing a section. Safe
# because Qt painting runs on a single (GUI) thread.
_active_section: str | None = None


def use_section(name: str | None) -> None:
    """Mark which widget section is painting (set at the top of paintEvent)."""
    global _active_section
    _active_section = name


def text_scale_for(section: str | None = None) -> float:
    """Global text_scale times the given (or active) widget's own text_scale."""
    g = float(CFG.get("text_scale", 1.0) or 1.0)
    if section is None:
        section = _active_section
    if section:
        widget = CFG.get(section)
        if isinstance(widget, dict):
            return g * float(widget.get("text_scale", 1.0) or 1.0)
    return g


def set_cfg(new_cfg: dict, notify: bool = True) -> dict:
    """Replace the live config (full, merged dict) and notify listeners."""
    global CFG
    CFG = copy.deepcopy(new_cfg)
    if notify:
        for cb in list(_listeners):
            try:
                cb(CFG)
            except Exception:
                pass
    return CFG


def reload() -> dict:
    return set_cfg(load())


_MISSING = object()


def diff_from_defaults(cfg: dict, base: dict | None = None) -> dict:
    """Return only the keys in cfg that differ from the defaults (minimal)."""
    base = DEFAULTS if base is None else base
    out: dict = {}
    for k, v in cfg.items():
        b = base.get(k, _MISSING)
        if isinstance(v, dict) and isinstance(b, dict):
            sub = diff_from_defaults(v, b)
            if sub:
                out[k] = sub
        elif b is _MISSING or v != b:
            out[k] = v
    return out


def save(cfg: dict, path: str = CONFIG_FILE, minimal: bool = True) -> dict:
    """Persist a full config dict, writing only the diff from defaults by default."""
    data = diff_from_defaults(cfg) if minimal else cfg
    with open(path, "w", encoding="utf-8") as fh:
        json.dump(data, fh, indent=2)
    return data


def full_defaults() -> dict:
    return copy.deepcopy(DEFAULTS)


def write_template(path: str = CONFIG_FILE) -> str:
    with open(path, "w", encoding="utf-8") as fh:
        json.dump(DEFAULTS, fh, indent=2)
    return path


def units() -> str:
    """Active unit system: 'metric' or 'imperial'."""
    u = str(CFG.get("units", "metric")).strip().lower()
    return "imperial" if u.startswith("imp") else "metric"


def is_imperial() -> bool:
    return units() == "imperial"


def conv_speed(ms):
    """m/s -> km/h (metric) or mph (imperial)."""
    if ms is None:
        return None
    return ms * (2.2369362921 if is_imperial() else 3.6)


def speed_unit() -> str:
    return "MPH" if is_imperial() else "KPH"


def conv_temp(c):
    """Celsius -> Celsius (metric) or Fahrenheit (imperial)."""
    if c is None:
        return None
    return c * 9.0 / 5.0 + 32.0 if is_imperial() else c


def temp_unit() -> str:
    return "\u00b0F" if is_imperial() else "\u00b0C"


def conv_fuel(litres):
    """Litres -> litres (metric) or US gallons (imperial)."""
    if litres is None:
        return None
    return litres * 0.2641720524 if is_imperial() else litres


def fuel_unit() -> str:
    return "GAL" if is_imperial() else "L"


# Parsing a color string is surprisingly hot (every pen/brush, every frame), so
# cache the parsed QColor keyed by the original spec. Keys are the literal config
# values, so a changed value is simply a new key -- no invalidation needed. The
# returned QColor must not be mutated in place (callers only read/copy it).
_COLOR_CACHE: dict = {}


def _parse_color(value) -> QColor:
    if isinstance(value, (list, tuple)):
        return QColor(*[int(x) for x in value])
    s = str(value).strip()
    if s.startswith("rgba(") or s.startswith("rgb("):
        nums = s[s.index("(") + 1: s.index(")")].split(",")
        parts = [float(n) for n in nums]
        r, g, b = (int(parts[0]), int(parts[1]), int(parts[2]))
        a = int(parts[3] * 255) if len(parts) > 3 and parts[3] <= 1 else (
            int(parts[3]) if len(parts) > 3 else 255)
        return QColor(r, g, b, a)
    if s.startswith("#"):
        hexs = s[1:]
        if len(hexs) == 3:
            r, g, b = (int(c * 2, 16) for c in hexs)
            return QColor(r, g, b)
        if len(hexs) == 6:
            return QColor(int(hexs[0:2], 16), int(hexs[2:4], 16), int(hexs[4:6], 16))
        if len(hexs) == 8:
            return QColor(int(hexs[0:2], 16), int(hexs[2:4], 16),
                          int(hexs[4:6], 16), int(hexs[6:8], 16))
    c = QColor(s)
    return c if c.isValid() else QColor(255, 0, 255)


def qcolor(value) -> QColor:
    """Parse a color spec into a QColor (cached). Do not mutate the result."""
    if isinstance(value, QColor):
        return value
    key = tuple(value) if isinstance(value, (list, tuple)) else value
    cached = _COLOR_CACHE.get(key)
    if cached is not None:
        return cached
    result = _parse_color(value)
    if len(_COLOR_CACHE) > 1024:
        _COLOR_CACHE.clear()
    _COLOR_CACHE[key] = result
    return result
