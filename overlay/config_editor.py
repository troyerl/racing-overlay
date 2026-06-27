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

import sys

from PyQt6.QtCore import Qt, QTimer, QRectF
from PyQt6.QtGui import QBrush, QColor, QLinearGradient, QPainter, QPen, QPixmap
from PyQt6.QtWidgets import (
    QAbstractItemView,
    QApplication,
    QCheckBox,
    QColorDialog,
    QComboBox,
    QDoubleSpinBox,
    QGroupBox,
    QHBoxLayout,
    QLabel,
    QLineEdit,
    QListWidget,
    QListWidgetItem,
    QPushButton,
    QScrollArea,
    QSpinBox,
    QTabBar,
    QTabWidget,
    QVBoxLayout,
    QWidget,
)

from . import config

COLOR_PARENTS = {"colors", "license_colors"}

# Simple key-name -> options dropdowns.
ENUMS = {
    "pit_mode": ["laps_since", "time_since", "at_lap", "at_time"],
    "units": ["metric", "imperial"],
    "center_mode": ["ring", "pedals"],
    "car_label": ["number", "position"],
}

# Friendly display text for raw config option values (combo boxes show these,
# but the underlying stored value is unchanged).
OPTION_LABELS = {
    "none": "None",
    # dash metrics
    "speed": "Speed", "speed_kph": "Speed (km/h)", "speed_mph": "Speed (mph)",
    "rpm": "RPM", "gear": "Gear", "position": "Position", "lap": "Lap",
    "lap_count": "Lap (x/total)", "fuel": "Fuel", "fuel_stack": "Fuel (+laps)",
    "fuel_laps": "Fuel laps left", "tires": "Tire wear (L/R)",
    "laps_left": "Laps remaining",
    "last_lap": "Last lap", "best_lap": "Best lap",
    "cur_lap": "Current lap", "delta": "Delta", "incidents": "Incidents",
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
}

# Special-cased word fixups so labels read naturally (RPM, iRating, ...).
_WORD_FIXUPS = {
    "rpm": "RPM", "sof": "SoF", "irating": "iRating", "sr": "SR", "ui": "UI",
    "id": "ID", "bg": "background", "frac": "fraction", "px": "size",
    "tau": "easing", "pct": "percent",
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

# Per-tab accent colors so each section reads like its widget's theme.
TAB_COLORS = {
    "General": "#9aa3b2",
    "Table": ACCENT,
    "Relative": ACCENT,
    "Standings": ACCENT,
    "Radar": ACCENT,
    "Dash": ACCENT,
    "Map": "#62b5ff",
    "Light Hud": "#9aa3b2",
}

STYLE = f"""
QWidget {{ color: #d7dae0; font-family: 'Segoe UI', 'SF Pro Text', Arial; font-size: 12px; }}
QLabel#title {{ font-size: 20px; font-weight: 700; color: #f4f6f8; }}
QLabel#subtitle {{ color: #8b93a1; font-size: 11px; }}
QLabel#status {{ color: {ACCENT}; font-size: 11px; }}

QLineEdit#search {{
    background: rgba(20,23,28,0.85); border: 1px solid #2c313b; border-radius: 11px;
    padding: 9px 14px; color: #e6e8ec; selection-background-color: {ACCENT};
}}
QLineEdit#search:focus {{ border: 1px solid {ACCENT}; }}

QTabWidget::pane {{ border: none; top: 2px; background: transparent; }}
QTabBar {{ background: transparent; }}

QScrollArea {{ border: none; background: transparent; }}

QGroupBox {{
    border: 1px solid rgba(70,223,122,0.20); border-radius: 13px; margin-top: 16px;
    background: rgba(16,19,24,0.72); padding: 10px 12px 12px 12px;
}}
QGroupBox::title {{
    subcontrol-origin: margin; left: 14px; padding: 2px 8px; color: {ACCENT};
    font-size: 10px; font-weight: 800;
}}

QLineEdit, QComboBox, QSpinBox, QDoubleSpinBox {{
    background: rgba(29,33,40,0.92); border: 1px solid #2c313b; border-radius: 9px;
    padding: 6px 10px; color: #e6e8ec; min-height: 18px; min-width: 150px;
}}
QLineEdit:focus, QComboBox:focus, QSpinBox:focus, QDoubleSpinBox:focus {{
    border: 1px solid {ACCENT};
}}
QComboBox::drop-down {{ border: none; width: 22px; }}
QComboBox QAbstractItemView {{
    background: #161a20; border: 1px solid #2c313b; color: #e6e8ec;
    selection-background-color: {ACCENT}; selection-color: #06210f; outline: none;
}}
QSpinBox::up-button, QSpinBox::down-button,
QDoubleSpinBox::up-button, QDoubleSpinBox::down-button {{ width: 16px; border: none; }}

QCheckBox::indicator {{
    width: 30px; height: 18px; border: 1px solid #39404c; border-radius: 6px;
    background: #14171c;
}}
QCheckBox::indicator:hover {{ border: 1px solid {ACCENT}; }}
QCheckBox::indicator:checked {{
    background: qlineargradient(x1:0, y1:0, x2:0, y2:1,
        stop:0 #5cf08c, stop:1 {ACCENT});
    border: 1px solid {ACCENT};
}}

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


class ChipTabBar(QTabBar):
    """Tab bar drawn as rounded chips, each tinted with its section color."""

    def __init__(self, colors: dict, parent=None):
        super().__init__(parent)
        self._colors = colors
        self.setDrawBase(False)
        self.setExpanding(False)
        self.setUsesScrollButtons(False)
        self.setElideMode(Qt.TextElideMode.ElideRight)
        self.setCursor(Qt.CursorShape.PointingHandCursor)

    def _color(self, i: int) -> QColor:
        return QColor(self._colors.get(self.tabText(i), "#8b93a1"))

    def tabSizeHint(self, index):  # noqa: N802
        s = super().tabSizeHint(index)
        s.setHeight(36)
        return s

    def paintEvent(self, event):  # noqa: N802
        p = QPainter(self)
        p.setRenderHint(QPainter.RenderHint.Antialiasing)
        cur = self.currentIndex()
        for i in range(self.count()):
            r = QRectF(self.tabRect(i)).adjusted(2, 5, -2, -5)
            col = self._color(i)
            selected = i == cur
            fill = QColor(col)
            fill.setAlpha(44 if selected else 18)
            edge = QColor(col)
            edge.setAlpha(255 if selected else 130)
            p.setBrush(fill)
            p.setPen(QPen(edge, 1.6 if selected else 1.2))
            p.drawRoundedRect(r, 9, 9)
            p.setPen(QColor("#f4f6f8") if selected else QColor("#aab2bf"))
            f = self.font()
            f.setBold(selected)
            p.setFont(f)
            txt = p.fontMetrics().elidedText(
                self.tabText(i), Qt.TextElideMode.ElideRight, int(r.width() - 12))
            p.drawText(r, Qt.AlignmentFlag.AlignCenter, txt)
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
        self.add_combo = QComboBox()
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
        available = [k for k in self._all_keys if k not in present]
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
    def __init__(self, parent=None):
        super().__init__(parent)
        self.setObjectName("root")
        self.setWindowTitle("Overlay Settings")
        self.resize(640, 800)
        self.setMinimumSize(480, 560)
        self.setStyleSheet(STYLE)

        self.working = config._deep_merge(config.full_defaults(), config.CFG)
        self._rows: list[dict] = []   # {widget, text, groups}
        self._groups: list[QGroupBox] = []
        self._carbon = _carbon_tile()

        root = QVBoxLayout(self)
        root.setContentsMargins(18, 16, 18, 14)
        root.setSpacing(12)

        title = QLabel("Overlay Settings")
        title.setObjectName("title")
        subtitle = QLabel("Customize every widget \u2022 changes apply live")
        subtitle.setObjectName("subtitle")
        root.addWidget(title)
        root.addWidget(subtitle)

        self.search = QLineEdit()
        self.search.setObjectName("search")
        self.search.setPlaceholderText("\U0001F50D  Search settings\u2026")
        self.search.setClearButtonEnabled(True)
        self.search.textChanged.connect(self._filter)
        root.addWidget(self.search)

        self.tabs = QTabWidget()
        self.tabs.setTabBar(ChipTabBar(TAB_COLORS))
        root.addWidget(self.tabs, 1)

        self.status = QLabel("")
        self.status.setObjectName("status")
        root.addWidget(self.status)
        self._status_timer = QTimer(self)
        self._status_timer.setSingleShot(True)
        self._status_timer.timeout.connect(lambda: self.status.setText(""))

        # Debounce disk writes so dragging a spin box doesn't save on every step.
        self._save_timer = QTimer(self)
        self._save_timer.setSingleShot(True)
        self._save_timer.timeout.connect(self._autosave)

        controls = QHBoxLayout()
        controls.setSpacing(8)
        self.live_chk = QCheckBox("Apply live")
        self.live_chk.setChecked(True)
        controls.addWidget(self.live_chk)
        self.autosave_chk = QCheckBox("Auto-save")
        self.autosave_chk.setChecked(True)
        controls.addWidget(self.autosave_chk)
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

        self._build_tabs()

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

    # --- UI construction ----------------------------------------------------

    def _build_tabs(self) -> None:
        self.tabs.clear()
        self._rows.clear()
        self._groups.clear()
        scalars = {k: v for k, v in config.DEFAULTS.items() if not isinstance(v, dict)}
        if scalars:
            self.tabs.addTab(self._scroll(self._build_section(scalars, [], [])), "General")
        for key, val in config.DEFAULTS.items():
            if isinstance(val, dict):
                self.tabs.addTab(
                    self._scroll(self._build_section(val, [key], [])), _pretty(key))

    def _scroll(self, inner: QWidget) -> QScrollArea:
        area = QScrollArea()
        area.setWidgetResizable(True)
        area.setWidget(inner)
        area.setStyleSheet("background: transparent;")
        area.viewport().setStyleSheet("background: transparent;")
        return area

    def _build_section(self, schema: dict, path: list, groups: list) -> QWidget:
        container = QWidget()
        v = QVBoxLayout(container)
        v.setContentsMargins(4, 4, 4, 4)
        v.setSpacing(7)

        # The Map tab gets a one-shot action to re-learn the live track.
        if path == ["map"]:
            rescan = QPushButton("\u21BB  Rescan track now")
            rescan.setObjectName("warn")
            rescan.setCursor(Qt.CursorShape.PointingHandCursor)
            rescan.clicked.connect(self._rescan_track)
            v.addWidget(rescan)
            hint = QLabel("Re-learns the current track from your driving and "
                          "overwrites its saved scan.")
            hint.setWordWrap(True)
            hint.setStyleSheet("color: #9aa3ad; font-size: 11px;")
            v.addWidget(hint)

        # Scalars (and palette) first, then nested groups.
        for key, default_val in schema.items():
            if isinstance(default_val, dict):
                continue
            cur_path = path + [key]
            value = _get_at(self.working, cur_path)
            if isinstance(default_val, list) and key in ("palette", "column_order"):
                continue  # rendered as its own group below
            v.addWidget(self._leaf_row(cur_path, default_val, value, groups))

        for key, default_val in schema.items():
            cur_path = path + [key]
            if key == "palette" and isinstance(default_val, list):
                box = self._group("Palette", cur_path, groups)
                value = _get_at(self.working, cur_path)
                box.layout().addWidget(
                    PaletteEditor(value, lambda x, p=cur_path: self._set(p, x)))
                v.addWidget(box)
            elif key == "column_order" and isinstance(default_val, list):
                box = self._group("Column order", cur_path, groups)
                value = _get_at(self.working, cur_path)
                box.layout().addWidget(
                    OrderEditor(value, COLUMN_LABELS, config.TABLE_COLUMNS,
                                lambda x, p=cur_path: self._set(p, x)))
                v.addWidget(box)
            elif isinstance(default_val, dict):
                box = self._group(_pretty(key), cur_path, groups)
                box.layout().addWidget(
                    self._build_section(default_val, cur_path, groups + [box]))
                v.addWidget(box)

        v.addStretch(1)
        return container

    def _group(self, title: str, path: list, groups: list) -> QGroupBox:
        box = QGroupBox(title.upper())
        lay = QVBoxLayout(box)
        lay.setContentsMargins(10, 6, 10, 8)
        lay.setSpacing(6)
        self._groups.append(box)
        box._ed_search = " ".join(_pretty(p) for p in path).lower()  # type: ignore
        return box

    def _leaf_row(self, path, default_val, value, groups) -> QWidget:
        row = QWidget()
        h = QHBoxLayout(row)
        h.setContentsMargins(2, 1, 2, 1)
        label = QLabel(_pretty(path[-1]))
        label.setMinimumWidth(150)
        h.addWidget(label)
        h.addWidget(self._control(path, default_val, value))
        h.addStretch(1)
        self._rows.append({
            "widget": row,
            "text": " ".join(_pretty(p) for p in path).lower(),
            "groups": list(groups),
        })
        return row

    def _control(self, path: list, default_val, value) -> QWidget:
        options = _enum_options(path)
        if options:
            combo = QComboBox()
            for opt in options:
                combo.addItem(_option_label(opt), opt)
            combo.setCurrentIndex(options.index(value) if value in options else 0)
            combo.currentIndexChanged.connect(
                lambda _i, p=path, c=combo: self._set(p, c.currentData()))
            return combo
        if _is_color(path, default_val):
            return ColorButton(value, lambda v, p=path: self._set(p, v))
        if isinstance(default_val, bool):
            chk = QCheckBox()
            chk.setChecked(bool(value))
            chk.toggled.connect(lambda v, p=path: self._set(p, bool(v)))
            return chk
        if isinstance(default_val, int):
            sp = QSpinBox()
            sp.setRange(-100000, 100000)
            sp.setValue(int(value))
            sp.valueChanged.connect(lambda v, p=path: self._set(p, int(v)))
            return sp
        if isinstance(default_val, float):
            dsp = QDoubleSpinBox()
            dsp.setRange(-100000.0, 100000.0)
            dsp.setDecimals(3)
            dsp.setSingleStep(0.01)
            dsp.setValue(float(value))
            dsp.valueChanged.connect(lambda v, p=path: self._set(p, float(v)))
            return dsp
        edit = QLineEdit(str(value))
        edit.textChanged.connect(lambda v, p=path: self._set(p, v))
        return edit

    # --- search filtering ---------------------------------------------------

    def _filter(self, text: str) -> None:
        t = text.lower().strip()
        live_groups = set()
        for r in self._rows:
            vis = (t in r["text"]) if t else True
            r["widget"].setVisible(vis)
            if vis:
                for g in r["groups"]:
                    live_groups.add(id(g))
        for box in self._groups:
            if not t:
                box.setVisible(True)
            else:
                own = t in getattr(box, "_ed_search", "")
                box.setVisible(own or id(box) in live_groups)

    # --- value changes ------------------------------------------------------

    def _rescan_track(self) -> None:
        if config.request_rescan():
            self._flash("Rescanning track\u2026 drive a lap")
        else:
            self._flash("Start the overlay first to rescan")

    def _flash(self, msg: str) -> None:
        self.status.setText(msg)
        self._status_timer.start(2500)

    def _set(self, path: list, value) -> None:
        _set_at(self.working, path, value)
        if self.live_chk.isChecked():
            config.set_cfg(self.working)
        if self.autosave_chk.isChecked():
            self._flash("Modified \u2014 saving\u2026")
            self._save_timer.start(400)
        else:
            self._flash("Modified \u2014 unsaved")

    def _autosave(self) -> None:
        config.save(self.working)
        self._flash("Saved to overlay_config.json")

    def _apply(self) -> None:
        config.set_cfg(self.working)
        self._flash("Applied to running overlay")

    def _save(self) -> None:
        self._save_timer.stop()
        config.set_cfg(self.working)
        config.save(self.working)
        self._flash("Saved to overlay_config.json")

    def _reset(self) -> None:
        self.working = config.full_defaults()
        self._build_tabs()
        self._filter(self.search.text())
        if self.live_chk.isChecked():
            config.set_cfg(self.working)
        if self.autosave_chk.isChecked():
            self._save_timer.start(400)
        self._flash("Reset to defaults")

    def _reload(self) -> None:
        self._save_timer.stop()
        self.working = config._deep_merge(config.full_defaults(), config.load())
        self._build_tabs()
        self._filter(self.search.text())
        if self.live_chk.isChecked():
            config.set_cfg(self.working)
        self._flash("Reloaded from file")


def main() -> int:
    app = QApplication(sys.argv)
    editor = ConfigEditor()
    editor.show()
    return app.exec()


if __name__ == "__main__":
    sys.exit(main())
