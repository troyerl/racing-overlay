"""Track Scan: in-session track editing + optional HTML loop import."""

from __future__ import annotations

import os
import threading

from PyQt6.QtCore import Qt, QTimer, pyqtSignal
from PyQt6.QtWidgets import (
    QButtonGroup,
    QCheckBox,
    QComboBox,
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
        return parse_track_id_from_html(html_path=path, regex_only=True)
    except ImportError:
        return None
    except (OSError, ValueError):
        return None


def _caption(text: str) -> QLabel:
    lbl = QLabel(text)
    lbl.setObjectName("enableHint")
    lbl.setWordWrap(True)
    lbl.setAlignment(Qt.AlignmentFlag.AlignHCenter)
    return lbl


class TrackImportV2Panel(QFrame):
    """Track Scan card: edit pit/merge on map in session, or import loop from HTML."""

    saved = pyqtSignal(str)
    notified = pyqtSignal(str)
    importFailed = pyqtSignal(str)
    importReady = pyqtSignal(object, object, str)

    def __init__(self, overlay=None, parent=None):
        super().__init__(parent)
        self.importFailed.connect(lambda m: self._finish_import(False, m))
        self.importReady.connect(self._on_import_ready)
        self.setObjectName("enableCard")
        self._overlay = overlay
        self._html_path: str | None = None
        self._html_track_id: int | None = None
        self._import_busy = False

        v = QVBoxLayout(self)
        v.setContentsMargins(15, 11, 15, 12)
        v.setSpacing(10)

        # --- Edit pit on map ---
        t = QLabel("Edit pit on map")
        t.setObjectName("enableTitle")
        v.addWidget(t)
        hint = QLabel(
            "Turn on edit, pick a phase, then click points on the overlay map. "
            "Lane 2 is optional (dual pits).")
        hint.setObjectName("enableHint")
        hint.setWordWrap(True)
        hint.setToolTip(
            "Yellow entry is optional when there is no distinct commit lane. "
            "Scroll to zoom; middle-click or Shift-drag to pan; drag handles "
            "to adjust. Entry end links to pit-road start (and road end to "
            "merge start). Corner labels are edited in Track metadata below.")
        v.addWidget(hint)

        edit_row = QHBoxLayout()
        edit_lbl = QLabel("Edit pit on map")
        edit_lbl.setObjectName("rowLabel")
        edit_row.addWidget(edit_lbl, 1)
        self._pit_edit_sw = QCheckBox("Enable")
        self._pit_edit_sw.setToolTip("Enable drawing / adjusting pit geometry on the map.")
        self._pit_edit_sw.toggled.connect(self._pit_edit_toggled)
        edit_row.addWidget(self._pit_edit_sw, 0, Qt.AlignmentFlag.AlignVCenter)
        v.addLayout(edit_row)

        phase_row = QHBoxLayout()
        phase_lbl = QLabel("Phase")
        phase_lbl.setObjectName("rowLabel")
        phase_row.addWidget(phase_lbl)
        self._phase_group = QButtonGroup(self)
        self._entry_btn = QPushButton("Entry (optional)")
        self._entry_btn.setToolTip(
            "Optional yellow entry / commit lane. Skip when the track has none.")
        self._road_btn = QPushButton("Pit road")
        self._road_btn.setToolTip("Main pit road (red). Required for Save track.")
        self._merge_btn = QPushButton("Merge")
        self._merge_btn.setToolTip("Exit merge line (blue). Required for Save track.")
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
        v.addLayout(phase_row)

        lane_row = QHBoxLayout()
        lane_lbl = QLabel("Lane")
        lane_lbl.setObjectName("rowLabel")
        lane_row.addWidget(lane_lbl)
        self._lane_group = QButtonGroup(self)
        self._lane1_btn = QPushButton("Lane 1")
        self._lane2_btn = QPushButton("Lane 2")
        self._lane2_btn.setToolTip(
            "Second pit road (e.g. Bristol). Optional at save time.")
        for btn, lane in ((self._lane1_btn, 1), (self._lane2_btn, 2)):
            btn.setCheckable(True)
            btn.setCursor(Qt.CursorShape.PointingHandCursor)
            btn.clicked.connect(lambda _=False, ln=lane: self._set_lane(ln))
            self._lane_group.addButton(btn)
            lane_row.addWidget(btn)
        self._lane1_btn.setChecked(True)
        lane_row.addStretch(1)
        v.addLayout(lane_row)

        tool_row = QHBoxLayout()
        self._load_pit_btn = QPushButton("Load saved pit")
        self._load_pit_btn.setCursor(Qt.CursorShape.PointingHandCursor)
        self._load_pit_btn.clicked.connect(self._load_saved_pit)
        undo = QPushButton("Undo last point")
        undo.setCursor(Qt.CursorShape.PointingHandCursor)
        undo.clicked.connect(self._undo_point)
        reset_view = QPushButton("Reset view")
        reset_view.setCursor(Qt.CursorShape.PointingHandCursor)
        reset_view.clicked.connect(self._reset_pit_view)
        tool_row.addWidget(self._load_pit_btn)
        tool_row.addWidget(undo)
        tool_row.addWidget(reset_view)
        tool_row.addStretch(1)
        v.addLayout(tool_row)

        # --- Clear ---
        clear_title = QLabel("Clear")
        clear_title.setObjectName("rowLabel")
        v.addWidget(clear_title)
        clear_row = QHBoxLayout()
        clear_row.setSpacing(6)
        self._clear_phase_combo = QComboBox()
        self._clear_phase_combo.setMinimumWidth(180)
        self._phase_clear_keys = (
            (1, "entry"), (1, "road"), (1, "merge"),
            (2, "entry"), (2, "road"), (2, "merge"),
        )
        self._clear_phase_btn = QPushButton("Clear selected")
        self._clear_phase_btn.setObjectName("warn")
        self._clear_phase_btn.setCursor(Qt.CursorShape.PointingHandCursor)
        self._clear_phase_btn.clicked.connect(self._clear_pit_phase)
        clear_all = QPushButton("Clear all pit")
        clear_all.setObjectName("warn")
        clear_all.setCursor(Qt.CursorShape.PointingHandCursor)
        clear_all.clicked.connect(self._clear_pit_all)
        clear_row.addWidget(self._clear_phase_combo, 1)
        clear_row.addWidget(self._clear_phase_btn)
        clear_row.addWidget(clear_all)
        v.addLayout(clear_row)

        # --- Save ---
        save_title = QLabel("Save")
        save_title.setObjectName("rowLabel")
        v.addWidget(save_title)
        save_row = QHBoxLayout()
        save_row.setSpacing(10)

        def _save_col(btn: QPushButton, caption: str) -> QVBoxLayout:
            col = QVBoxLayout()
            col.setSpacing(2)
            col.addWidget(btn)
            col.addWidget(_caption(caption))
            return col

        save_loop = QPushButton("Save loop")
        save_loop.setCursor(Qt.CursorShape.PointingHandCursor)
        save_loop.setToolTip(
            "Upload racing-line geometry without pit. Safe when the track "
            "already exists in the shared library.")
        save_loop.clicked.connect(self._save_loop)
        save_pit = QPushButton("Save pit")
        save_pit.setCursor(Qt.CursorShape.PointingHandCursor)
        save_pit.setToolTip(
            "Update pit lane on an existing local/cloud track. Entry "
            "auto-links to pit-road start.")
        save_pit.clicked.connect(self._save_pit)
        save = QPushButton("Save track")
        save.setCursor(Qt.CursorShape.PointingHandCursor)
        save.setToolTip(
            "First publish only: requires pit road + merge on lane 1. "
            "Blocked if this TrackID is already in the shared library. "
            "In demo mode the map previews the upload for this session only.")
        save.clicked.connect(self._save_track)
        save_row.addLayout(_save_col(save_loop, "Racing line only"), 1)
        save_row.addLayout(_save_col(save_pit, "Update pit on existing track"), 1)
        save_row.addLayout(_save_col(save, "First publish (loop + pit)"), 1)
        v.addLayout(save_row)

        self._status = QLabel("")
        self._status.setObjectName("enableHint")
        self._status.setWordWrap(True)
        v.addWidget(self._status)

        sep = QFrame()
        sep.setFrameShape(QFrame.Shape.HLine)
        sep.setObjectName("cardSep")
        v.addWidget(sep)

        import_title = QLabel("Import loop from HTML")
        import_title.setObjectName("enableTitle")
        v.addWidget(import_title)
        import_hint = QLabel(
            "Optional. Choose a members track HTML page, then Import loop.")
        import_hint.setObjectName("enableHint")
        import_hint.setWordWrap(True)
        import_hint.setToolTip(
            "TrackID is read from id=\"track-map-123\" on the outer members "
            "track-map div.")
        v.addWidget(import_hint)

        row = QHBoxLayout()
        self._path_lbl = QLabel("No HTML file chosen")
        self._path_lbl.setObjectName("enableHint")
        self._path_lbl.setWordWrap(True)
        self._browse_btn = QPushButton("Choose HTML\u2026")
        self._browse_btn.setCursor(Qt.CursorShape.PointingHandCursor)
        self._browse_btn.clicked.connect(self._browse)
        row.addWidget(self._path_lbl, 1)
        row.addWidget(self._browse_btn, 0)
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
            self._report("Import requires a .html / .htm members page.")
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
        self._report(
            f"Ready — click Import loop to load {os.path.basename(path)}.",
            flash=False)

    def _set_import_busy(self, busy: bool) -> None:
        self._import_busy = busy
        self._browse_btn.setEnabled(not busy)
        self._import_btn.setEnabled(not busy and self._can_import())

    def _import_loop(self) -> None:
        if self._import_busy:
            return
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
        self._set_import_busy(True)
        self._report(f"Importing {os.path.basename(path)}\u2026", flash=False)

        def _worker() -> None:
            try:
                ok, msg, doc, tid = self._overlay.parse_loop_v2(path)
            except Exception as exc:
                self.importFailed.emit(str(exc))
                return
            if not ok:
                self.importFailed.emit(msg)
                return
            self.importReady.emit(doc, tid, path)

        threading.Thread(target=_worker, daemon=True).start()

    def _on_import_ready(self, doc: dict, tid: int, path: str) -> None:
        self._apply_import(doc, tid, path)

    def _apply_import(self, doc: dict, tid: int, path: str) -> None:
        try:
            ok, msg = self._overlay.apply_loop_v2_import(doc, tid, path)
        except Exception as exc:
            ok, msg = False, str(exc)
        self._finish_import(ok, msg)

    def _finish_import(self, ok: bool, msg: str) -> None:
        self._set_import_busy(False)
        self._status.setText(msg)
        self.notified.emit(msg if msg else ("Import failed" if not ok else ""))
        if ok:
            self.saved.emit(msg)
            self._sync_from_overlay()
        self.refresh()

    def _current_phase(self) -> str:
        if self._entry_btn.isChecked():
            return "entry"
        if self._merge_btn.isChecked():
            return "merge"
        return "road"

    def _current_lane(self) -> int:
        return 2 if self._lane2_btn.isChecked() else 1

    def _set_lane(self, lane: int) -> None:
        if self._overlay is None or not hasattr(self._overlay, "set_pit_edit_mode"):
            return
        on = self._pit_edit_sw.isChecked()
        self._overlay.set_pit_edit_mode(on, self._current_phase(), lane=lane)
        self._lane1_btn.setChecked(lane == 1)
        self._lane2_btn.setChecked(lane == 2)
        self._sync_from_overlay()

    def _set_phase(self, phase: str) -> None:
        if self._overlay is None or not hasattr(self._overlay, "set_pit_edit_mode"):
            return
        on = self._pit_edit_sw.isChecked()
        self._overlay.set_pit_edit_mode(on, phase, lane=self._current_lane())
        self._entry_btn.setChecked(phase == "entry")
        self._road_btn.setChecked(phase == "road")
        self._merge_btn.setChecked(phase == "merge")
        self._sync_from_overlay()

    def _pit_edit_toggled(self, on: bool) -> None:
        if self._overlay is None or not hasattr(self._overlay, "set_pit_edit_mode"):
            return
        self._overlay.set_pit_edit_mode(on, self._current_phase(),
                                       lane=self._current_lane())
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

    def _phase_clear_label(self, lane: int, phase: str, count: int) -> str:
        names = {"entry": "Entry", "road": "Pit road", "merge": "Merge"}
        name = names.get(phase, phase)
        prefix = f"Lane {lane} · " if lane == 2 else ""
        return f"{prefix}{name} ({count} pt{'s' if count != 1 else ''})"

    def _phase_counts(self, state: dict) -> dict[tuple[int, str], int]:
        return {
            (1, "entry"): state.get("entry_count", 0),
            (1, "road"): state.get("road_count", 0),
            (1, "merge"): state.get("merge_count", 0),
            (2, "entry"): state.get("entry_count_2", 0),
            (2, "road"): state.get("road_count_2", 0),
            (2, "merge"): state.get("merge_count_2", 0),
        }

    def _refresh_clear_phase_combo(self, state: dict) -> None:
        counts = self._phase_counts(state)
        active = (state.get("pit_edit_lane", 1), state.get("pit_edit_phase", "road"))
        prev = self._clear_phase_combo.currentData()
        self._clear_phase_combo.blockSignals(True)
        self._clear_phase_combo.clear()
        for key in self._phase_clear_keys:
            lane, phase = key
            self._clear_phase_combo.addItem(
                self._phase_clear_label(lane, phase, counts[key]), key)
        pick = prev if prev in self._phase_clear_keys else active
        if pick in self._phase_clear_keys:
            idx = self._phase_clear_keys.index(pick)
            self._clear_phase_combo.setCurrentIndex(idx)
        self._clear_phase_combo.blockSignals(False)
        cur = self._clear_phase_combo.currentData()
        self._clear_phase_btn.setEnabled(counts.get(cur, 0) > 0 if cur else False)

    def _clear_phase_selection(self) -> tuple[int, str]:
        data = self._clear_phase_combo.currentData()
        if isinstance(data, tuple) and len(data) == 2:
            lane, phase = data
            if lane in (1, 2) and phase in ("entry", "road", "merge"):
                return lane, phase
        return self._current_lane(), "road"

    def _clear_pit_phase(self) -> None:
        if self._overlay is None:
            return
        lane, phase = self._clear_phase_selection()
        state = (self._overlay.pit_edit_state()
                 if hasattr(self._overlay, "pit_edit_state") else {})
        counts = self._phase_counts(state)
        if counts.get((lane, phase), 0) <= 0:
            label = self._phase_clear_label(lane, phase, 0).split(" (")[0]
            self._report(f"{label} has no points to clear.", flash=False)
            return
        if hasattr(self._overlay, "clear_pit_edit_phase"):
            self._overlay.clear_pit_edit_phase(phase, lane=lane)
        elif hasattr(self._overlay, "map_widget"):
            self._overlay.map_widget.clear_pit_edit_phase(phase, lane=lane)
        self._sync_from_overlay()
        label = self._phase_clear_label(lane, phase, 0).split(" (")[0]
        self._report(f"Cleared {label.lower()} points.", flash=False)

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

    def _save_pit(self) -> None:
        if self._overlay is None:
            self._report("Start the overlay first.")
            return
        if not hasattr(self._overlay, "save_pit_v2"):
            return
        state = (self._overlay.pit_edit_state()
                 if hasattr(self._overlay, "pit_edit_state") else {})
        if not state.get("has_loop"):
            self._report("No track loaded — join a session or import HTML first.")
            return

        def _run() -> None:
            try:
                ok, msg = self._overlay.save_pit_v2()
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
        ne2 = state.get("entry_count_2", 0)
        nr2 = state.get("road_count_2", 0)
        nm2 = state.get("merge_count_2", 0)
        phase = state.get("pit_edit_phase", "road")
        lane = state.get("pit_edit_lane", 1)
        self._entry_btn.setChecked(phase == "entry")
        self._road_btn.setChecked(phase == "road")
        self._merge_btn.setChecked(phase == "merge")
        self._lane1_btn.setChecked(lane == 1)
        self._lane2_btn.setChecked(lane == 2)
        if not state.get("has_loop"):
            return
        tid = state.get("authoring_track_id")
        suffix = f"TrackID {tid}" if tid is not None else "no TrackID"
        in_sim = state.get("in_sim")
        mode = "In session" if in_sim else "Authoring"
        entry_bits = f"Entry: {ne} pts · " if ne else ""
        lane2_bits = ""
        if nr2 or nm2 or ne2:
            e2 = f"Entry: {ne2} · " if ne2 else ""
            lane2_bits = f" · Lane 2: {e2}road {nr2}, merge {nm2}"
        base = self._status.text().split("\n")[0] if self._status.text() else ""
        line = (f"{mode} · {suffix} · Lane {lane} · {entry_bits}"
                f"Pit road: {nr} pts · Merge: {nm} pts{lane2_bits}.")
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

        self._import_btn.setEnabled(
            not self._import_busy and self._can_import())
        if hasattr(self, "_browse_btn"):
            self._browse_btn.setEnabled(not self._import_busy)
        edit_enabled = has_loop
        self._pit_edit_sw.setEnabled(edit_enabled)
        self._entry_btn.setEnabled(edit_enabled)
        self._road_btn.setEnabled(edit_enabled)
        self._merge_btn.setEnabled(edit_enabled)
        self._lane1_btn.setEnabled(edit_enabled)
        self._lane2_btn.setEnabled(edit_enabled)
        self._load_pit_btn.setEnabled(
            edit_enabled and (has_saved_pit or state.get("has_saved_pit_2")))
        self._clear_phase_combo.setEnabled(edit_enabled)
        if edit_enabled:
            self._refresh_clear_phase_combo(state)
        else:
            self._clear_phase_btn.setEnabled(False)
        if edit_enabled:
            self._sync_from_overlay()
        elif in_sim:
            self._status.setText(
                "Track loading — pit editor enables when the loop is ready.")
        elif not self._status.text():
            self._status.setText(
                "Join a session on track to edit, or import a loop from HTML.")
