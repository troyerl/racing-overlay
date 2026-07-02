"""Track Scan v2: HTML loop import + manual pit authoring on the live map."""

from __future__ import annotations

import os

from PyQt6.QtCore import Qt, QTimer, pyqtSignal
from PyQt6.QtWidgets import (
    QButtonGroup,
    QCheckBox,
    QFileDialog,
    QFrame,
    QHBoxLayout,
    QLabel,
    QPushButton,
    QVBoxLayout,
)


def _parse_html_track_id(path: str) -> int | None:
    try:
        from tools.svg_layers_to_track_v2 import parse_track_id_from_html
        return parse_track_id_from_html(html_path=path)
    except Exception:
        return None


class TrackImportV2Panel(QFrame):
    """Track Scan card: import members HTML loop, draw pit on overlay map."""

    saved = pyqtSignal(str)
    notified = pyqtSignal(str)

    def __init__(self, overlay=None, parent=None):
        super().__init__(parent)
        self.setObjectName("enableCard")
        self._overlay = overlay
        self._html_path: str | None = None
        self._html_track_id: int | None = None

        v = QVBoxLayout(self)
        v.setContentsMargins(15, 11, 15, 12)
        v.setSpacing(8)

        t = QLabel("HTML loop import (v2)")
        t.setObjectName("enableTitle")
        v.addWidget(t)

        hint = QLabel(
            "Import the racing loop from a members-site track page (active-config "
            "layer only). TrackID is read from the HTML (id=\"track-map-123\") — "
            "iRacing does not need to be running. Draw pit road and merge on the "
            "overlay map; entry blend is generated on save.")
        hint.setObjectName("enableHint")
        hint.setWordWrap(True)
        v.addWidget(hint)

        row = QHBoxLayout()
        self._path_lbl = QLabel("No HTML file chosen")
        self._path_lbl.setObjectName("enableHint")
        self._path_lbl.setWordWrap(True)
        browse = QPushButton("Choose HTML…")
        browse.setCursor(Qt.CursorShape.PointingHandCursor)
        browse.clicked.connect(self._browse)
        row.addWidget(self._path_lbl, 1)
        row.addWidget(browse, 0)
        v.addLayout(row)

        self._import_btn = QPushButton("Import loop")
        self._import_btn.setEnabled(False)
        self._import_btn.setCursor(Qt.CursorShape.PointingHandCursor)
        self._import_btn.clicked.connect(self._import_loop)
        v.addWidget(self._import_btn)

        phase_row = QHBoxLayout()
        phase_lbl = QLabel("Draw on map")
        phase_lbl.setObjectName("rowLabel")
        phase_row.addWidget(phase_lbl)
        self._phase_group = QButtonGroup(self)
        self._road_btn = QPushButton("Pit road")
        self._merge_btn = QPushButton("Merge")
        for btn, phase in ((self._road_btn, "road"), (self._merge_btn, "merge")):
            btn.setCheckable(True)
            btn.setCursor(Qt.CursorShape.PointingHandCursor)
            btn.clicked.connect(lambda _=False, p=phase: self._set_phase(p))
            self._phase_group.addButton(btn)
            phase_row.addWidget(btn)
        phase_row.addStretch(1)
        self._pit_edit_sw = QCheckBox("Enable")
        self._pit_edit_sw.toggled.connect(self._pit_edit_toggled)
        phase_row.addWidget(self._pit_edit_sw, 0, Qt.AlignmentFlag.AlignVCenter)
        v.addLayout(phase_row)

        btn_row = QHBoxLayout()
        undo = QPushButton("Undo last point")
        undo.setCursor(Qt.CursorShape.PointingHandCursor)
        undo.clicked.connect(self._undo_point)
        clear = QPushButton("Clear pit")
        clear.setObjectName("warn")
        clear.setCursor(Qt.CursorShape.PointingHandCursor)
        clear.clicked.connect(self._clear_pit)
        save = QPushButton("Save track")
        save.setCursor(Qt.CursorShape.PointingHandCursor)
        save.clicked.connect(self._save_track)
        btn_row.addWidget(undo)
        btn_row.addWidget(clear)
        btn_row.addStretch(1)
        btn_row.addWidget(save)
        v.addLayout(btn_row)

        self._status = QLabel("")
        self._status.setObjectName("enableHint")
        self._status.setWordWrap(True)
        v.addWidget(self._status)

        self._road_btn.setChecked(True)
        self.refresh()

    def set_overlay(self, overlay) -> None:
        self._overlay = overlay
        self.refresh()

    def _session_track_id(self):
        if self._overlay is None:
            return None
        if hasattr(self._overlay, "effective_track_id"):
            return self._overlay.effective_track_id()
        return getattr(self._overlay, "_track_id", None)

    def _can_import(self) -> bool:
        return bool(self._html_path and (
            self._html_track_id is not None or self._session_track_id() is not None))

    def _report(self, msg: str, *, flash: bool = True) -> None:
        self._status.setText(msg)
        if flash and msg:
            self.notified.emit(msg)

    def _browse(self) -> None:
        parent = self.window() or self
        path, _ = QFileDialog.getOpenFileName(
            parent, "Choose members track HTML", "",
            "HTML (*.html *.htm);;All files (*)")
        if not path:
            return
        ext = os.path.splitext(path)[1].lower()
        if ext not in (".html", ".htm"):
            self._report("V2 import requires a .html / .htm members page.")
            return
        self._html_path = path
        self._html_track_id = _parse_html_track_id(path)
        label = os.path.basename(path)
        if self._html_track_id is not None:
            label = f"{label}  (TrackID {self._html_track_id})"
        self._path_lbl.setText(label)
        self._import_btn.setEnabled(self._can_import())
        if self._html_track_id is None and self._session_track_id() is None:
            self._report(
                "No track-map-### id in HTML — save the outer members "
                "track-map div from DevTools.")
            return
        self._report(f"Importing {os.path.basename(path)}…", flash=False)
        self._import_loop()

    def _import_loop(self) -> None:
        if not self._html_path:
            self._report("Choose an HTML file first.")
            return
        if self._overlay is None:
            self._report("Start the overlay first.")
            return
        if not self._can_import():
            self._report(
                "No TrackID — HTML needs id=\"track-map-123\" on the outer div.")
            return

        path = self._html_path

        def _run() -> None:
            try:
                ok, msg = self._overlay.import_loop_v2(path)
            except Exception as exc:
                ok, msg = False, str(exc)
            self._status.setText(msg)
            self.notified.emit(msg if msg else ("Import failed" if not ok else ""))
            if ok:
                self.saved.emit(msg)
                self._sync_from_overlay()
            self.refresh()

        QTimer.singleShot(0, _run)

    def _set_phase(self, phase: str) -> None:
        if self._overlay is None or not hasattr(self._overlay, "set_pit_edit_mode"):
            return
        on = self._pit_edit_sw.isChecked()
        self._overlay.set_pit_edit_mode(on, phase)
        self._road_btn.setChecked(phase == "road")
        self._merge_btn.setChecked(phase == "merge")
        self._sync_from_overlay()

    def _pit_edit_toggled(self, on: bool) -> None:
        if self._overlay is None or not hasattr(self._overlay, "set_pit_edit_mode"):
            return
        phase = "merge" if self._merge_btn.isChecked() else "road"
        self._overlay.set_pit_edit_mode(on, phase)
        self._sync_from_overlay()

    def _undo_point(self) -> None:
        if self._overlay is None or not hasattr(self._overlay, "map_widget"):
            return
        self._overlay.map_widget.pop_last_pit_edit_point()
        self._sync_from_overlay()

    def _clear_pit(self) -> None:
        if self._overlay is None or not hasattr(self._overlay, "map_widget"):
            return
        self._overlay.map_widget.clear_pit_edit()
        self._sync_from_overlay()

    def _save_track(self) -> None:
        if self._overlay is None:
            self._report("Start the overlay first.")
            return
        if not hasattr(self._overlay, "save_manual_track_v2"):
            return

        def _run() -> None:
            try:
                ok, msg = self._overlay.save_manual_track_v2()
            except Exception as exc:
                ok, msg = False, str(exc)
            self._status.setText(msg)
            self.notified.emit(msg if msg else ("Save failed" if not ok else ""))
            if ok:
                self.saved.emit(msg)
            self.refresh()

        QTimer.singleShot(0, _run)

    def _sync_from_overlay(self) -> None:
        if self._overlay is None or not hasattr(self._overlay, "pit_edit_state"):
            return
        state = self._overlay.pit_edit_state()
        nr = state.get("road_count", 0)
        nm = state.get("merge_count", 0)
        phase = state.get("pit_edit_phase", "road")
        self._road_btn.setChecked(phase == "road")
        self._merge_btn.setChecked(phase == "merge")
        if state.get("has_loop"):
            base = self._status.text().split("\n")[0] if self._status.text() else ""
            tid = state.get("authoring_track_id")
            suffix = f" (TrackID {tid})" if tid is not None else ""
            if base.startswith("Saved "):
                self._status.setText(
                    f"{base}\nPit road: {nr} pts · Merge: {nm} pts.{suffix}")
            else:
                self._status.setText(
                    f"Pit road: {nr} pts · Merge: {nm} pts.{suffix}")

    def refresh(self) -> None:
        has_loop = False
        if self._overlay is not None and hasattr(self._overlay, "pit_edit_state"):
            has_loop = bool(self._overlay.pit_edit_state().get("has_loop"))
        self._import_btn.setEnabled(self._can_import())
        enabled = has_loop
        self._pit_edit_sw.setEnabled(enabled)
        self._road_btn.setEnabled(enabled)
        self._merge_btn.setEnabled(enabled)
        if enabled:
            self._sync_from_overlay()
        elif not self._status.text():
            self._status.setText(
                "Choose members HTML — TrackID is read from track-map-###.")
