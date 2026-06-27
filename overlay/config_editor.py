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

from PyQt6.QtCore import Qt, QTimer
from PyQt6.QtGui import QColor
from PyQt6.QtWidgets import (
    QApplication,
    QCheckBox,
    QColorDialog,
    QComboBox,
    QDoubleSpinBox,
    QGroupBox,
    QHBoxLayout,
    QLabel,
    QLineEdit,
    QPushButton,
    QScrollArea,
    QSpinBox,
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
    "ring_source": ["rpm", "throttle", "brake"],
}

# Friendly display text for raw config option values (combo boxes show these,
# but the underlying stored value is unchanged).
OPTION_LABELS = {
    "none": "None",
    # dash metrics
    "speed": "Speed", "speed_kph": "Speed (km/h)", "speed_mph": "Speed (mph)",
    "rpm": "RPM", "gear": "Gear", "position": "Position", "lap": "Lap",
    "fuel": "Fuel", "last_lap": "Last lap", "best_lap": "Best lap",
    "cur_lap": "Current lap", "delta": "Delta", "incidents": "Incidents",
    "track_temp": "Track temp", "air_temp": "Air temp",
    # table header / footer items
    "sof": "Strength of field", "race_time": "Race time",
    "order_pill": "Order", "title": "Title", "count": "Count",
    # pit_mode
    "laps_since": "Laps since pit", "time_since": "Time since pit",
    "at_lap": "Lap pitted on", "at_time": "Race time pitted",
    # units
    "metric": "Metric (km/h, °C, L)", "imperial": "Imperial (mph, °F, gal)",
    # ring_source
    "throttle": "Throttle", "brake": "Brake",
}

# Special-cased word fixups so labels read naturally (RPM, iRating, ...).
_WORD_FIXUPS = {
    "rpm": "RPM", "sof": "SoF", "irating": "iRating", "sr": "SR", "ui": "UI",
    "id": "ID", "bg": "background", "frac": "fraction", "px": "size",
    "tau": "easing", "pct": "percent",
}

from .widgets.dash import METRIC_KEYS as _DASH_METRICS

# Items available for each table's header / footer sections (and the dash slots).
SECTION_ITEMS = {
    ("relative", "header"): ["none", "sof", "position"],
    ("relative", "footer"): ["none", "race_time", "lap", "incidents"],
    ("standings", "header"): ["none", "order_pill", "title", "count"],
    ("dash", "center"): list(_DASH_METRICS),
    ("dash", "bottom"): list(_DASH_METRICS),
}
SECTION_KEYS = {"left", "center", "right"}

ACCENT = "#4c9aff"

STYLE = f"""
QWidget#root {{ background: #14161b; }}
QWidget {{ color: #d7dae0; font-family: 'Segoe UI', 'SF Pro Text', Arial; font-size: 12px; }}
QLabel#title {{ font-size: 19px; font-weight: 600; color: #f2f4f7; }}
QLabel#subtitle {{ color: #8b93a1; font-size: 11px; }}
QLabel#status {{ color: {ACCENT}; font-size: 11px; }}

QLineEdit#search {{
    background: #1d2128; border: 1px solid #2c313b; border-radius: 9px;
    padding: 8px 12px; color: #e6e8ec;
}}
QLineEdit#search:focus {{ border: 1px solid {ACCENT}; }}

QTabWidget::pane {{ border: none; top: -1px; }}
QTabBar::tab {{
    background: transparent; color: #8b93a1; padding: 9px 15px; margin-right: 2px;
    border: none; border-bottom: 2px solid transparent; font-weight: 500;
}}
QTabBar::tab:selected {{ color: #f2f4f7; border-bottom: 2px solid {ACCENT}; }}
QTabBar::tab:hover {{ color: #cfd4dc; }}

QScrollArea {{ border: none; background: transparent; }}

QGroupBox {{
    border: 1px solid #262b34; border-radius: 12px; margin-top: 16px;
    background: #171a20; padding: 10px 12px 12px 12px;
}}
QGroupBox::title {{
    subcontrol-origin: margin; left: 14px; padding: 2px 7px; color: #9aa3b2;
    font-size: 10px; font-weight: 700;
}}

QLineEdit, QComboBox, QSpinBox, QDoubleSpinBox {{
    background: #1d2128; border: 1px solid #2c313b; border-radius: 8px;
    padding: 5px 9px; color: #e6e8ec; min-height: 18px; min-width: 150px;
}}
QLineEdit:focus, QComboBox:focus, QSpinBox:focus, QDoubleSpinBox:focus {{
    border: 1px solid {ACCENT};
}}
QComboBox::drop-down {{ border: none; width: 20px; }}
QComboBox QAbstractItemView {{
    background: #1d2128; border: 1px solid #2c313b; color: #e6e8ec;
    selection-background-color: {ACCENT}; selection-color: #0b1220; outline: none;
}}
QSpinBox::up-button, QSpinBox::down-button,
QDoubleSpinBox::up-button, QDoubleSpinBox::down-button {{ width: 16px; border: none; }}

QCheckBox::indicator {{
    width: 18px; height: 18px; border: 1px solid #39404c; border-radius: 5px;
    background: #1d2128;
}}
QCheckBox::indicator:checked {{ background: {ACCENT}; border: 1px solid {ACCENT}; }}

QPushButton {{
    background: #222732; border: 1px solid #2f3540; border-radius: 9px;
    padding: 8px 15px; color: #dfe3ea;
}}
QPushButton:hover {{ background: #2a3140; }}
QPushButton#primary {{
    background: {ACCENT}; border: 1px solid {ACCENT}; color: #0b1220; font-weight: 600;
}}
QPushButton#primary:hover {{ background: #5ea7ff; }}

QScrollBar:vertical {{ background: transparent; width: 11px; margin: 2px; }}
QScrollBar::handle:vertical {{ background: #2c313b; border-radius: 5px; min-height: 32px; }}
QScrollBar::handle:vertical:hover {{ background: #3a4150; }}
QScrollBar::add-line, QScrollBar::sub-line {{ height: 0; }}
QScrollBar::add-page, QScrollBar::sub-page {{ background: transparent; }}
"""


def _enum_options(path: list):
    """Return dropdown options for a path, or None if it isn't an enum."""
    if not path:
        return None
    key = path[-1]
    if key in ENUMS:
        return ENUMS[key]
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
        for text, slot, primary in (
            ("Reset", self._reset, False),
            ("Reload", self._reload, False),
            ("Apply", self._apply, False),
            ("Save", self._save, True),
        ):
            b = QPushButton(text)
            b.setCursor(Qt.CursorShape.PointingHandCursor)
            if primary:
                b.setObjectName("primary")
            b.clicked.connect(slot)
            controls.addWidget(b)
        root.addLayout(controls)

        self._build_tabs()

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
        return area

    def _build_section(self, schema: dict, path: list, groups: list) -> QWidget:
        container = QWidget()
        v = QVBoxLayout(container)
        v.setContentsMargins(4, 4, 4, 4)
        v.setSpacing(7)

        # Scalars (and palette) first, then nested groups.
        for key, default_val in schema.items():
            if isinstance(default_val, dict):
                continue
            cur_path = path + [key]
            value = _get_at(self.working, cur_path)
            if key == "palette" and isinstance(default_val, list):
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
