"""Track Scan: in-session track editing + optional HTML loop import."""

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
    except ImportError:
        return None
    except (OSError, ValueError):
        return None


class TrackImportV2Panel(QFrame):
    """Track Scan card: edit pit/merge on map in session, or import loop from HTML."""

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

        t = QLabel("Edit track on map")
        t.setObjectName("enableTitle")
        v.addWidget(t)

        hint = QLabel(
            "While on track in iRacing, draw or adjust the pit road (red) and "
            "exit merge line (blue) on the overlay map. Optional yellow entry "
            "blend is for tracks that have a distinct commit lane — skip it "
            "when they don't. Scroll to zoom, Middle-click drag or Shift-drag "
            "to pan; drag handles to adjust points. Clear all pit or clear the "
            "current phase to start over on one segment. Pit end and merge start "
            "stay linked. "
            "Corner labels are edited in Track metadata below. "
            "Save loop uploads geometry without pit; Save track requires pit "
            "road + merge. In demo mode the map previews your upload for this "
            "session only.")
        hint.setObjectName("enableHint")
        hint.setWordWrap(True)
        v.addWidget(hint)

        phase_row = QHBoxLayout()
        phase_lbl = QLabel("Pit phases")
        phase_lbl.setObjectName("rowLabel")
        phase_row.addWidget(phase_lbl)
        self._phase_group = QButtonGroup(self)
        self._entry_btn = QPushButton("Entry (optional)")
        self._road_btn = QPushButton("Pit road")
        self._merge_btn = QPushButton("Merge")
        for btn, phase in (
            (self._entry_btn, "entry"),
            (self._road_btn, "road"),
            (self._merge_btn, "merge"),
        ):
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
        self._load_pit_btn = QPushButton("Load saved pit")
        self._load_pit_btn.setCursor(Qt.CursorShape.PointingHandCursor)
        self._load_pit_btn.clicked.connect(self._load_saved_pit)
        undo = QPushButton("Undo last point")
        undo.setCursor(Qt.CursorShape.PointingHandCursor)
        undo.clicked.connect(self._undo_point)
        reset_view = QPushButton("Reset view")
        reset_view.setCursor(Qt.CursorShape.PointingHandCursor)
        reset_view.clicked.connect(self._reset_pit_view)
        self._clear_phase_btn = QPushButton("Clear phase")
        self._clear_phase_btn.setObjectName("warn")
        self._clear_phase_btn.setCursor(Qt.CursorShape.PointingHandCursor)
        self._clear_phase_btn.clicked.connect(self._clear_pit_phase)
        clear_all = QPushButton("Clear all pit")
        clear_all.setObjectName("warn")
        clear_all.setCursor(Qt.CursorShape.PointingHandCursor)
        clear_all.clicked.connect(self._clear_pit_all)
        save_loop = QPushButton("Save loop")
        save_loop.setCursor(Qt.CursorShape.PointingHandCursor)
        save_loop.clicked.connect(self._save_loop)
        save = QPushButton("Save track")
        save.setCursor(Qt.CursorShape.PointingHandCursor)
        save.clicked.connect(self._save_track)
        btn_row.addWidget(self._load_pit_btn)
        btn_row.addWidget(undo)
        btn_row.addWidget(reset_view)
        btn_row.addWidget(self._clear_phase_btn)
        btn_row.addWidget(clear_all)
        btn_row.addStretch(1)
        btn_row.addWidget(save_loop)
        btn_row.addWidget(save)
        v.addLayout(btn_row)

        self._status = QLabel("")
        self._status.setObjectName("enableHint")
        self._status.setWordWrap(True)
        v.addWidget(self._status)

        sep = QFrame()
        sep.setFrameShape(QFrame.Shape.HLine)
        sep.setObjectName("cardSep")
        v.addWidget(sep)

        import_title = QLabel("Import loop from HTML (optional)")
        import_title.setObjectName("rowLabel")
        v.addWidget(import_title)

        import_hint = QLabel(
            "Replace the racing line from a members-site track page. TrackID is "
            "read from id=\"track-map-123\" in the HTML.")
        import_hint.setObjectName("enableHint")
        import_hint.setWordWrap(True)
        v.addWidget(import_hint)

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

    def _current_phase(self) -> str:
        if self._entry_btn.isChecked():
            return "entry"
        if self._merge_btn.isChecked():
            return "merge"
        return "road"

    def _set_phase(self, phase: str) -> None:
        if self._overlay is None or not hasattr(self._overlay, "set_pit_edit_mode"):
            return
        on = self._pit_edit_sw.isChecked()
        self._overlay.set_pit_edit_mode(on, phase)
        self._entry_btn.setChecked(phase == "entry")
        self._road_btn.setChecked(phase == "road")
        self._merge_btn.setChecked(phase == "merge")
        self._sync_from_overlay()

    def _pit_edit_toggled(self, on: bool) -> None:
        if self._overlay is None or not hasattr(self._overlay, "set_pit_edit_mode"):
            return
        self._overlay.set_pit_edit_mode(on, self._current_phase())
        self._sync_from_overlay()

    def _load_saved_pit(self) -> None:
        if self._overlay is None or not hasattr(self._overlay, "load_pit_into_editor"):
            return
        if self._overlay.load_pit_into_editor(force=True):
            self._report(
                "Loaded pit entry / road / merge from saved track.", flash=False)
            self._sync_from_overlay()
        else:
            self._report(
                "No saved pit geometry — click pit road points on the map, "
                "or complete pit scans first.")

    def _undo_point(self) -> None:
        if self._overlay is None or not hasattr(self._overlay, "map_widget"):
            return
        self._overlay.map_widget.pop_last_pit_edit_point()
        self._sync_from_overlay()

    def _reset_pit_view(self) -> None:
        if self._overlay is None or not hasattr(self._overlay, "map_widget"):
            return
        self._overlay.map_widget.reset_pit_edit_view()

    def _clear_pit_all(self) -> None:
        if self._overlay is None or not hasattr(self._overlay, "map_widget"):
            return
        self._overlay.map_widget.clear_pit_edit()
        self._overlay.map_widget.clear_pit()
        self._sync_from_overlay()
        self._report("Cleared all pit geometry — redraw on the map.", flash=False)

    def _clear_pit_phase(self) -> None:
        if self._overlay is None:
            return
        phase = self._current_phase()
        if hasattr(self._overlay, "clear_pit_edit_phase"):
            self._overlay.clear_pit_edit_phase(phase)
        elif hasattr(self._overlay, "map_widget"):
            self._overlay.map_widget.clear_pit_edit_phase(phase)
        self._sync_from_overlay()
        label = {"entry": "entry", "road": "pit road", "merge": "merge"}.get(
            phase, phase)
        self._report(f"Cleared {label} points.", flash=False)

    def _save_loop(self) -> None:
        if self._overlay is None:
            self._report("Start the overlay first.")
            return
        if not hasattr(self._overlay, "save_loop_v2"):
            return
        state = (self._overlay.pit_edit_state()
                 if hasattr(self._overlay, "pit_edit_state") else {})
        if not state.get("has_loop"):
            self._report("No track loaded — join a session or import HTML first.")
            return

        def _run() -> None:
            try:
                ok, msg = self._overlay.save_loop_v2()
            except Exception as exc:
                ok, msg = False, str(exc)
            self._status.setText(msg)
            self.notified.emit(msg if msg else ("Save failed" if not ok else ""))
            if ok:
                self.saved.emit(msg)
            self.refresh()

        QTimer.singleShot(0, _run)

    def _save_track(self) -> None:
        if self._overlay is None:
            self._report("Start the overlay first.")
            return
        if not hasattr(self._overlay, "save_manual_track_v2"):
            return
        state = (self._overlay.pit_edit_state()
                 if hasattr(self._overlay, "pit_edit_state") else {})
        if not state.get("has_loop"):
            self._report("No track loaded — join a session or import HTML first.")
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
        ne = state.get("entry_count", 0)
        nr = state.get("road_count", 0)
        nm = state.get("merge_count", 0)
        phase = state.get("pit_edit_phase", "road")
        self._entry_btn.setChecked(phase == "entry")
        self._road_btn.setChecked(phase == "road")
        self._merge_btn.setChecked(phase == "merge")
        if not state.get("has_loop"):
            return
        tid = state.get("authoring_track_id")
        suffix = f"TrackID {tid}" if tid is not None else "no TrackID"
        in_sim = state.get("in_sim")
        mode = "In session" if in_sim else "Authoring"
        entry_bits = f"Entry: {ne} pts · " if ne else ""
        base = self._status.text().split("\n")[0] if self._status.text() else ""
        line = (f"{mode} · {suffix} · {entry_bits}"
                f"Pit road: {nr} pts · Merge: {nm} pts.")
        if base.startswith("Saved "):
            self._status.setText(f"{base}\n{line}")
        else:
            self._status.setText(line)

    def refresh(self) -> None:
        state: dict = {}
        if self._overlay is not None and hasattr(self._overlay, "pit_edit_state"):
            state = self._overlay.pit_edit_state()
        has_loop = bool(state.get("has_loop"))
        in_sim = bool(state.get("in_sim"))
        has_saved_pit = bool(state.get("has_saved_pit"))

        self._import_btn.setEnabled(self._can_import())
        edit_enabled = has_loop
        self._pit_edit_sw.setEnabled(edit_enabled)
        self._entry_btn.setEnabled(edit_enabled)
        self._road_btn.setEnabled(edit_enabled)
        self._merge_btn.setEnabled(edit_enabled)
        self._load_pit_btn.setEnabled(edit_enabled and has_saved_pit)
        phase_counts = {
            "entry": state.get("entry_count", 0),
            "road": state.get("road_count", 0),
            "merge": state.get("merge_count", 0),
        }
        phase = state.get("pit_edit_phase", "road")
        self._clear_phase_btn.setEnabled(
            edit_enabled and phase_counts.get(phase, 0) > 0)
        if edit_enabled:
            self._sync_from_overlay()
        elif in_sim:
            self._status.setText("Track loading — pit editor enables when the loop is ready.")
        elif not self._status.text():
            self._status.setText(
                "Join a session on track to edit, or import a loop from HTML.")
