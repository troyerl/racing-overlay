"""
Visual settings editor for the overlay.

A schema-driven GUI built automatically from config.DEFAULTS, so it always
exposes *every* customizable key (colors, fonts, sizes, counts, toggles, easing,
palettes). It reads/writes overlay_config.json (JSON, not SQLite -- the config is
a small nested document read at startup, where JSON is simpler and stays
human-editable; SQLite would only pay off for large/queried/concurrent data).

Run standalone:
    python3 config_editor.py

Or launch it alongside a live overlay (changes apply instantly):
    python3 sim_hud.py --demo --no-clickthrough --settings
"""

from __future__ import annotations

import copy
import sys

from PyQt6.QtCore import (
    Qt,
    QTimer,
    QRectF,
    QPointF,
    QSize,
    QEasingCurve,
    QPropertyAnimation,
    pyqtProperty,
    pyqtSignal,
)
from PyQt6.QtGui import (QBrush, QColor, QFont, QLinearGradient, QPainter, QPen,
                         QPixmap)
from PyQt6.QtWidgets import (
    QAbstractButton,
    QAbstractItemView,
    QApplication,
    QColorDialog,
    QComboBox,
    QDoubleSpinBox,
    QFrame,
    QHBoxLayout,
    QInputDialog,
    QLabel,
    QLineEdit,
    QListWidget,
    QListWidgetItem,
    QMessageBox,
    QProgressDialog,
    QPushButton,
    QScrollArea,
    QSlider,
    QSpinBox,
    QStackedWidget,
    QVBoxLayout,
    QWidget,
)

from . import config, constants, paths, track_store, version

COLOR_PARENTS = {"colors", "license_colors"}

# Simple key-name -> options dropdowns.
# A short curated list of fonts that suit a racing HUD. Segoe UI is the default
# (ships with Windows); Bahnschrift / Agency FB are condensed "industrial" faces
# that also ship with Windows; the rest are common fallbacks. Any font not
# installed falls back gracefully, and a custom value already in the config is
# kept as an option so the dropdown never discards it.
FONT_CHOICES = [
    "Segoe UI", "Bahnschrift", "Bahnschrift Condensed", "Agency FB",
    "Eurostile", "Consolas", "SF Mono", "Menlo", "Tahoma", "Verdana", "Arial",
]

ENUMS = {
    "pit_mode": ["laps_since", "time_since", "at_lap", "at_time"],
    "units": ["metric", "imperial"],
    "center_mode": ["ring", "pedals"],
    "car_label": ["number", "position"],
    "delta_mode": ["previous", "best"],
    "mode": ["session_best", "best_lap", "optimal"],
    "rotation": [0, 90, 180, 270],
    "font_family": FONT_CHOICES,
    "tabular_font_family": [""] + FONT_CHOICES,
}

# Friendly display text for raw config option values (combo boxes show these,
# but the underlying stored value is unchanged).
OPTION_LABELS = {
    "none": "None",
    "": "Default (Consolas / SF Mono)",
    # dash metrics
    "speed": "Speed", "speed_kph": "Speed (km/h)", "speed_mph": "Speed (mph)",
    "rpm": "RPM", "gear": "Gear", "position": "Position", "car_number": "Car number",
    "lap": "Lap",
    "lap_count": "Lap (x/total)", "fuel": "Fuel", "fuel_stack": "Fuel (+laps)",
    "fuel_laps": "Fuel laps left", "tires": "Tire wear (L/R)",
    "laps_left": "Laps remaining",
    "last_lap": "Last lap", "best_lap": "Best lap",
    "cur_lap": "Current lap", "delta": "Delta", "incidents": "Incidents",
    "irating": "iRating",
    "track_temp": "Track temp", "air_temp": "Air temp",
    # table header / footer items
    "sof": "Strength of field", "class_sof": "Class strength of field",
    "race_time": "Race time (elapsed / total)", "session_time": "Time remaining",
    "class_position": "Class position", "track_name": "Track name",
    "session_best": "Session best lap", "local_time": "Local time",
    "sim_time": "Sim time of day", "cpu": "CPU usage %", "mem": "Memory usage %",
    "order_pill": "Order", "title": "Title", "count": "Count",
    # pit_mode
    "laps_since": "Laps since pit", "time_since": "Time since pit",
    "at_lap": "Lap pitted on", "at_time": "Race time pitted",
    # units
    "metric": "Metric (km/h, °C, L)", "imperial": "Imperial (mph, °F, gal)",
    # input toggles / center_mode
    "throttle": "Throttle", "brake": "Brake",
    "ring": "Gear ring", "pedals": "Pedal bars",
    # map car-dot label
    "number": "Car number",
    # laptime log delta baseline
    "previous": "Previous lap", "best": "Session best lap",
    # delta bar reference lap (session_best reuses the label above)
    "best_lap": "My best lap", "optimal": "Optimal lap",
    # map rotation (degrees clockwise)
    0: "0\u00b0 (default)", 90: "90\u00b0 clockwise",
    180: "180\u00b0", 270: "270\u00b0 clockwise",
}

# Friendly labels for specific config keys whose auto-generated name is too
# terse to be meaningful. Keyed by "section.key" (preferred) or bare key.
LABEL_OVERRIDES = {
    "check_updates_on_launch": "Check for updates on launch",
    "font_family": "Font",
    "row_height_px": "Fixed row height (px, 0 = scale to fit)",
    "max_row_height_frac": "Max row height (panel fraction)",
    "irating_abbreviate": "Abbreviate iRating (1.4k vs 1432)",
    "show_irating_projection": "Show projected iRating change",
    "dash.show_irating_projection": "Show projected iRating change next to iRating",
    "dash.corner_radius_frac": "Sub-panel corner roundness",
    "dash.irating_border": "iRating pill border",
    "font_scale": "Row text size",
    "gap_font_scale": "Gap column text size",
    "row_dividers": "Hairline dividers between rows",
    "header_bg": "Header band background",
    "footer_bg": "Footer band background",
    "cell_dark": "Dark cell / pill fill",
    "cell_border": "Dark cell / pill border",
    "border": "Panel border (alias for panel_border)",
    "panel_border": "Panel border",
    "row_alt": "Alternating row shading",
    "corner_border": "Corner label pill border",
    "scan_bg": "Track-scan badge background",
    "hint_bg": "Transient hint banner background",
    "hint_text": "Hint banner text color",
    "name_font_bold": "Bold driver names",
    "data_font_bold": "Bold position / gap / lap times",
    "irating_show_icon": "Show iRating chart icon",
    "tabular_font_family": "Tabular font (gap / lap times)",
    "header_font_scale": "Header text size (independent of rows)",
    "footer_font_scale": "Footer text size (independent of rows)",
    "radar.show_front": "Front sensing",
    "radar.show_rear": "Rear sensing",
    "radar.side_span_pct": "Side marker travel (lap fraction)",
    "radar.side_proximity_color": "Fade side marker yellow\u2192red by overlap",
    "inputs.history_seconds": "Trace length (seconds)",
    "inputs.label_text": "Title text",
    "inputs.show_label": "Show title tab",
    "inputs.show_graph": "Show scrolling trace",
    "inputs.show_bars": "Show value bars",
    "inputs.show_gauge": "Show gear/speed gauge",
    "inputs.show_steering": "Show steering line",
    "inputs.show_brake_threshold": "Show brake threshold line",
    "inputs.brake_threshold": "Brake threshold (%)",
    "fuel_calc.title": "Title text",
    "fuel_calc.history_laps": "Laps to average for fuel use",
    "fuel_calc.show_title": "Show title bar",
    "fuel_calc.show_pill": "Show pit-window status pill",
    "fuel_calc.show_add": "Show fuel-to-add box",
    "fuel_calc.show_gauge": "Show fuel level gauge",
    "fuel_calc.show_stats": "Show usage table (avg / max / min)",
    "fuel_calc.show_strip": "Show pit-window timeline",
    "fuel_calc.show_time": "Show time-until-empty",
    "fuel_calc.show_laps": "Show laps-until-empty",
    "delta_bar.mode": "Reference lap",
    "delta_bar.range": "Full-scale delta (seconds)",
    "delta_bar.show_value": "Show numeric delta",
    "flags.idle_text": "Text when no flag is flying",
    "sector_timing.sectors": "Sector count (fallback)",
    "lap_compare.max_turns": "Max corners listed",
    "lap_compare.min_time_loss": "Min time delta to list a corner (s)",
    "lap_compare.show_live_delta": "Show live delta to best",
    "lap_compare.show_graph": "Show delta-over-distance trace",
    "show_pit_blends": "Show pit entry/exit lines",
    "show_pit_speed": "Show pit speed limit",
    "pit_lane_opacity": "Pit lane opacity",
    "pit_dot_opacity": "Pit car dot opacity",
    "dot_radius_frac": "Car dot size",
    "pit_blend": "Pit entry line",
    "pit_blend_out": "Pit exit line",
}

# Rows that only make sense when another toggle is on: maps a leaf's dotted path
# to (controller dotted path, required value). The row is hidden in the editor
# while the controller doesn't hold that value -- e.g. the pit speed/blend
# options vanish when the pit lane itself is hidden.
ROW_DEPENDENCIES = {
    "map.show_pit_speed": ("map.show_pit", True),
    "map.show_pit_blends": ("map.show_pit", True),
    "map.pit_lane_opacity": ("map.show_pit", True),
}
_DEP_CONTROLLERS = {ctrl for ctrl, _ in ROW_DEPENDENCIES.values()}


def _label_for(path: list) -> str:
    """Human label for a config leaf: an explicit override, else prettified."""
    dotted = ".".join(str(p) for p in path)
    return (LABEL_OVERRIDES.get(dotted)
            or LABEL_OVERRIDES.get(str(path[-1]))
            or _pretty(path[-1]))


# Special-cased word fixups so labels read naturally (RPM, iRating, ...).
_WORD_FIXUPS = {
    "rpm": "RPM", "sof": "SoF", "irating": "iRating", "sr": "SR", "ui": "UI",
    "id": "ID", "bg": "background", "frac": "fraction", "px": "size",
    "tau": "easing", "pct": "percent", "hz": "rate", "cpu": "CPU", "mem": "memory",
}

from .widgets.dash import METRIC_KEYS as _DASH_METRICS
# Items available for each table's header / footer sections. Every item works
# in any slot; order_pill / title / count are standings-specific extras.
_SLOT_COMMON = [
    "none", "sof", "class_sof", "position", "class_position",
    "session_time", "race_time", "lap", "incidents", "track_name",
    "track_temp", "air_temp", "best_lap", "session_best",
    "local_time", "sim_time", "cpu", "mem",
]
_SLOT_STANDINGS = _SLOT_COMMON + ["order_pill", "title", "count"]
SECTION_ITEMS = {
    ("relative", "header"): _SLOT_COMMON,
    ("relative", "footer"): _SLOT_COMMON,
    ("standings", "header"): _SLOT_STANDINGS,
    ("standings", "footer"): _SLOT_STANDINGS,
}
SECTION_KEYS = {"left", "center", "right"}

# Dash content slots: each picks any metric (or "none" to hide it).
DASH_SLOT_KEYS = {"top_right", "primary_left", "primary_right",
                  "stat_left", "stat_right",
                  "strip_left", "strip_center", "strip_right"}

# Friendly names for the reorderable table columns.
COLUMN_LABELS = {
    "badge": "Status badge", "position": "Position", "car_number": "Car number",
    "name": "Driver name", "license": "License", "irating": "iRating",
    "pit": "Pit", "gap": "Gap", "last_lap": "Last lap", "best_lap": "Best lap",
}

ACCENT = "#46df7a"        # neon green (matches the dash)
ACCENT_DIM = "#2f9d56"
ORANGE = "#ff9416"
YELLOW = "#ffd23a"
BLUE = "#4c9aff"

# A unique accent color per widget so each section reads like its own theme:
# its sliders, toggles and sidebar dot all take this color.
TAB_COLORS = {
    "General": "#9aa3b2",       # neutral gray (global settings)
    "App": "#9aa3b2",           # neutral gray (global, preset-independent)
    "Table": "#7f8c9a",         # slate (shared table base)
    "Relative": "#2fe0b0",      # teal
    "Standings": "#a98bff",     # purple
    "Laptime Log": YELLOW,      # yellow
    "Fuel Calc": ORANGE,        # orange
    "Radar": "#ff5b5b",         # red
    "Dash": ACCENT,             # green
    "Inputs": "#28cfe0",        # cyan
    "Delta Bar": "#9ee84b",     # lime
    "Flags": "#ff7ec2",         # pink
    "Lap Compare": "#ffb43a",   # amber
    "Sector Timing": "#e07bff", # magenta
    "Map": "#5aa9ff",           # blue
}

# Section keys shown under the "Settings" top tab (global, non-widget config).
# Everything else is a widget and lives under the "Widgets" top tab. "__app__"
# holds preset-independent settings (updates, preset auto-switching). "__scan__"
# is the write-access-only track/pit authoring tab (added at build time only when
# the user can write, so it stays absent for read-only users).
SETTINGS_SECTION_KEYS = {"__general__", "__app__", "__scan__"}

STYLE = f"""
QWidget {{ color: #d7dae0; font-family: 'Segoe UI', 'SF Pro Text', Arial; font-size: 12px; }}
QLabel#title {{ font-size: 21px; font-weight: 800; color: #f4f6f8; }}
QLabel#subtitle {{ color: #8b93a1; font-size: 11px; }}
QLabel#status {{ color: {ACCENT}; font-size: 11px; }}
QLabel#rowLabel {{ color: #c7cdd6; }}
QLabel#pageTitle {{ font-size: 16px; font-weight: 800; color: #f4f6f8; }}
QLabel#pageHint {{ color: #8b93a1; font-size: 11px; }}
QLabel#enableTitle {{ font-size: 13px; font-weight: 700; color: #f4f6f8; }}
QLabel#enableHint {{ color: #8b93a1; font-size: 11px; }}

QLineEdit#search {{
    background: rgba(20,23,28,0.85); border: 1px solid #2c313b; border-radius: 11px;
    padding: 10px 14px; color: #e6e8ec; selection-background-color: {ACCENT};
}}
QLineEdit#search:focus {{ border: 1px solid {ACCENT}; }}

QScrollArea {{ border: none; background: transparent; }}

/* Sidebar navigation rail */
QWidget#navRail {{
    background: rgba(13,15,19,0.78); border: 1px solid #20242c; border-radius: 14px;
}}

/* Enable card at the top of a widget page */
QFrame#enableCard {{
    background: rgba(18,21,27,0.85); border: 1px solid #262b34; border-radius: 13px;
}}

/* Accordion header buttons */
QPushButton#accordion {{
    background: rgba(20,23,29,0.85); border: 1px solid #262b34;
    border-radius: 11px; padding: 10px 14px; color: #cfd5de;
    text-align: left; font-size: 11px; font-weight: 800; letter-spacing: 0.6px;
}}
QPushButton#accordion:hover {{ border: 1px solid {ACCENT_DIM}; color: #f4f6f8; }}
QPushButton#accordion:checked {{ color: #f4f6f8; }}
QWidget#accordionBody {{
    background: rgba(13,16,20,0.55);
    border-left: 1px solid #20242c; border-right: 1px solid #20242c;
    border-bottom: 1px solid #20242c;
    border-bottom-left-radius: 11px; border-bottom-right-radius: 11px;
}}

QLineEdit, QComboBox, QSpinBox, QDoubleSpinBox {{
    background: rgba(29,33,40,0.92); border: 1px solid #2c313b; border-radius: 9px;
    padding: 6px 10px; color: #e6e8ec; min-height: 18px;
}}
QLineEdit:focus, QComboBox:focus, QSpinBox:focus, QDoubleSpinBox:focus {{
    border: 1px solid {ACCENT};
}}
QComboBox {{ min-width: 150px; padding-right: 26px; }}
/* The chevron is custom-painted by Combo; hide the native arrow + box. */
QComboBox::drop-down {{ border: none; width: 26px; }}
QComboBox::down-arrow {{ image: none; width: 0; height: 0; }}
QComboBox QAbstractItemView {{
    background: #161a20; border: 1px solid #2c313b; color: #e6e8ec;
    selection-background-color: {ACCENT}; selection-color: #06210f; outline: none;
}}
QSpinBox::up-button, QSpinBox::down-button,
QDoubleSpinBox::up-button, QDoubleSpinBox::down-button {{ width: 16px; border: none; }}

/* Sliders */
QSlider::groove:horizontal {{
    height: 5px; border-radius: 3px; background: #232831;
}}
QSlider::sub-page:horizontal {{
    height: 5px; border-radius: 3px;
    background: qlineargradient(x1:0, y1:0, x2:1, y2:0, stop:0 {ACCENT_DIM}, stop:1 {ACCENT});
}}
QSlider::add-page:horizontal {{ height: 5px; border-radius: 3px; background: #232831; }}
QSlider::handle:horizontal {{
    width: 16px; height: 16px; margin: -6px 0; border-radius: 8px;
    background: #f4f6f8; border: 2px solid {ACCENT};
}}
QSlider::handle:horizontal:hover {{ background: #ffffff; }}

QPushButton {{
    background: rgba(34,39,50,0.9); border: 1px solid #2f3540; border-radius: 10px;
    padding: 9px 16px; color: #dfe3ea;
}}
QPushButton:hover {{ background: #2a3140; }}
QPushButton#primary {{
    background: {BLUE}; border: 1px solid {BLUE}; color: #06121f; font-weight: 700;
}}
QPushButton#primary:hover {{ background: #5ea7ff; }}
QPushButton#warn {{
    background: transparent; border: 1px solid {YELLOW}; color: {YELLOW}; font-weight: 600;
}}
QPushButton#warn:hover {{ background: rgba(255,210,58,0.12); }}
QPushButton#danger {{
    background: transparent; border: 1px solid {ORANGE}; color: {ORANGE}; font-weight: 600;
}}
QPushButton#danger:hover {{ background: rgba(255,148,22,0.12); }}
QPushButton#go {{
    background: qlineargradient(x1:0, y1:0, x2:0, y2:1, stop:0 #5cf08c, stop:1 {ACCENT});
    border: 1px solid {ACCENT}; color: #06210f; font-weight: 700;
}}
QPushButton#go:hover {{ background: #5cf08c; }}
QPushButton#stop {{
    background: transparent; border: 1px solid {ORANGE}; color: {ORANGE}; font-weight: 700;
}}
QPushButton#stop:hover {{ background: rgba(255,148,22,0.12); }}

/* Top-level horizontal tabs (Widgets / Settings) */
QWidget#topTabs {{
    background: rgba(13,15,19,0.78); border: 1px solid #20242c; border-radius: 12px;
}}
QPushButton#topTab {{
    background: transparent; border: none; border-radius: 9px;
    padding: 9px 20px; color: #8b93a1; font-size: 12px; font-weight: 800;
    letter-spacing: 0.4px;
}}
QPushButton#topTab:hover {{ color: #d7dae0; }}
QPushButton#topTab:checked {{
    background: rgba(70,223,122,0.16); border: 1px solid {ACCENT_DIM}; color: #f4f6f8;
}}

QListWidget#orderList {{
    background: rgba(20,23,28,0.72); border: 1px solid #2c313b; border-radius: 10px;
    padding: 4px; outline: none;
}}
QListWidget#orderList::item {{
    background: rgba(34,39,50,0.85); border: 1px solid #2f3540; border-radius: 7px;
    padding: 8px 10px; margin: 2px 1px; color: #dfe3ea;
}}
QListWidget#orderList::item:hover {{ border: 1px solid {ACCENT_DIM}; }}
QListWidget#orderList::item:selected {{
    background: qlineargradient(x1:0, y1:0, x2:0, y2:1,
        stop:0 rgba(70,223,122,0.30), stop:1 rgba(70,223,122,0.16));
    border: 1px solid {ACCENT}; color: #f4f6f8;
}}

QScrollBar:vertical {{ background: transparent; width: 11px; margin: 2px; }}
QScrollBar::handle:vertical {{ background: #2c313b; border-radius: 5px; min-height: 32px; }}
QScrollBar::handle:vertical:hover {{ background: {ACCENT_DIM}; }}
QScrollBar::add-line, QScrollBar::sub-line {{ height: 0; }}
QScrollBar::add-page, QScrollBar::sub-page {{ background: transparent; }}
"""

_CARBON: QPixmap | None = None


def _carbon_tile() -> QPixmap:
    """A small carbon-fiber weave tile used as the window background."""
    global _CARBON
    if _CARBON is not None:
        return _CARBON
    cell = 6
    size = cell * 2
    pm = QPixmap(size, size)
    pm.fill(QColor("#0d0f12"))
    p = QPainter(pm)

    def weave(x, y, flip):
        g = (QLinearGradient(x + cell, y, x, y + cell) if flip
             else QLinearGradient(x, y, x + cell, y + cell))
        g.setColorAt(0.0, QColor("#20242b"))
        g.setColorAt(0.5, QColor("#161a20"))
        g.setColorAt(1.0, QColor("#0b0d11"))
        p.fillRect(QRectF(x, y, cell, cell), QBrush(g))

    weave(0, 0, False)
    weave(cell, cell, False)
    weave(cell, 0, True)
    weave(0, cell, True)
    p.end()
    _CARBON = pm
    return pm


class _WheelGuard:
    """Mixin: the mouse wheel only changes the control once it has focus.

    Sliders, spin boxes and combos otherwise eat wheel scrolls while you're just
    trying to scroll the settings page, nudging values by accident. With this,
    an unfocused control ignores the wheel so it bubbles up to the scroll area;
    click (or tab) into the control first to adjust it with the wheel.
    """

    def __init__(self, *args, **kwargs):
        super().__init__(*args, **kwargs)
        self.setFocusPolicy(Qt.FocusPolicy.StrongFocus)

    def wheelEvent(self, event):  # noqa: N802 (Qt naming)
        if self.hasFocus():
            super().wheelEvent(event)
        else:
            event.ignore()


class _Slider(_WheelGuard, QSlider):
    pass


class _SpinBox(_WheelGuard, QSpinBox):
    pass


class _DoubleSpinBox(_WheelGuard, QDoubleSpinBox):
    pass


class Combo(_WheelGuard, QComboBox):
    """A dropdown that paints its own chevron, flipping up while the list is open."""

    def __init__(self, parent=None):
        super().__init__(parent)
        self._open = False

    def showPopup(self) -> None:  # noqa: N802 (Qt naming)
        self._open = True
        self.update()
        super().showPopup()

    def hidePopup(self) -> None:  # noqa: N802 (Qt naming)
        self._open = False
        self.update()
        super().hidePopup()

    def paintEvent(self, event) -> None:  # noqa: N802 (Qt naming)
        super().paintEvent(event)  # field + text (native arrow is hidden via QSS)
        p = QPainter(self)
        p.setRenderHint(QPainter.RenderHint.Antialiasing)
        pen = QPen(QColor("#aab2bf"), 1.6)
        pen.setCapStyle(Qt.PenCapStyle.RoundCap)
        pen.setJoinStyle(Qt.PenJoinStyle.RoundJoin)
        p.setPen(pen)
        cx = self.width() - 14.0
        cy = self.height() / 2.0
        s = 4.0
        if self._open:  # chevron up
            p.drawLine(QPointF(cx - s, cy + s * 0.55), QPointF(cx, cy - s * 0.55))
            p.drawLine(QPointF(cx, cy - s * 0.55), QPointF(cx + s, cy + s * 0.55))
        else:           # chevron down
            p.drawLine(QPointF(cx - s, cy - s * 0.55), QPointF(cx, cy + s * 0.55))
            p.drawLine(QPointF(cx, cy + s * 0.55), QPointF(cx + s, cy - s * 0.55))
        p.end()


class ToggleSwitch(QAbstractButton):
    """A modern animated sliding on/off switch (replaces checkboxes)."""

    def __init__(self, checked: bool = False, accent: str = ACCENT, parent=None):
        super().__init__(parent)
        self.setCheckable(True)
        self.setCursor(Qt.CursorShape.PointingHandCursor)
        self._accent = QColor(accent)
        self._track_off = QColor("#3a4150")
        self._knob = QColor("#f6f8fb")
        self._w, self._h = 46, 26
        self.setFixedSize(self._w, self._h)
        self._pos = 1.0 if checked else 0.0
        self.setChecked(checked)
        self.toggled.connect(self._animate)

    def set_accent(self, color: str) -> None:
        self._accent = QColor(color)
        self.update()

    def set_checked_silent(self, on: bool) -> None:
        """Set the state + knob position without emitting toggled (no animation).

        Programmatic setChecked() with signals blocked skips the toggled-driven
        animation, so the knob would otherwise stay put; sync _pos directly.
        """
        self.blockSignals(True)
        self.setChecked(on)
        self.blockSignals(False)
        self._pos = 1.0 if on else 0.0
        self.update()

    def _animate(self, on: bool) -> None:
        self._anim = QPropertyAnimation(self, b"pos", self)
        self._anim.setDuration(150)
        self._anim.setEasingCurve(QEasingCurve.Type.InOutCubic)
        self._anim.setStartValue(self._pos)
        self._anim.setEndValue(1.0 if on else 0.0)
        self._anim.start()

    def _get_pos(self) -> float:
        return self._pos

    def _set_pos(self, v: float) -> None:
        self._pos = v
        self.update()

    pos = pyqtProperty(float, fget=_get_pos, fset=_set_pos)

    def sizeHint(self):  # noqa: N802
        return QSize(self._w, self._h)

    def hitButton(self, pos):  # noqa: N802
        return self.rect().contains(pos)

    def paintEvent(self, _event):  # noqa: N802
        p = QPainter(self)
        p.setRenderHint(QPainter.RenderHint.Antialiasing)
        t = self._pos
        off, on = self._track_off, self._accent
        track = QColor(
            int(off.red() + (on.red() - off.red()) * t),
            int(off.green() + (on.green() - off.green()) * t),
            int(off.blue() + (on.blue() - off.blue()) * t),
        )
        p.setPen(Qt.PenStyle.NoPen)
        p.setBrush(track)
        p.drawRoundedRect(QRectF(0, 0, self._w, self._h), self._h / 2, self._h / 2)
        d = self._h - 6
        x = 3 + t * (self._w - d - 6)
        p.setBrush(self._knob)
        p.drawEllipse(QRectF(x, 3, d, d))
        p.end()


def _section_reset_qss(accent: str) -> str:
    """A small ghost button tinted with the section's accent color."""
    return (
        "QPushButton#sectionReset {"
        f" color:{accent}; background:transparent;"
        f" border:1px solid {accent}55; border-radius:6px;"
        " padding:4px 12px; font-size:12px; font-weight:600; }"
        "QPushButton#sectionReset:hover {"
        f" background:{accent}22; border:1px solid {accent}; }}"
    )


def _num_range(path: list, default):
    """Guess a friendly (lo, hi, step) slider range from a key name + value."""
    key = str(path[-1]).lower()
    is_float = isinstance(default, float)
    if key == "dot_radius_frac":
        # A fine, small-valued range (0.05 == default size) rather than 0..1.
        lo, hi, step = 0.01, 0.15, 0.005
    elif any(s in key for s in ("frac", "opacity")) or key.endswith("_pct") or "tau" in key:
        lo, hi, step = 0.0, 1.0, 0.01
    elif "scale" in key:
        lo, hi, step = 0.2, 3.0, 0.05
    elif key.endswith("_hz"):
        lo, hi, step = 0.0, 20.0, 0.5
    elif "seconds" in key:
        lo, hi, step = 0.0, 10.0, 0.5
    elif "range" in key:
        lo, hi, step = 0.0, 5.0, 0.1
    elif "threshold" in key or "percent" in key:
        lo, hi, step = 0, 100, 1
    elif "segments" in key:
        lo, hi, step = 1, 48, 1
    elif "width" in key:
        lo, hi, step = 0, 40, 1
    elif key in ("rows", "rows_ahead", "rows_behind", "history_laps"):
        lo, hi, step = 0, 30, 1
    elif key == "row_height_px":
        lo, hi, step = 0, 72, 1
    elif key.endswith("px"):
        lo, hi, step = 6, 48, 1
    elif is_float:
        lo, hi, step = 0.0, max(2.0, abs(default) * 4 or 2.0), 0.05
    else:
        lo, hi, step = 0, int(max(10, abs(default) * 4 or 10)), 1
    lo = min(lo, default)
    hi = max(hi, default)
    return (float(lo), float(hi), float(step)) if is_float else (int(lo), int(hi), int(step))


class NumberControl(QWidget):
    """A slider paired with a precise spin box for any numeric value."""

    def __init__(self, path: list, default, value, on_change, accent: str = ACCENT):
        super().__init__()
        self.is_float = isinstance(default, float)
        self.lo, self.hi, self.step = _num_range(path, default)
        if self.step <= 0:
            self.step = 0.01 if self.is_float else 1
        self.steps = max(1, int(round((self.hi - self.lo) / self.step)))
        self._on_change = on_change

        h = QHBoxLayout(self)
        h.setContentsMargins(0, 0, 0, 0)
        h.setSpacing(12)

        self.slider = _Slider(Qt.Orientation.Horizontal)
        self.slider.setRange(0, self.steps)
        self.slider.setValue(self._to_slider(value))
        self.slider.setMinimumWidth(140)
        self.slider.setCursor(Qt.CursorShape.PointingHandCursor)
        # Tint the fill + handle with this widget's accent color (overrides the
        # global green slider style for this instance only).
        self.slider.setStyleSheet(
            "QSlider::groove:horizontal { height:5px; border-radius:3px;"
            " background:#232831; }"
            "QSlider::add-page:horizontal { height:5px; border-radius:3px;"
            " background:#232831; }"
            "QSlider::sub-page:horizontal { height:5px; border-radius:3px;"
            f" background:{accent}; }}"
            "QSlider::handle:horizontal { width:16px; height:16px; margin:-6px 0;"
            f" border-radius:8px; background:#f4f6f8; border:2px solid {accent}; }}"
            "QSlider::handle:horizontal:hover { background:#ffffff; }")

        # Coerce the stored value to the spin's type: a QSpinBox rejects floats,
        # and a config edited by hand (or left over from an older default) can
        # carry the "wrong" numeric type for a key.
        if not isinstance(value, (int, float)):
            value = default
        if self.is_float:
            self.spin = _DoubleSpinBox()
            self.spin.setDecimals(3)
            self.spin.setRange(-1_000_000.0, 1_000_000.0)
            self.spin.setSingleStep(self.step)
            self.spin.setValue(float(value))
        else:
            self.spin = _SpinBox()
            self.spin.setRange(-1_000_000, 1_000_000)
            self.spin.setSingleStep(max(1, int(self.step)))
            self.spin.setValue(int(round(value)))
        self.spin.setFixedWidth(94)

        self.slider.valueChanged.connect(self._slider_changed)
        self.spin.valueChanged.connect(self._spin_changed)
        h.addWidget(self.slider, 1)
        h.addWidget(self.spin)

    def _to_slider(self, val) -> int:
        return max(0, min(self.steps, int(round((val - self.lo) / self.step))))

    def _from_slider(self, s: int):
        v = self.lo + s * self.step
        return round(v, 3) if self.is_float else int(round(v))

    def _slider_changed(self, s: int) -> None:
        v = self._from_slider(s)
        self.spin.blockSignals(True)
        self.spin.setValue(v)
        self.spin.blockSignals(False)
        self._on_change(v)

    def _spin_changed(self, v) -> None:
        v = float(v) if self.is_float else int(v)
        self.slider.blockSignals(True)
        self.slider.setValue(self._to_slider(v))
        self.slider.blockSignals(False)
        self._on_change(v)


class CollapsibleSection(QWidget):
    """A titled accordion: click the header to expand/collapse its body."""

    def __init__(self, title: str, accent: str = ACCENT, expanded: bool = True):
        super().__init__()
        self._title = title
        self._default_expanded = expanded
        self._search = title.lower()

        v = QVBoxLayout(self)
        v.setContentsMargins(0, 0, 0, 0)
        v.setSpacing(0)

        self.header = QPushButton(self._fmt(expanded))
        self.header.setObjectName("accordion")
        self.header.setCheckable(True)
        self.header.setChecked(expanded)
        self.header.setCursor(Qt.CursorShape.PointingHandCursor)
        self.header.toggled.connect(self._toggle)
        v.addWidget(self.header)

        self.body = QWidget()
        self.body.setObjectName("accordionBody")
        self._body_lay = QVBoxLayout(self.body)
        self._body_lay.setContentsMargins(12, 10, 12, 12)
        self._body_lay.setSpacing(7)
        v.addWidget(self.body)
        self.body.setVisible(expanded)

    def _fmt(self, expanded: bool) -> str:
        return ("\u25BE   " if expanded else "\u25B8   ") + self._title.upper()

    def _toggle(self, on: bool) -> None:
        self.body.setVisible(on)
        self.header.setText(self._fmt(on))

    def setExpanded(self, on: bool) -> None:  # noqa: N802
        if self.header.isChecked() != on:
            self.header.setChecked(on)

    def body_layout(self) -> QVBoxLayout:
        return self._body_lay


class NavItem(QWidget):
    """A clickable sidebar row with a section color dot and selection highlight."""

    clicked = pyqtSignal()

    def __init__(self, title: str, color: str):
        super().__init__()
        self._color = QColor(color)
        self._selected = False
        self._dot_on = True
        self.setCursor(Qt.CursorShape.PointingHandCursor)
        self.setFixedHeight(40)

        h = QHBoxLayout(self)
        h.setContentsMargins(18, 0, 16, 0)
        h.setSpacing(10)
        self.label = QLabel(title)
        self.label.setStyleSheet("background: transparent; color: #aab2bf;")
        h.addWidget(self.label)
        h.addStretch(1)

    def set_dot(self, on: bool) -> None:
        self._dot_on = on
        self.update()

    def setSelected(self, sel: bool) -> None:  # noqa: N802
        self._selected = sel
        f = self.label.font()
        f.setBold(sel)
        self.label.setFont(f)
        self.label.setStyleSheet(
            "background: transparent; color: %s;"
            % ("#f4f6f8" if sel else "#aab2bf"))
        self.update()

    def mousePressEvent(self, _event):  # noqa: N802
        self.clicked.emit()

    def paintEvent(self, _event):  # noqa: N802
        p = QPainter(self)
        p.setRenderHint(QPainter.RenderHint.Antialiasing)
        r = QRectF(self.rect()).adjusted(6, 3, -6, -3)
        if self._selected:
            fill = QColor(self._color)
            fill.setAlpha(34)
            p.setBrush(fill)
            p.setPen(QPen(QColor(self._color), 1.3))
            p.drawRoundedRect(r, 10, 10)
            p.setPen(Qt.PenStyle.NoPen)
            p.setBrush(self._color)
            p.drawRoundedRect(QRectF(r.left() + 3, r.center().y() - 8, 3, 16), 1.5, 1.5)
        # Always the widget's own color; dimmed (translucent) when the widget is
        # off so the dot still doubles as an enabled/disabled indicator.
        dot = QColor(self._color)
        if not self._dot_on:
            dot.setAlpha(70)
        p.setPen(Qt.PenStyle.NoPen)
        p.setBrush(dot)
        p.drawEllipse(QPointF(r.right() - 6, r.center().y()), 4.0, 4.0)
        p.end()


def _enum_options(path: list):
    """Return dropdown options for a path, or None if it isn't an enum."""
    if not path:
        return None
    key = path[-1]
    if key in ENUMS:
        return ENUMS[key]
    if path[0] == "dash" and key in DASH_SLOT_KEYS:
        return list(_DASH_METRICS)
    if key in SECTION_KEYS and len(path) >= 3:
        return SECTION_ITEMS.get((path[0], path[-2]))
    return None


def _pretty(key: str) -> str:
    """Turn a config key into a readable label (RPM, iRating, Text Scale, ...)."""
    words = []
    for i, w in enumerate(str(key).split("_")):
        fix = _WORD_FIXUPS.get(w.lower())
        if fix is not None:
            words.append(fix if i else fix[0].upper() + fix[1:])
        else:
            words.append(w.capitalize())
    return " ".join(words)


def _option_label(value) -> str:
    """Friendly display text for a dropdown option value."""
    return OPTION_LABELS.get(value, _pretty(value))


def _sort_combo_options(options: list, label_fn=_option_label) -> list:
    return sorted(options, key=lambda v: label_fn(v).casefold())


def _is_color(path: list, value) -> bool:
    if len(path) >= 2 and path[-2] in COLOR_PARENTS:
        return True
    return isinstance(value, str) and (value.startswith("#") or value.startswith("rgba("))


def _to_hex(c: QColor) -> str:
    if c.alpha() >= 255:
        return f"#{c.red():02x}{c.green():02x}{c.blue():02x}"
    return f"#{c.red():02x}{c.green():02x}{c.blue():02x}{c.alpha():02x}"


def _get_at(d: dict, path: list):
    for k in path:
        d = d[k]
    return d


def _set_at(d: dict, path: list, value) -> None:
    for k in path[:-1]:
        d = d[k]
    d[path[-1]] = value


class ColorButton(QPushButton):
    """A swatch button that opens a color picker (with alpha)."""

    def __init__(self, value: str, on_pick):
        super().__init__()
        self._on_pick = on_pick
        self.setFixedSize(150, 28)
        self.setCursor(Qt.CursorShape.PointingHandCursor)
        self.set_value(value)
        self.clicked.connect(self._pick)

    def set_value(self, value: str) -> None:
        self._value = value
        c = config.qcolor(value)
        text_col = "#101319" if c.lightness() > 130 else "#f2f4f7"
        self.setText(f"  {value}")
        self.setStyleSheet(
            "QPushButton {"
            f" background-color: rgba({c.red()},{c.green()},{c.blue()},{c.alpha()});"
            f" color: {text_col};"
            " border: 1px solid #2c313b; border-radius: 8px; text-align: left;"
            " font-family: monospace; font-size: 11px; padding: 5px 9px; }"
        )

    def _pick(self) -> None:
        c = QColorDialog.getColor(
            config.qcolor(self._value), self, "Pick color",
            QColorDialog.ColorDialogOption.ShowAlphaChannel,
        )
        if c.isValid():
            hexv = _to_hex(c)
            self.set_value(hexv)
            self._on_pick(hexv)


class PaletteEditor(QWidget):
    """Editable list of colors (the track-map car palette)."""

    def __init__(self, values: list, on_change):
        super().__init__()
        self._values = list(values)
        self._on_change = on_change
        self._lay = QVBoxLayout(self)
        self._lay.setContentsMargins(0, 0, 0, 0)
        self._lay.setSpacing(6)
        self._rebuild()

    def _rebuild(self) -> None:
        while self._lay.count():
            item = self._lay.takeAt(0)
            if item.widget():
                item.widget().deleteLater()
        for i, val in enumerate(self._values):
            row = QHBoxLayout()
            btn = ColorButton(val, lambda v, idx=i: self._set(idx, v))
            rem = QPushButton("\u2715")
            rem.setFixedSize(28, 28)
            rem.setCursor(Qt.CursorShape.PointingHandCursor)
            rem.clicked.connect(lambda _=False, idx=i: self._remove(idx))
            row.addWidget(btn)
            row.addWidget(rem)
            row.addStretch(1)
            holder = QWidget()
            holder.setLayout(row)
            self._lay.addWidget(holder)
        add = QPushButton("+  Add color")
        add.setCursor(Qt.CursorShape.PointingHandCursor)
        add.clicked.connect(self._add)
        self._lay.addWidget(add)

    def _set(self, idx, value):
        self._values[idx] = value
        self._on_change(list(self._values))

    def _remove(self, idx):
        if len(self._values) > 1:
            del self._values[idx]
            self._rebuild()
            self._on_change(list(self._values))

    def _add(self):
        self._values.append("#ffffff")
        self._rebuild()
        self._on_change(list(self._values))


class OrderEditor(QWidget):
    """Drag-to-reorder list of keys with add/remove (e.g. table columns).

    Drag rows to reorder, pick from the dropdown and press Add to insert a
    column that isn't shown yet, or select a row and press Remove to hide it.
    """

    def __init__(self, values: list, labels: dict, all_keys: list, on_change):
        super().__init__()
        self._labels = labels
        self._all_keys = list(all_keys)
        self._on_change = on_change

        v = QVBoxLayout(self)
        v.setContentsMargins(0, 0, 0, 0)
        v.setSpacing(7)

        self.list = QListWidget()
        self.list.setObjectName("orderList")
        self.list.setDragDropMode(QAbstractItemView.DragDropMode.InternalMove)
        self.list.setSelectionMode(QAbstractItemView.SelectionMode.SingleSelection)
        self.list.setUniformItemSizes(True)
        self.list.setMinimumHeight(150)
        self.list.model().rowsMoved.connect(lambda *_: self._emit())
        v.addWidget(self.list)

        controls = QHBoxLayout()
        controls.setSpacing(6)
        self.add_combo = Combo()
        self.add_btn = QPushButton("+  Add")
        self.remove_btn = QPushButton("\u2715  Remove")
        for b in (self.add_btn, self.remove_btn):
            b.setCursor(Qt.CursorShape.PointingHandCursor)
        self.add_btn.clicked.connect(self._add)
        self.remove_btn.clicked.connect(self._remove)
        controls.addWidget(self.add_combo, 1)
        controls.addWidget(self.add_btn)
        controls.addWidget(self.remove_btn)
        v.addLayout(controls)

        for key in values:
            self._add_item(key)
        self._refresh_controls()

    def _add_item(self, key: str) -> None:
        item = QListWidgetItem(self._labels.get(key, _pretty(key)))
        item.setData(Qt.ItemDataRole.UserRole, key)
        self.list.addItem(item)

    def _current_keys(self) -> list:
        return [self.list.item(i).data(Qt.ItemDataRole.UserRole)
                for i in range(self.list.count())]

    def _refresh_controls(self) -> None:
        present = set(self._current_keys())
        self.add_combo.clear()
        available = _sort_combo_options(
            [k for k in self._all_keys if k not in present],
            lambda k: self._labels.get(k, _pretty(k)),
        )
        for k in available:
            self.add_combo.addItem(self._labels.get(k, _pretty(k)), k)
        self.add_combo.setEnabled(bool(available))
        self.add_btn.setEnabled(bool(available))
        self.remove_btn.setEnabled(self.list.count() > 1)

    def _emit(self) -> None:
        self._refresh_controls()
        self._on_change(self._current_keys())

    def _add(self) -> None:
        key = self.add_combo.currentData()
        if key:
            self._add_item(key)
            self._emit()

    def _remove(self) -> None:
        row = self.list.currentRow()
        if row < 0:
            row = self.list.count() - 1
        if row >= 0 and self.list.count() > 1:
            self.list.takeItem(row)
            self._emit()


class ConfigEditor(QWidget):
    def __init__(self, parent=None, overlay=None):
        super().__init__(parent)
        # Optional overlay controller (the running HUD) so the settings window
        # can start/stop the widgets. None when launched standalone.
        self._overlay = overlay
        self.setObjectName("root")
        self.setWindowTitle("GridGlance Settings")
        self.resize(880, 820)
        self.setMinimumSize(720, 560)
        self.setStyleSheet(STYLE)

        self._edit_ctx = config.active_context()
        self.working = config.editor_full(self._edit_ctx)
        self._rows: list[dict] = []          # {widget, text, accordions}
        self._accordions: list[CollapsibleSection] = []
        self._nav_items: dict[str, NavItem] = {}
        self._sections: list[tuple[str, str]] = []
        self._cur_index = 0
        self._top_tab = "widgets"
        self._carbon = _carbon_tile()
        self._updater = None
        self._dl_dialog = None
        self._dl_canceled = False
        # Friendly names for car paths / league ids bound to presets
        # (best-effort, from the running overlay when a session is detected).
        self._car_labels: dict[str, str] = {}
        self._league_labels: dict[int, str] = {}

        root = QVBoxLayout(self)
        root.setContentsMargins(18, 16, 18, 14)
        root.setSpacing(12)

        title = QLabel("Overlay Settings")
        title.setObjectName("title")
        subtitle = QLabel("Customize every widget \u2022 changes apply live")
        subtitle.setObjectName("subtitle")
        root.addWidget(title)
        root.addWidget(subtitle)

        # Preset selector: each preset is a complete, independent config + layout
        # (e.g. a "League" set vs a "Practice" set, or different cars).
        root.addLayout(self._build_preset_bar())

        # Profile selector: edit the on-track vs in-garage configuration.
        prof = QHBoxLayout()
        prof.setSpacing(8)
        plabel = QLabel("Profile")
        plabel.setStyleSheet("background: transparent; color: #aab2bf; "
                             "font-weight: 700;")
        self.ctx_combo = Combo()
        self.ctx_combo.setObjectName("ctxCombo")
        for ctx in _sort_combo_options(
                list(config.contexts()),
                lambda c: config.CONTEXT_LABELS.get(c, c)):
            self.ctx_combo.addItem(config.CONTEXT_LABELS.get(ctx, ctx), ctx)
        i = self.ctx_combo.findData(self._edit_ctx)
        self.ctx_combo.setCurrentIndex(max(0, i))
        self.ctx_combo.currentIndexChanged.connect(self._change_ctx)
        self.ctx_hint = QLabel("")
        self.ctx_hint.setObjectName("subtitle")
        self.ctx_hint.setWordWrap(True)
        prof.addWidget(plabel)
        prof.addWidget(self.ctx_combo)
        prof.addWidget(self.ctx_hint, 1)
        root.addLayout(prof)

        self.search = QLineEdit()
        self.search.setObjectName("search")
        self.search.setPlaceholderText("\U0001F50D  Search settings\u2026")
        self.search.setClearButtonEnabled(True)
        self.search.textChanged.connect(self._filter)
        root.addWidget(self.search)

        # Top-level tabs split the sidebar into widget pages vs global settings.
        tabbar = QWidget()
        tabbar.setObjectName("topTabs")
        tab_lay = QHBoxLayout(tabbar)
        tab_lay.setContentsMargins(6, 6, 6, 6)
        tab_lay.setSpacing(6)
        self._top_tabs: dict[str, QPushButton] = {}
        for name, label in (("widgets", "Widgets"), ("settings", "Settings")):
            b = QPushButton(label)
            b.setObjectName("topTab")
            b.setCheckable(True)
            b.setCursor(Qt.CursorShape.PointingHandCursor)
            b.clicked.connect(lambda _c=False, n=name: self._set_top_tab(n))
            tab_lay.addWidget(b)
            self._top_tabs[name] = b
        tab_lay.addStretch(1)
        root.addWidget(tabbar)

        # Sidebar navigation rail + stacked pages.
        body = QHBoxLayout()
        body.setSpacing(12)
        self.nav_rail = QWidget()
        self.nav_rail.setObjectName("navRail")
        self.nav_rail.setFixedWidth(196)
        self.nav_lay = QVBoxLayout(self.nav_rail)
        self.nav_lay.setContentsMargins(8, 12, 8, 12)
        self.nav_lay.setSpacing(2)
        self.stack = QStackedWidget()
        body.addWidget(self.nav_rail)
        body.addWidget(self.stack, 1)
        root.addLayout(body, 1)

        self.status = QLabel("")
        self.status.setObjectName("status")
        root.addWidget(self.status)
        self._status_timer = QTimer(self)
        self._status_timer.setSingleShot(True)
        self._status_timer.timeout.connect(lambda: self.status.setText(""))

        # Debounce disk writes so dragging a slider doesn't save on every step.
        self._save_timer = QTimer(self)
        self._save_timer.setSingleShot(True)
        self._save_timer.timeout.connect(self._autosave)

        controls = QHBoxLayout()
        controls.setSpacing(10)
        self.overlay_btn = QPushButton("Start Overlay")
        self.overlay_btn.setObjectName("go")
        self.overlay_btn.setCursor(Qt.CursorShape.PointingHandCursor)
        self.overlay_btn.clicked.connect(self._toggle_overlay)
        controls.addWidget(self.overlay_btn)
        self.edit_w, self.edit_sw = self._opt_toggle("Edit layout", False)
        self.edit_w.setToolTip("Make the overlay widgets draggable so you can "
                               "move and resize them; turn off to lock.")
        self.edit_sw.toggled.connect(self._toggle_edit)
        controls.addWidget(self.edit_w)
        if self._overlay is None:
            self.overlay_btn.hide()
            self.edit_w.hide()
        else:
            self._refresh_overlay_btn()
            self._sync_edit_switch()
        live_w, self.live_sw = self._opt_toggle("Apply live", True)
        controls.addWidget(live_w)
        save_w, self.autosave_sw = self._opt_toggle("Auto-save", True)
        controls.addWidget(save_w)
        controls.addStretch(1)
        for text, slot, oname in (
            ("Reset", self._reset, "danger"),
            ("Reload", self._reload, ""),
            ("Apply", self._apply, "warn"),
            ("Save", self._save, "primary"),
        ):
            b = QPushButton(text)
            b.setCursor(Qt.CursorShape.PointingHandCursor)
            if oname:
                b.setObjectName(oname)
            b.clicked.connect(slot)
            controls.addWidget(b)
        root.addLayout(controls)

        self._build_nav_and_pages()
        self._filter(self.search.text())  # apply row dependencies on first show
        self._update_ctx_hint()

    # --- background ---------------------------------------------------------

    def paintEvent(self, event):  # noqa: N802
        p = QPainter(self)
        p.drawTiledPixmap(self.rect(), self._carbon)
        # Subtle top sheen + darker edges for depth over the carbon weave.
        g = QLinearGradient(0, 0, 0, self.height())
        g.setColorAt(0.0, QColor(255, 255, 255, 12))
        g.setColorAt(0.22, QColor(0, 0, 0, 0))
        g.setColorAt(1.0, QColor(0, 0, 0, 70))
        p.fillRect(self.rect(), g)

    # --- small helpers ------------------------------------------------------

    def _opt_toggle(self, text: str, checked: bool):
        """A compact labeled switch for the app-level options bar."""
        w = QWidget()
        h = QHBoxLayout(w)
        h.setContentsMargins(0, 0, 0, 0)
        h.setSpacing(7)
        sw = ToggleSwitch(checked=checked, accent=ACCENT)
        lbl = QLabel(text)
        lbl.setStyleSheet("background: transparent; color: #aab2bf;")
        h.addWidget(sw)
        h.addWidget(lbl)
        return w, sw

    # --- updates ------------------------------------------------------------

    def _about_card(self) -> QFrame:
        """Version + 'Check for updates' card at the top of the General page."""
        card = QFrame()
        card.setObjectName("enableCard")
        h = QHBoxLayout(card)
        h.setContentsMargins(15, 11, 15, 11)
        h.setSpacing(12)
        texts = QVBoxLayout()
        texts.setSpacing(1)
        t = QLabel(f"GridGlance  v{version.__version__}")
        t.setObjectName("enableTitle")
        self._update_status = QLabel("Check GitHub for the latest version.")
        self._update_status.setObjectName("enableHint")
        self._update_status.setWordWrap(True)
        texts.addWidget(t)
        texts.addWidget(self._update_status)
        h.addLayout(texts, 1)
        self._update_btn = QPushButton("Check for Updates")
        self._update_btn.setObjectName("primary")
        self._update_btn.setCursor(Qt.CursorShape.PointingHandCursor)
        self._update_btn.clicked.connect(self._check_updates)
        h.addWidget(self._update_btn, 0, Qt.AlignmentFlag.AlignVCenter)
        # Offer an in-app uninstall only for an installed Windows build (where
        # the Inno Setup uninstaller exists beside the exe).
        if paths.uninstaller_path():
            uninstall = QPushButton("Uninstall")
            uninstall.setObjectName("danger")
            uninstall.setCursor(Qt.CursorShape.PointingHandCursor)
            uninstall.clicked.connect(self._uninstall)
            h.addWidget(uninstall, 0, Qt.AlignmentFlag.AlignVCenter)
        return card

    def _uninstall(self) -> None:
        """Confirm, then close the app and launch the Windows uninstaller."""
        from PyQt6.QtWidgets import QApplication, QMessageBox
        path = paths.uninstaller_path()
        if not path:
            return
        box = QMessageBox(self)
        box.setWindowTitle("Uninstall GridGlance")
        box.setIcon(QMessageBox.Icon.Warning)
        box.setText("Uninstall GridGlance?")
        box.setInformativeText(
            "This closes the app and starts the Windows uninstaller. Your saved "
            "settings and learned tracks (in your user folder) are left in place.")
        box.setStandardButtons(
            QMessageBox.StandardButton.Yes | QMessageBox.StandardButton.No)
        box.setDefaultButton(QMessageBox.StandardButton.No)
        if box.exec() != QMessageBox.StandardButton.Yes:
            return
        try:
            import subprocess
            subprocess.Popen([path], close_fds=True)  # noqa: S603
        except OSError as exc:
            QMessageBox.critical(self, "Uninstall failed",
                                 f"Couldn't start the uninstaller.\n\n{exc}")
            return
        QApplication.instance().quit()

    def _ensure_updater(self):
        if self._updater is not None:
            return self._updater
        from .updater import UpdateChecker
        up = UpdateChecker()
        up.found.connect(self._on_update_found)
        up.up_to_date.connect(self._on_up_to_date)
        up.check_failed.connect(self._on_check_failed)
        up.progress.connect(self._on_progress)
        up.downloaded.connect(self._on_downloaded)
        up.failed.connect(self._on_download_failed)
        self._updater = up
        return up

    def _check_updates(self) -> None:
        self._ensure_updater()
        self._update_btn.setEnabled(False)
        self._update_status.setText("Checking for updates\u2026")
        self._flash("Checking for updates\u2026")
        self._updater.check_now()

    def _on_up_to_date(self, ver: str) -> None:
        self._update_btn.setEnabled(True)
        self._update_status.setText(f"You're on the latest version (v{ver}).")
        QMessageBox.information(
            self, "No updates",
            f"You're already on the latest version (v{ver}).")

    def _on_check_failed(self, msg: str) -> None:
        self._update_btn.setEnabled(True)
        self._update_status.setText("Update check failed.")
        QMessageBox.warning(self, "Update check failed",
                            f"Couldn't check for updates.\n\n{msg}")

    def _on_update_found(self, info: dict) -> None:
        self._update_btn.setEnabled(True)
        ver = info.get("version", "?")
        self._update_status.setText(f"Version {ver} is available.")
        box = QMessageBox(self)
        box.setWindowTitle("Update available")
        box.setIcon(QMessageBox.Icon.Information)
        box.setText(f"GridGlance {ver} is available "
                    f"(you have v{version.__version__}).")
        box.setInformativeText(
            "Update and restart now? GridGlance will close, update itself "
            "and reopen automatically -- no setup steps.")
        notes = (info.get("notes") or "").strip()
        if notes:
            box.setDetailedText(notes)
        box.setStandardButtons(QMessageBox.StandardButton.Yes
                               | QMessageBox.StandardButton.No)
        box.setDefaultButton(QMessageBox.StandardButton.Yes)
        if box.exec() != QMessageBox.StandardButton.Yes:
            self._update_status.setText(f"Version {ver} is available.")
            return
        url = info.get("url")
        if not url:
            QMessageBox.warning(
                self, "No installer",
                "That release doesn't have a downloadable installer "
                "for this platform.")
            return
        self._begin_download(url, ver)

    def _begin_download(self, url: str, ver: str) -> None:
        self._dl_canceled = False
        dlg = QProgressDialog("Downloading update\u2026", "Cancel", 0, 100, self)
        dlg.setWindowTitle(f"Downloading v{ver}")
        dlg.setWindowModality(Qt.WindowModality.WindowModal)
        dlg.setAutoClose(False)
        dlg.setAutoReset(False)
        dlg.setMinimumDuration(0)
        dlg.setValue(0)
        dlg.canceled.connect(self._cancel_download)
        self._dl_dialog = dlg
        self._update_btn.setEnabled(False)
        self._update_status.setText("Downloading update\u2026")
        dlg.show()
        self._updater.download_async(url)

    def _cancel_download(self) -> None:
        # The worker thread can't be force-killed; flag it so we ignore the
        # eventual result, and close the dialog.
        self._dl_canceled = True
        if self._dl_dialog is not None:
            self._dl_dialog.close()
            self._dl_dialog = None
        self._update_btn.setEnabled(True)
        self._update_status.setText("Update download canceled.")

    def _on_progress(self, done: int, total: int) -> None:
        if self._dl_dialog is None:
            return
        if total > 0:
            self._dl_dialog.setMaximum(100)
            self._dl_dialog.setValue(int(done * 100 / total))
            mb = done / 1_048_576
            tot = total / 1_048_576
            self._dl_dialog.setLabelText(
                f"Downloading update\u2026  {mb:.1f} / {tot:.1f} MB")
        else:
            self._dl_dialog.setMaximum(0)  # indeterminate "busy" bar

    def _on_downloaded(self, path: str) -> None:
        if self._dl_canceled:
            return
        if self._dl_dialog is not None:
            self._dl_dialog.close()
            self._dl_dialog = None
        self._update_status.setText("Updating \u2014 GridGlance will reopen\u2026")
        self._launch_installer(path)

    def _on_download_failed(self, msg: str) -> None:
        if self._dl_dialog is not None:
            self._dl_dialog.close()
            self._dl_dialog = None
        self._update_btn.setEnabled(True)
        self._update_status.setText("Download failed.")
        QMessageBox.warning(self, "Download failed",
                            f"Couldn't download the update.\n\n{msg}")

    def _launch_installer(self, path: str) -> None:
        import os
        import subprocess
        try:
            if os.name == "nt":
                # /VERYSILENT: no setup wizard. The installer closes the running
                # app, replaces its files and relaunches it automatically, so the
                # update looks like the app just restarting on the new version.
                subprocess.Popen([path, "/VERYSILENT", "/SUPPRESSMSGBOXES",
                                  "/NORESTART"])
            else:
                subprocess.Popen([path])
        except Exception as exc:  # noqa: BLE001
            QMessageBox.warning(
                self, "Couldn't start installer",
                f"The update was downloaded to:\n{path}\n\n"
                f"but couldn't be launched automatically:\n{exc}")
            return
        # Stop the overlay (if we control it) and quit so files can be replaced.
        if self._overlay is not None and hasattr(self._overlay, "stop_overlay"):
            try:
                self._overlay.stop_overlay()
            except Exception:
                pass
        QApplication.instance().quit()

    # --- UI construction ----------------------------------------------------

    def _build_nav_and_pages(self) -> None:
        while self.stack.count():
            w = self.stack.widget(0)
            self.stack.removeWidget(w)
            w.deleteLater()
        while self.nav_lay.count():
            item = self.nav_lay.takeAt(0)
            if item.widget():
                item.widget().deleteLater()
        self._rows.clear()
        self._accordions.clear()
        self._nav_items.clear()

        # General + App first (Settings tab), then -- only for users with write
        # access -- the Track Scan authoring tab, then the rest alphabetically by
        # title so the widget vertical tabs read A-Z (Dash, Fuel Calc, ...).
        head = [("__general__", "General"), ("__app__", "App")]
        if track_store.can_write():
            head.append(("__scan__", "Track Scan"))
        self._sections = head + sorted(
            ((k, _pretty(k)) for k, val in config.DEFAULTS.items()
             if isinstance(val, dict)),
            key=lambda kt: kt[1].lower())
        for idx, (key, title) in enumerate(self._sections):
            color = TAB_COLORS.get(title, "#9aa3b2")
            self.stack.addWidget(self._scroll(self._build_page(key, title, color)))
            nav = NavItem(title, color)
            nav.clicked.connect(lambda i=idx: self._select(i))
            if key not in ("__general__", "__app__") and "show" in config.DEFAULTS.get(key, {}):
                nav.set_dot(bool(_get_at(self.working, [key, "show"])))
            self._nav_items[key] = nav
            self.nav_lay.addWidget(nav)
        self.nav_lay.addStretch(1)
        self._cur_index = min(self._cur_index, len(self._sections) - 1)
        self._apply_top_tab()

    def _group_keys(self, tab: str) -> set[str]:
        """The section keys that belong to the given top tab."""
        allk = {k for k, _t in self._sections}
        settings = allk & SETTINGS_SECTION_KEYS
        return settings if tab == "settings" else (allk - settings)

    def _set_top_tab(self, name: str) -> None:
        self._top_tab = name
        self._apply_top_tab()

    def _apply_top_tab(self) -> None:
        """Show only the nav items for the active top tab, keeping a valid page."""
        for name, btn in self._top_tabs.items():
            btn.setChecked(name == self._top_tab)
        keys = self._group_keys(self._top_tab)
        first_idx = None
        for idx, (key, _t) in enumerate(self._sections):
            nav = self._nav_items.get(key)
            visible = key in keys
            if nav:
                nav.setVisible(visible)
            if visible and first_idx is None:
                first_idx = idx
        cur_key = (self._sections[self._cur_index][0]
                   if 0 <= self._cur_index < len(self._sections) else None)
        if cur_key in keys:
            self._select(self._cur_index)
        elif first_idx is not None:
            self._select(first_idx)

    def _select(self, index: int) -> None:
        self._cur_index = index
        self.stack.setCurrentIndex(index)
        for idx, (key, _t) in enumerate(self._sections):
            nav = self._nav_items.get(key)
            if nav:
                nav.setSelected(idx == index)
        self._sync_corner_edit_mode()
        if (0 <= index < len(self._sections)
                and self._sections[index][0] == "__scan__"):
            self._refresh_track_authoring()

    def _scroll(self, inner: QWidget) -> QScrollArea:
        area = QScrollArea()
        area.setWidgetResizable(True)
        area.setWidget(inner)
        # Scope the transparent background to the scroll area + its viewport via
        # explicit selectors. A bare "background: transparent;" cascades to every
        # child widget (e.g. it wiped the #primary button's blue fill).
        area.viewport().setObjectName("scrollViewport")
        area.setStyleSheet(
            "QScrollArea { background: transparent; border: none; }"
            " QWidget#scrollViewport { background: transparent; }")
        return area

    def _build_page(self, key: str, title: str, color: str) -> QWidget:
        page = QWidget()
        v = QVBoxLayout(page)
        v.setContentsMargins(6, 4, 10, 8)
        v.setSpacing(9)

        head_row = QHBoxLayout()
        head_row.setContentsMargins(0, 0, 0, 0)
        head = QLabel(title)
        head.setObjectName("pageTitle")
        head_row.addWidget(head)
        head_row.addStretch(1)
        if key not in ("__general__", "__app__", "__scan__"):
            reset_btn = QPushButton("Reset to defaults")
            reset_btn.setObjectName("sectionReset")
            reset_btn.setCursor(Qt.CursorShape.PointingHandCursor)
            reset_btn.setStyleSheet(_section_reset_qss(color))
            reset_btn.clicked.connect(
                lambda _=False, k=key, t=title: self._reset_section(k, t))
            head_row.addWidget(reset_btn)
        v.addLayout(head_row)

        if key == "__app__":
            # Preset-independent (global) settings: updates + preset auto-switch.
            note = QLabel("These are global settings \u2014 switching presets "
                          "won't change them.")
            note.setObjectName("subtitle")
            note.setWordWrap(True)
            v.addWidget(note)
            v.addWidget(self._about_card())
            v.addWidget(self._auto_switch_card())
            v.addWidget(self._cloud_tracks_card())
            v.addStretch(1)
            return page

        if key == "__general__":
            v.addWidget(self._preset_leagues_card())
            v.addWidget(self._preset_cars_card())
            scalars = {k: val for k, val in config.DEFAULTS.items()
                       if not isinstance(val, dict)}
            self._populate(v, scalars, [], color, [])
            v.addStretch(1)
            return page

        if key == "__scan__":
            note = QLabel("Authoring tools \u2014 you see these because you have "
                          "write access. Import a members HTML track map, draw "
                          "pit road and merge on the overlay, then save. Corner "
                          "labels and start/finish can be edited on the map.")
            note.setObjectName("subtitle")
            note.setWordWrap(True)
            v.addWidget(note)
            from .widgets.track_import_panel import TrackImportV2Panel
            self._v2_import_panel = TrackImportV2Panel(self._overlay)
            self._v2_import_panel.saved.connect(self._flash)
            self._v2_import_panel.notified.connect(self._flash)
            v.addWidget(self._v2_import_panel)
            v.addWidget(self._track_authoring_card())
            v.addStretch(1)
            return page

        schema = config.DEFAULTS[key]
        target = v
        skip: set = set()
        if "show" in schema:
            card, body = self._enable_card(key, title, color)
            v.addWidget(card)
            v.addWidget(body)
            target = body.layout()
            skip = {"show"}

        self._populate(target, schema, [key], color, [], skip=skip)
        v.addStretch(1)
        return page

    def _enable_card(self, key: str, title: str, color: str):
        """A prominent master on/off switch; its body holds the rest of the
        widget's settings and collapses when the widget is disabled."""
        card = QFrame()
        card.setObjectName("enableCard")
        h = QHBoxLayout(card)
        h.setContentsMargins(15, 11, 15, 11)
        h.setSpacing(12)
        texts = QVBoxLayout()
        texts.setSpacing(1)
        t = QLabel(f"Enable {title}")
        t.setObjectName("enableTitle")
        hint = QLabel("Show this widget and reveal its settings below.")
        hint.setObjectName("enableHint")
        hint.setWordWrap(True)
        texts.addWidget(t)
        texts.addWidget(hint)
        h.addLayout(texts, 1)
        cur = bool(_get_at(self.working, [key, "show"]))
        toggle = ToggleSwitch(checked=cur, accent=color)
        h.addWidget(toggle, 0, Qt.AlignmentFlag.AlignVCenter)

        body = QWidget()
        bl = QVBoxLayout(body)
        bl.setContentsMargins(0, 2, 0, 0)
        bl.setSpacing(8)
        body.setVisible(cur)

        def on_toggle(on, k=key, b=body):
            self._set([k, "show"], bool(on))
            b.setVisible(bool(on))
            nav = self._nav_items.get(k)
            if nav:
                nav.set_dot(bool(on))

        toggle.toggled.connect(on_toggle)
        return card, body

    def _track_authoring_card(self) -> QFrame:
        """Track Scan tab: edit pit speed, corner count, and label positions."""
        card = QFrame()
        card.setObjectName("enableCard")
        v = QVBoxLayout(card)
        v.setContentsMargins(15, 11, 15, 12)
        v.setSpacing(8)
        t = QLabel("Track metadata")
        t.setObjectName("enableTitle")
        hint = QLabel("Corner count, pit speed, and label positions. Drag corner "
                      "numbers on the map when edit is enabled; release to save.")
        hint.setObjectName("enableHint")
        hint.setWordWrap(True)
        v.addWidget(t)
        v.addWidget(hint)

        unit = config.speed_unit()
        pit_row = QHBoxLayout()
        pit_lbl = QLabel(f"Pit speed limit ({unit})")
        pit_lbl.setObjectName("rowLabel")
        self._pit_speed_spin = QDoubleSpinBox()
        self._pit_speed_spin.setRange(0.0, 120.0 if unit == "MPH" else 200.0)
        self._pit_speed_spin.setDecimals(1)
        self._pit_speed_spin.setSingleStep(1.0)
        self._pit_speed_spin.valueChanged.connect(self._pit_speed_authoring_changed)
        pit_row.addWidget(pit_lbl)
        pit_row.addStretch(1)
        pit_row.addWidget(self._pit_speed_spin)
        v.addLayout(pit_row)

        pit_lane_row = QHBoxLayout()
        pit_lane_lbl = QLabel("Pit lane speed (%)")
        pit_lane_lbl.setObjectName("rowLabel")
        self._pit_lane_speed_spin = QDoubleSpinBox()
        self._pit_lane_speed_spin.setRange(25.0, 300.0)
        self._pit_lane_speed_spin.setDecimals(0)
        self._pit_lane_speed_spin.setSingleStep(5.0)
        self._pit_lane_speed_spin.setSuffix(" %")
        self._pit_lane_speed_spin.valueChanged.connect(
            self._pit_lane_speed_authoring_changed)
        pit_lane_row.addWidget(pit_lane_lbl)
        pit_lane_row.addStretch(1)
        pit_lane_row.addWidget(self._pit_lane_speed_spin)
        v.addLayout(pit_lane_row)

        turn_row = QHBoxLayout()
        turn_lbl = QLabel("Number of corners")
        turn_lbl.setObjectName("rowLabel")
        self._num_turns_spin = QSpinBox()
        self._num_turns_spin.setRange(0, 30)
        self._num_turns_spin.setSpecialValueText("Auto")
        self._num_turns_spin.valueChanged.connect(self._num_turns_authoring_changed)
        turn_row.addWidget(turn_lbl)
        turn_row.addStretch(1)
        turn_row.addWidget(self._num_turns_spin)
        v.addLayout(turn_row)

        edit_row = QHBoxLayout()
        edit_texts = QVBoxLayout()
        edit_texts.setSpacing(1)
        edit_title = QLabel("Edit corner labels on map")
        edit_title.setObjectName("rowLabel")
        edit_hint = QLabel("Drag corner numbers on the track map to reposition "
                           "them; release to save.")
        edit_hint.setObjectName("enableHint")
        edit_hint.setWordWrap(True)
        edit_texts.addWidget(edit_title)
        edit_texts.addWidget(edit_hint)
        edit_row.addLayout(edit_texts, 1)
        self._corner_edit_sw = ToggleSwitch(accent="#b84626")
        self._corner_edit_sw.toggled.connect(self._corner_edit_toggled)
        edit_row.addWidget(self._corner_edit_sw, 0, Qt.AlignmentFlag.AlignVCenter)
        v.addLayout(edit_row)

        sf_row = QHBoxLayout()
        sf_texts = QVBoxLayout()
        sf_texts.setSpacing(1)
        sf_title = QLabel("Edit start/finish on map")
        sf_title.setObjectName("rowLabel")
        sf_hint = QLabel("Drag the white start/finish line along the track; "
                          "release to save.")
        sf_hint.setObjectName("enableHint")
        sf_hint.setWordWrap(True)
        sf_texts.addWidget(sf_title)
        sf_texts.addWidget(sf_hint)
        sf_row.addLayout(sf_texts, 1)
        self._sf_edit_sw = ToggleSwitch(accent="#b84626")
        self._sf_edit_sw.toggled.connect(self._sf_edit_toggled)
        sf_row.addWidget(self._sf_edit_sw, 0, Qt.AlignmentFlag.AlignVCenter)
        v.addLayout(sf_row)

        self._authoring_status = QLabel("")
        self._authoring_status.setObjectName("enableHint")
        self._authoring_status.setWordWrap(True)
        v.addWidget(self._authoring_status)
        self._refresh_track_authoring()
        return card

    def _refresh_track_authoring(self) -> None:
        """Sync Track Scan metadata controls from the running overlay."""
        if not hasattr(self, "_pit_speed_spin"):
            return
        enabled = False
        sf_enabled = False
        state = {}
        if self._overlay is not None and hasattr(self._overlay, "track_authoring_state"):
            state = self._overlay.track_authoring_state()
            enabled = bool(state.get("has_track"))
            sf_enabled = bool(state.get("can_author_map"))
        self._pit_speed_spin.blockSignals(True)
        self._pit_lane_speed_spin.blockSignals(True)
        self._num_turns_spin.blockSignals(True)
        self._pit_speed_spin.setEnabled(enabled)
        self._pit_lane_speed_spin.setEnabled(enabled)
        self._num_turns_spin.setEnabled(enabled)
        self._corner_edit_sw.setEnabled(enabled)
        self._sf_edit_sw.setEnabled(sf_enabled)
        if enabled:
            ms = float(state.get("pit_speed_ms") or 0.0)
            self._pit_speed_spin.setValue(config.conv_speed(ms) if ms else 0.0)
            lane_pct = float(state.get("pit_lane_speed_pct") or 1.0)
            self._pit_lane_speed_spin.setValue(lane_pct * 100.0)
            n = state.get("num_turns")
            self._num_turns_spin.setValue(int(n) if n else 0)
            cnt = state.get("corner_count", 0)
            tid = state.get("authoring_track_id")
            tid_txt = f" (TrackID {tid})" if tid is not None else ""
            self._authoring_status.setText(
                f"{cnt} corner labels on map{tid_txt}."
                if cnt else f"No corner labels yet{tid_txt}.")
        else:
            self._pit_speed_spin.setValue(0.0)
            self._pit_lane_speed_spin.setValue(100.0)
            self._num_turns_spin.setValue(0)
            if state.get("demo"):
                self._authoring_status.setText(
                    "Track metadata editing needs a live iRacing session "
                    "(demo mode has no TrackID).")
            else:
                self._authoring_status.setText(
                    "Join a session and load a track in the overlay to edit "
                    "metadata.")
            self._corner_edit_sw.setChecked(False)
            self._sf_edit_sw.setChecked(False)
        self._pit_speed_spin.blockSignals(False)
        self._pit_lane_speed_spin.blockSignals(False)
        self._num_turns_spin.blockSignals(False)
        self._sync_corner_edit_mode()
        self._sync_sf_edit_mode()
    def _pit_speed_authoring_changed(self, value: float) -> None:
        if self._overlay is None or not hasattr(self._overlay, "set_pit_speed_authoring"):
            return
        ms = value / 2.2369362921 if config.is_imperial() else value / 3.6
        if self._overlay.set_pit_speed_authoring(ms):
            self._authoring_status.setText(
                f"Pit speed saved ({value:.1f} {config.speed_unit()}).")
            self._flash("Pit speed saved")
        else:
            self._authoring_status.setText(
                "Could not save pit speed (no local track file).")
            self._flash("Save failed")

    def _pit_lane_speed_authoring_changed(self, value: float) -> None:
        if self._overlay is None or not hasattr(
                self._overlay, "set_pit_lane_speed_authoring"):
            return
        if self._overlay.set_pit_lane_speed_authoring(value / 100.0):
            self._authoring_status.setText(
                f"Pit lane speed saved ({value:.0f}%).")
            self._flash("Pit lane speed saved")
        else:
            self._authoring_status.setText(
                "Could not save pit lane speed (no local track file).")
            self._flash("Save failed")

    def _num_turns_authoring_changed(self, value: int) -> None:
        if self._overlay is None or not hasattr(self._overlay, "set_num_turns_authoring"):
            return
        if self._overlay.set_num_turns_authoring(value):
            self._refresh_track_authoring()
            label = str(value) if value else "auto"
            self._authoring_status.setText(f"Corners updated ({label}) and saved.")
            self._flash("Corner count saved")
        else:
            self._authoring_status.setText(
                "Could not save corner count (no local track file).")
            self._flash("Save failed")

    def _corner_edit_toggled(self, on: bool) -> None:
        if on and self._overlay is not None and hasattr(
                self._overlay, "set_pit_edit_mode"):
            self._overlay.set_pit_edit_mode(False)
            if hasattr(self, "_v2_import_panel"):
                self._v2_import_panel._pit_edit_sw.blockSignals(True)
                self._v2_import_panel._pit_edit_sw.setChecked(False)
                self._v2_import_panel._pit_edit_sw.blockSignals(False)
        if on and hasattr(self, "_sf_edit_sw"):
            self._sf_edit_sw.blockSignals(True)
            self._sf_edit_sw.setChecked(False)
            self._sf_edit_sw.blockSignals(False)
        self._sync_corner_edit_mode()
        if on:
            self._authoring_status.setText(
                "Corner edit on \u2014 drag labels on the map.")

    def _sf_edit_toggled(self, on: bool) -> None:
        if on and self._overlay is not None and hasattr(
                self._overlay, "set_pit_edit_mode"):
            self._overlay.set_pit_edit_mode(False)
            if hasattr(self, "_v2_import_panel"):
                self._v2_import_panel._pit_edit_sw.blockSignals(True)
                self._v2_import_panel._pit_edit_sw.setChecked(False)
                self._v2_import_panel._pit_edit_sw.blockSignals(False)
        if on and hasattr(self, "_corner_edit_sw"):
            self._corner_edit_sw.blockSignals(True)
            self._corner_edit_sw.setChecked(False)
            self._corner_edit_sw.blockSignals(False)
        self._sync_sf_edit_mode()
        if on:
            self._authoring_status.setText(
                "Start/finish edit on \u2014 drag the white line on the map.")

    def _sync_corner_edit_mode(self) -> None:
        if self._overlay is None or not hasattr(self._overlay, "set_corner_edit_mode"):
            return
        on = False
        if hasattr(self, "_corner_edit_sw"):
            cur_key = (self._sections[self._cur_index][0]
                       if 0 <= self._cur_index < len(self._sections) else None)
            on = (cur_key == "__scan__" and self._corner_edit_sw.isChecked()
                  and self._corner_edit_sw.isEnabled())
        self._overlay.set_corner_edit_mode(on)

    def _sync_sf_edit_mode(self) -> None:
        if self._overlay is None or not hasattr(self._overlay, "set_sf_edit_mode"):
            return
        on = False
        if hasattr(self, "_sf_edit_sw"):
            cur_key = (self._sections[self._cur_index][0]
                       if 0 <= self._cur_index < len(self._sections) else None)
            on = (cur_key == "__scan__" and self._sf_edit_sw.isChecked()
                  and self._sf_edit_sw.isEnabled())
        self._overlay.set_sf_edit_mode(on)

    # Nested groups that are usually long/secondary start collapsed.
    _COLLAPSED = {"colors", "license_colors", "widths", "sizes", "columns"}

    def _populate(self, lay, schema: dict, path: list, color: str,
                  chain: list, skip=()) -> None:
        # Simple leaves first, then palette / order editors, then sub-groups.
        for key, default_val in schema.items():
            if key in skip or isinstance(default_val, dict):
                continue
            if isinstance(default_val, list) and key in ("palette", "column_order"):
                continue
            cur = path + [key]
            lay.addWidget(self._leaf_row(cur, default_val,
                                         _get_at(self.working, cur), color, chain))

        for key, default_val in schema.items():
            if key in skip:
                continue
            cur = path + [key]
            if key == "palette" and isinstance(default_val, list):
                acc = self._accordion("Palette", cur, color, expanded=False)
                acc.body_layout().addWidget(
                    PaletteEditor(_get_at(self.working, cur),
                                  lambda x, p=cur: self._set(p, x)))
                lay.addWidget(acc)
            elif key == "column_order" and isinstance(default_val, list):
                acc = self._accordion("Column order", cur, color, expanded=True)
                acc.body_layout().addWidget(
                    OrderEditor(_get_at(self.working, cur), COLUMN_LABELS,
                                config.TABLE_COLUMNS,
                                lambda x, p=cur: self._set(p, x)))
                lay.addWidget(acc)
            elif isinstance(default_val, dict):
                expanded = key not in self._COLLAPSED
                acc = self._accordion(_pretty(key), cur, color, expanded=expanded)
                self._populate(acc.body_layout(), default_val, cur, color,
                               chain + [acc])
                lay.addWidget(acc)

    def _accordion(self, title: str, path: list, color: str,
                   expanded: bool = True) -> CollapsibleSection:
        acc = CollapsibleSection(title, accent=color, expanded=expanded)
        acc._search = " ".join(_pretty(p) for p in path).lower()  # type: ignore
        self._accordions.append(acc)
        return acc

    def _leaf_row(self, path, default_val, value, color, chain) -> QWidget:
        row = QWidget()
        h = QHBoxLayout(row)
        h.setContentsMargins(2, 2, 2, 2)
        h.setSpacing(12)
        label_text = _label_for(path)
        label = QLabel(label_text)
        label.setObjectName("rowLabel")
        label.setMinimumWidth(170)
        label.setWordWrap(True)
        h.addWidget(label, 0)

        ctrl = self._control(path, default_val, value, color)
        if isinstance(ctrl, NumberControl):
            h.addWidget(ctrl, 1)
        else:
            h.addStretch(1)
            h.addWidget(ctrl, 0)

        self._rows.append({
            "widget": row,
            # Searchable on both the friendly label and the raw key path.
            "text": (label_text + " " + " ".join(_pretty(p) for p in path)).lower(),
            "accordions": list(chain),
            "dep": ROW_DEPENDENCIES.get(".".join(str(p) for p in path)),
        })
        return row

    def _dep_ok(self, r) -> bool:
        """Whether a row's controlling toggle (if any) currently allows it."""
        dep = r.get("dep")
        if not dep:
            return True
        ctrl, want = dep
        try:
            return _get_at(self.working, ctrl.split(".")) == want
        except (KeyError, TypeError):
            return True

    def _control(self, path: list, default_val, value, color: str) -> QWidget:
        options = _enum_options(path)
        if options:
            # Keep an out-of-list current value (e.g. a custom font_family from a
            # hand-edited config) so the dropdown never silently discards it.
            options = list(options)
            if value not in options:
                options.append(value)
            is_font = bool(path) and path[-1] == "font_family"
            label_fn = (lambda v: str(v)) if is_font else _option_label
            options = _sort_combo_options(options, label_fn)
            combo = Combo()
            for opt in options:
                # Font names display verbatim (not run through _pretty, which
                # would mangle e.g. "Segoe UI" -> "Segoe ui").
                combo.addItem(opt if is_font else _option_label(opt), opt)
                if is_font and isinstance(opt, str):
                    combo.setItemData(combo.count() - 1, QFont(opt, 11),
                                      Qt.ItemDataRole.FontRole)
            combo.setCurrentIndex(options.index(value) if value in options else 0)
            combo.currentIndexChanged.connect(
                lambda _i, p=path, c=combo: self._set(p, c.currentData()))
            return combo
        if _is_color(path, default_val):
            return ColorButton(value, lambda v, p=path: self._set(p, v))
        # The General/Table tabs use a muted gray that's nearly invisible as an
        # "on" color, so their sliders/toggles fall back to the green accent.
        accent = ACCENT if (not color or color.lower() in ("#9aa3b2", "#7f8c9a")) else color
        if isinstance(default_val, bool):
            sw = ToggleSwitch(checked=bool(value), accent=accent)
            sw.toggled.connect(lambda v, p=path: self._set(p, bool(v)))
            return sw
        if isinstance(default_val, (int, float)):
            return NumberControl(path, default_val, value,
                                 lambda v, p=path: self._set(p, v), accent=accent)
        edit = QLineEdit(str(value))
        edit.setMinimumWidth(180)
        edit.textChanged.connect(lambda v, p=path: self._set(p, v))
        return edit

    # --- search filtering ---------------------------------------------------

    def _filter(self, text: str) -> None:
        t = text.lower().strip()
        for r in self._rows:
            ok = self._dep_ok(r) and ((t in r["text"]) if t else True)
            r["widget"].setVisible(ok)
        for acc in self._accordions:
            rows = [r for r in self._rows if acc in r["accordions"]]
            has = (any(r["widget"].isVisible() for r in rows)
                   or (bool(t) and t in getattr(acc, "_search", "")))
            if t:
                acc.setVisible(has)
                if has:
                    acc.setExpanded(True)
            else:
                acc.setVisible(True)
                acc.setExpanded(acc._default_expanded)

    # --- value changes ------------------------------------------------------

    def _toggle_overlay(self) -> None:
        if self._overlay is None:
            return
        running = self._overlay.toggle_overlay()
        self._refresh_overlay_btn()
        self._flash("Overlay started" if running else "Overlay stopped")

    def _refresh_overlay_btn(self) -> None:
        if self._overlay is None:
            return
        running = self._overlay.overlay_running()
        self.overlay_btn.setText("Stop Overlay" if running else "Start Overlay")
        self.overlay_btn.setObjectName("stop" if running else "go")
        # Re-polish so the objectName-based style (#go / #stop) takes effect.
        self.overlay_btn.style().unpolish(self.overlay_btn)
        self.overlay_btn.style().polish(self.overlay_btn)

    def _sync_edit_switch(self) -> None:
        """Reflect the overlay's current edit/lock state without re-firing it."""
        if self._overlay is None or not hasattr(self._overlay, "edit_mode_enabled"):
            return
        self.edit_sw.blockSignals(True)
        self.edit_sw.setChecked(self._overlay.edit_mode_enabled())
        self.edit_sw.blockSignals(False)

    def _toggle_edit(self, on: bool) -> None:
        if self._overlay is None or not hasattr(self._overlay, "set_edit_mode"):
            return
        self._overlay.set_edit_mode(bool(on))
        self._flash("Edit layout on \u2014 drag widgets to move/resize"
                    if on else "Layout locked")

    def showEvent(self, event):  # noqa: N802
        super().showEvent(event)
        # Preview the profile we're editing so live changes are visible.
        config.set_preview_context(self._edit_ctx)
        # Re-sync state in case it changed from the tray while we were hidden.
        if self._overlay is not None:
            self._refresh_overlay_btn()
            self._sync_edit_switch()
            self._refresh_track_authoring()
            if hasattr(self, "_v2_import_panel"):
                self._v2_import_panel.set_overlay(self._overlay)
                self._v2_import_panel.refresh()

    def _flash(self, msg: str) -> None:
        self.status.setText(msg)
        self._status_timer.start(2500)

    # --- presets -------------------------------------------------------------

    def _build_preset_bar(self):
        """Top row to pick/manage presets (independent config + layout sets)."""
        bar = QHBoxLayout()
        bar.setSpacing(8)
        lbl = QLabel("Preset")
        lbl.setStyleSheet("background: transparent; color: #aab2bf; "
                          "font-weight: 700;")
        self.preset_combo = Combo()
        self.preset_combo.setObjectName("ctxCombo")
        self._refresh_preset_combo()
        self.preset_combo.currentIndexChanged.connect(self._change_preset)
        bar.addWidget(lbl)
        bar.addWidget(self.preset_combo)
        for text, slot, obj in (
            ("New", self._new_preset, ""),
            ("Duplicate", self._dup_preset, ""),
            ("Rename", self._rename_preset, ""),
            ("Delete", self._delete_preset, "danger"),
        ):
            b = QPushButton(text)
            if obj:
                b.setObjectName(obj)
            b.setCursor(Qt.CursorShape.PointingHandCursor)
            b.clicked.connect(slot)
            bar.addWidget(b)
        bar.addStretch(1)
        self.default_w, self.default_sw = self._opt_toggle("Default preset", False)
        self.default_sw.toggled.connect(self._toggle_default)
        self.default_w.setToolTip(
            "Use this preset when no league or car preset matches. Exactly one "
            "preset is always the default, so you set it by choosing another.")
        bar.addWidget(self.default_w)
        self._update_default_toggle()
        return bar

    def _refresh_preset_combo(self) -> None:
        self.preset_combo.blockSignals(True)
        self.preset_combo.clear()
        for name in sorted(config.presets(), key=str.casefold):
            self.preset_combo.addItem(name, name)
        i = self.preset_combo.findData(config.active_preset())
        self.preset_combo.setCurrentIndex(max(0, i))
        self.preset_combo.blockSignals(False)

    def _after_preset_change(self, msg: str) -> None:
        """Rebuild the editor around the now-active preset and flash a status."""
        self._refresh_preset_combo()
        self.working = config.editor_full(self._edit_ctx)
        config.set_preview_context(self._edit_ctx)
        self._cur_index = 0
        self._build_nav_and_pages()
        self._filter(self.search.text())
        self._refresh_preset_cars()
        self._refresh_preset_leagues()
        self._update_default_toggle()
        self._flash(msg)

    def _change_preset(self) -> None:
        new = self.preset_combo.currentData()
        if not new or new == config.active_preset():
            return
        # Capture pending edits to the preset we're leaving, then switch.
        self._save_timer.stop()
        config.apply_edits(self._edit_ctx, self.working, notify=False)
        config.set_active_preset(new)
        self._after_preset_change(f"Switched to \u201c{new}\u201d")

    def _ask_name(self, title: str, label: str, default: str = "") -> str | None:
        name, ok = QInputDialog.getText(self, title, label, text=default)
        if not ok:
            return None
        name = name.strip()
        if not name:
            return None
        if name in config.presets():
            QMessageBox.warning(self, title,
                                f"A preset named \u201c{name}\u201d already exists.")
            return None
        return name

    def _new_preset(self) -> None:
        name = self._ask_name("New preset", "Preset name:")
        if not name:
            return
        config.apply_edits(self._edit_ctx, self.working, notify=False)
        config.create_preset(name, activate=True)
        self._after_preset_change(f"Created \u201c{name}\u201d")

    def _dup_preset(self) -> None:
        src = config.active_preset()
        name = self._ask_name("Duplicate preset", "New preset name:",
                              default=f"{src} copy")
        if not name:
            return
        config.apply_edits(self._edit_ctx, self.working, notify=False)
        config.save_profiles()
        config.duplicate_preset(src, name)
        self._after_preset_change(f"Duplicated to \u201c{name}\u201d")

    def _rename_preset(self) -> None:
        old = config.active_preset()
        name = self._ask_name("Rename preset", "New name:", default=old)
        if not name:
            return
        config.apply_edits(self._edit_ctx, self.working, notify=False)
        config.rename_preset(old, name)
        self._refresh_preset_combo()
        self._update_default_toggle()
        self._flash(f"Renamed to \u201c{name}\u201d")

    def _delete_preset(self) -> None:
        name = config.active_preset()
        if len(config.presets()) <= 1:
            QMessageBox.warning(self, "Delete preset",
                                "You can't delete your only preset.")
            return
        box = QMessageBox(self)
        box.setWindowTitle("Delete preset")
        box.setIcon(QMessageBox.Icon.Warning)
        box.setText(f"Delete the preset \u201c{name}\u201d?")
        box.setInformativeText("Its config and saved layout will be removed. "
                               "This can't be undone.")
        box.setStandardButtons(QMessageBox.StandardButton.Yes
                               | QMessageBox.StandardButton.No)
        box.setDefaultButton(QMessageBox.StandardButton.No)
        if box.exec() != QMessageBox.StandardButton.Yes:
            return
        config.delete_preset(name)
        self._after_preset_change(f"Deleted \u201c{name}\u201d")

    def _update_default_toggle(self) -> None:
        """Reflect/enforce the radio rule: the current default can't be unset."""
        sw = getattr(self, "default_sw", None)
        if sw is None:
            return
        is_default = config.default_preset() == config.active_preset()
        sw.set_checked_silent(is_default)
        # You set the default by choosing a *different* preset, so the current
        # default (and the only preset, which is forced default) is locked on.
        sw.setEnabled(not is_default)

    def _toggle_default(self, on: bool) -> None:
        if not on:
            # Radio behavior: can't clear the default directly; re-check it.
            self._update_default_toggle()
            return
        config.set_default_preset(config.active_preset())
        self._update_default_toggle()
        self._flash(f"\u201c{config.active_preset()}\u201d is now the default preset")

    def _auto_switch_card(self) -> QFrame:
        """General-page card with the three independent auto-switch toggles."""
        card = QFrame()
        card.setObjectName("enableCard")
        v = QVBoxLayout(card)
        v.setContentsMargins(15, 11, 15, 12)
        v.setSpacing(8)
        t = QLabel("Auto-switch presets")
        t.setObjectName("enableTitle")
        hint = QLabel("Activate a preset to match the session. When more than one "
                      "rule applies, league wins over car, and car wins over your "
                      "default.")
        hint.setObjectName("enableHint")
        hint.setWordWrap(True)
        v.addWidget(t)
        v.addWidget(hint)
        rules = (
            ("In a league with a bound preset",
             config.auto_switch_by_league, config.set_auto_switch_by_league),
            ("Driving a car with a bound preset",
             config.auto_switch_by_car, config.set_auto_switch_by_car),
            ("Otherwise, use my default preset",
             config.auto_switch_to_default, config.set_auto_switch_to_default),
        )
        for label, getter, setter in rules:
            w, sw = self._opt_toggle(label, getter())
            sw.toggled.connect(lambda on, s=setter: self._set_auto_switch(s, on))
            v.addWidget(w)
        return card

    def _set_auto_switch(self, setter, on: bool) -> None:
        setter(bool(on))
        self._flash("Auto-switch updated")

    def _cloud_tracks_card(self) -> QFrame:
        """General-page card: opt in/out of shared (cloud) track maps."""
        card = QFrame()
        card.setObjectName("enableCard")
        v = QVBoxLayout(card)
        v.setContentsMargins(15, 11, 15, 12)
        v.setSpacing(8)
        t = QLabel("Community track maps")
        t.setObjectName("enableTitle")
        hint = QLabel("Download shared track maps automatically the first time "
                      "you visit a track, instead of driving a lap to learn it. "
                      "Turn off to stay fully offline.")
        hint.setObjectName("enableHint")
        hint.setWordWrap(True)
        v.addWidget(t)
        v.addWidget(hint)
        w, sw = self._opt_toggle("Download shared track maps",
                                 config.cloud_tracks())
        sw.toggled.connect(self._toggle_cloud_tracks)
        v.addWidget(w)
        return card

    def _toggle_cloud_tracks(self, on: bool) -> None:
        config.set_cloud_tracks(bool(on))
        self._flash("Community track maps on" if on
                    else "Community track maps off")

    def _preset_cars_card(self) -> QFrame:
        """Card on the General page to bind cars that auto-load this preset."""
        card = QFrame()
        card.setObjectName("enableCard")
        v = QVBoxLayout(card)
        v.setContentsMargins(15, 11, 15, 12)
        v.setSpacing(8)
        t = QLabel("Auto-load this preset for cars")
        t.setObjectName("enableTitle")
        hint = QLabel("When you drive one of these cars, GridGlance switches to "
                      "this preset (needs the car auto-switch rule on).")
        hint.setObjectName("enableHint")
        hint.setWordWrap(True)
        v.addWidget(t)
        v.addWidget(hint)
        self._cars_list = QListWidget()
        self._cars_list.setObjectName("orderList")
        self._cars_list.setMaximumHeight(120)
        v.addWidget(self._cars_list)
        row = QHBoxLayout()
        row.setSpacing(8)
        add_btn = QPushButton("Add current car")
        add_btn.setCursor(Qt.CursorShape.PointingHandCursor)
        add_btn.clicked.connect(self._add_current_car)
        rem_btn = QPushButton("Remove selected")
        rem_btn.setObjectName("danger")
        rem_btn.setCursor(Qt.CursorShape.PointingHandCursor)
        rem_btn.clicked.connect(self._remove_selected_car)
        row.addWidget(add_btn)
        row.addWidget(rem_btn)
        row.addStretch(1)
        v.addLayout(row)
        self._refresh_preset_cars()
        return card

    def _refresh_preset_cars(self) -> None:
        lst = getattr(self, "_cars_list", None)
        if lst is None:
            return
        lst.clear()
        for path in config.preset_cars():
            item = QListWidgetItem(self._car_labels.get(path, path))
            item.setData(Qt.ItemDataRole.UserRole, path)
            lst.addItem(item)

    def _add_current_car(self) -> None:
        car = ("", "")
        if self._overlay is not None and hasattr(self._overlay, "current_car"):
            car = self._overlay.current_car()
        path, name = car
        if not path:
            QMessageBox.information(
                self, "Add car",
                "No car detected. Start the overlay and get in a car in "
                "iRacing first.")
            return
        self._car_labels[path] = name or path
        cars = config.preset_cars()
        if path not in cars:
            cars.append(path)
            config.set_preset_cars(config.active_preset(), cars)
        self._refresh_preset_cars()
        self._flash(f"Bound {name or path} to this preset")

    def _remove_selected_car(self) -> None:
        lst = getattr(self, "_cars_list", None)
        if lst is None or lst.currentItem() is None:
            return
        path = lst.currentItem().data(Qt.ItemDataRole.UserRole)
        cars = [c for c in config.preset_cars() if c != path]
        config.set_preset_cars(config.active_preset(), cars)
        self._refresh_preset_cars()

    def _preset_leagues_card(self) -> QFrame:
        """Card on the General page to bind leagues that auto-load this preset."""
        card = QFrame()
        card.setObjectName("enableCard")
        v = QVBoxLayout(card)
        v.setContentsMargins(15, 11, 15, 12)
        v.setSpacing(8)
        t = QLabel("Auto-load this preset for leagues")
        t.setObjectName("enableTitle")
        hint = QLabel("When you're in one of these league sessions, GridGlance "
                      "switches to this preset (needs the league auto-switch rule "
                      "on). League takes priority over car.")
        hint.setObjectName("enableHint")
        hint.setWordWrap(True)
        v.addWidget(t)
        v.addWidget(hint)
        self._leagues_list = QListWidget()
        self._leagues_list.setObjectName("orderList")
        self._leagues_list.setMaximumHeight(120)
        v.addWidget(self._leagues_list)
        row = QHBoxLayout()
        row.setSpacing(8)
        add_btn = QPushButton("Add current league")
        add_btn.setCursor(Qt.CursorShape.PointingHandCursor)
        add_btn.clicked.connect(self._add_current_league)
        rem_btn = QPushButton("Remove selected")
        rem_btn.setObjectName("danger")
        rem_btn.setCursor(Qt.CursorShape.PointingHandCursor)
        rem_btn.clicked.connect(self._remove_selected_league)
        row.addWidget(add_btn)
        row.addWidget(rem_btn)
        row.addStretch(1)
        v.addLayout(row)
        self._refresh_preset_leagues()
        return card

    def _refresh_preset_leagues(self) -> None:
        lst = getattr(self, "_leagues_list", None)
        if lst is None:
            return
        lst.clear()
        for lid in config.preset_leagues():
            item = QListWidgetItem(self._league_labels.get(lid, f"League {lid}"))
            item.setData(Qt.ItemDataRole.UserRole, lid)
            lst.addItem(item)

    def _add_current_league(self) -> None:
        league = (0, "")
        if self._overlay is not None and hasattr(self._overlay, "current_league"):
            league = self._overlay.current_league()
        lid, label = league
        if not lid:
            QMessageBox.information(
                self, "Add league",
                "No league session detected. Join the league session in "
                "iRacing first.")
            return
        self._league_labels[lid] = label or f"League {lid}"
        leagues = config.preset_leagues()
        if lid not in leagues:
            leagues.append(lid)
            config.set_preset_leagues(config.active_preset(), leagues)
        self._refresh_preset_leagues()
        self._flash(f"Bound {self._league_labels[lid]} to this preset")

    def _remove_selected_league(self) -> None:
        lst = getattr(self, "_leagues_list", None)
        if lst is None or lst.currentItem() is None:
            return
        lid = lst.currentItem().data(Qt.ItemDataRole.UserRole)
        leagues = [x for x in config.preset_leagues() if x != lid]
        config.set_preset_leagues(config.active_preset(), leagues)
        self._refresh_preset_leagues()

    # --- profile (context) switching ----------------------------------------

    def _ctx_name(self) -> str:
        return config.CONTEXT_LABELS.get(self._edit_ctx, self._edit_ctx)

    def _update_ctx_hint(self) -> None:
        if self._edit_ctx == "garage":
            self.ctx_hint.setText(
                "Editing the in-garage profile \u2014 only changes from the "
                "on-track settings are saved here, and apply when you're in "
                "the garage.")
        else:
            self.ctx_hint.setText(
                "Editing the on-track profile \u2014 your normal racing layout.")

    def _change_ctx(self) -> None:
        new = self.ctx_combo.currentData()
        if not new or new == self._edit_ctx:
            return
        # Capture any pending edits to the profile we're leaving, then persist.
        self._save_timer.stop()
        config.apply_edits(self._edit_ctx, self.working, notify=False)
        if self.autosave_sw.isChecked():
            config.save_profiles()
        self._edit_ctx = new
        self.working = config.editor_full(new)
        # Pin the live overlay to the profile being edited so changes preview.
        config.set_preview_context(new)
        self._cur_index = 0
        self._build_nav_and_pages()
        self._filter(self.search.text())
        self._update_ctx_hint()
        self._flash(f"Editing {self._ctx_name()} profile")

    # --- value changes ------------------------------------------------------

    def _set(self, path: list, value) -> None:
        _set_at(self.working, path, value)
        # Toggling a controller (e.g. map.show_pit) reveals/hides dependent rows.
        if ".".join(str(p) for p in path) in _DEP_CONTROLLERS:
            self._filter(self.search.text())
        if self.live_sw.isChecked():
            config.apply_edits(self._edit_ctx, self.working)
        if self.autosave_sw.isChecked():
            self._flash("Modified \u2014 saving\u2026")
            self._save_timer.start(400)
        else:
            self._flash("Modified \u2014 unsaved")

    def _autosave(self) -> None:
        config.apply_edits(self._edit_ctx, self.working,
                           notify=self.live_sw.isChecked())
        config.save_profiles()
        self._flash("Saved to overlay_config.json")

    def _apply(self) -> None:
        config.apply_edits(self._edit_ctx, self.working)
        config.set_preview_context(self._edit_ctx)
        self._flash(f"Applied to {self._ctx_name()} profile")

    def _save(self) -> None:
        self._save_timer.stop()
        config.apply_edits(self._edit_ctx, self.working)
        config.save_profiles()
        self._flash("Saved to overlay_config.json")

    def _reset(self) -> None:
        if self._edit_ctx == "garage":
            config.clear_garage(notify=self.live_sw.isChecked())
            self.working = config.editor_full("garage")
            msg = "Cleared garage overrides"
        else:
            self.working = config.full_defaults()
            if self.live_sw.isChecked():
                config.apply_base(self.working)
            msg = "Reset on-track profile to defaults"
        self._build_nav_and_pages()
        self._filter(self.search.text())
        if self.autosave_sw.isChecked():
            self._save_timer.start(400)
        self._flash(msg)

    def _reset_section(self, key: str, title: str) -> None:
        """Restore a single widget's settings to their built-in defaults."""
        self.working[key] = copy.deepcopy(config.DEFAULTS[key])
        if self.live_sw.isChecked():
            config.apply_edits(self._edit_ctx, self.working)
        if self.autosave_sw.isChecked():
            self._save_timer.start(400)
            self._flash(f"Reset {title} \u2014 saving\u2026")
        else:
            self._flash(f"Reset {title} to defaults \u2014 unsaved")
        # Rebuild so every control (and the nav dot) reflects the defaults.
        self._build_nav_and_pages()
        self._filter(self.search.text())

    def _reload(self) -> None:
        self._save_timer.stop()
        config.reload()
        self.working = config.editor_full(self._edit_ctx)
        self._build_nav_and_pages()
        self._filter(self.search.text())
        config.set_preview_context(self._edit_ctx)
        self._flash("Reloaded from file")

    # --- preview lifecycle --------------------------------------------------

    def closeEvent(self, event):  # noqa: N802
        if self._overlay is not None and hasattr(self._overlay, "set_corner_edit_mode"):
            self._overlay.set_corner_edit_mode(False)
        # Stop pinning the live overlay to the edited profile; resume the
        # telemetry-driven context.
        config.set_preview_context(None)
        super().closeEvent(event)


def main() -> int:
    app = QApplication(sys.argv)
    editor = ConfigEditor()
    editor.show()
    return app.exec()


if __name__ == "__main__":
    sys.exit(main())
