"""
A single frameless, always-on-top overlay panel that can be moved independently
and remembers its position.

Each panel is its own top-level window so the user can scatter them anywhere.
Positions persist to overlay_layout.json (see layout_store), keyed by name.

Dragging requires "edit mode" (click_through=False): a click-through window is
transparent to the mouse and cannot be grabbed. Launch with --no-clickthrough to
arrange panels, then relaunch locked; saved positions are restored either way.
"""

from __future__ import annotations

from PyQt6.QtCore import Qt, QTimer
from PyQt6.QtWidgets import QLabel, QSizeGrip, QVBoxLayout, QWidget

from . import common as oc
from . import layout_store

# Clamp how far a text panel's font scales relative to its default height.
_MIN_FONT_SCALE = 0.6
_MAX_FONT_SCALE = 4.0


class PanelWindow(QWidget):
    def __init__(
        self,
        key: str,
        content: QWidget,
        default_geom: tuple[int, int, int, int],
        layout_state: dict,
        click_through: bool = True,
        on_save=None,
    ):
        super().__init__()
        self.key = key
        self.click_through = click_through
        self.layout_state = layout_state
        # Where geometry changes are persisted. Defaults to the standalone layout
        # file; the app passes config.save_active_layout so layouts are per-preset.
        self._on_save = on_save or layout_store.save_layout
        self._content = content
        self._drag_offset = None
        self._grip = None
        self._base_height = max(1, default_geom[3])

        # For QLabel content, remember the base font size so we can scale it.
        self._base_font_pt = None
        if isinstance(content, QLabel):
            pt = content.font().pointSizeF()
            self._base_font_pt = pt if pt > 0 else 14.0

        self.setWindowFlags(
            Qt.WindowType.WindowStaysOnTopHint
            | Qt.WindowType.FramelessWindowHint
            | Qt.WindowType.Tool
        )
        self.setAttribute(Qt.WidgetAttribute.WA_TranslucentBackground, True)
        if self.click_through:
            self.setAttribute(Qt.WidgetAttribute.WA_TransparentForMouseEvents, True)
        self.setMinimumSize(90, 44)

        lay = QVBoxLayout(self)
        lay.setContentsMargins(6, 6, 6, 6)
        lay.addWidget(content)

        # A resize handle in the bottom-right corner, only in edit mode.
        if not self.click_through:
            self._grip = QSizeGrip(self)
            self._grip.resize(16, 16)

        # Debounce saves while the user drags the resize grip.
        self._save_timer = QTimer(self)
        self._save_timer.setSingleShot(True)
        self._save_timer.setInterval(300)
        self._save_timer.timeout.connect(self._save_geometry)

        stored = self.layout_state.get(key)
        x, y, w, h = stored if stored and len(stored) == 4 else default_geom
        self.setGeometry(int(x), int(y), int(w), int(h))
        self._rescale_font()

    def showEvent(self, event) -> None:  # noqa: N802 (Qt naming)
        super().showEvent(event)
        oc.set_windows_click_through(self, self.click_through)

    def set_click_through(self, value: bool) -> None:
        """Flip between locked (click-through) and edit mode (draggable) live.

        In edit mode the window grabs the mouse so it can be dragged, and shows
        a resize grip in the bottom-right corner.
        """
        value = bool(value)
        self.click_through = value
        self._drag_offset = None
        self.setAttribute(Qt.WidgetAttribute.WA_TransparentForMouseEvents, value)
        # Re-apply the Win32 styles against the live window handle.
        if self.isVisible():
            oc.set_windows_click_through(self, value)
        if value:
            if self._grip is not None:
                self._grip.deleteLater()
                self._grip = None
        elif self._grip is None:
            self._grip = QSizeGrip(self)
            self._grip.resize(16, 16)
            self._grip.move(self.width() - 16, self.height() - 16)
            self._grip.show()
        self.update()

    def resizeEvent(self, event) -> None:  # noqa: N802
        super().resizeEvent(event)
        if self._grip is not None:
            self._grip.move(self.width() - 16, self.height() - 16)
        self._rescale_font()
        # A resize that wasn't triggered by our own setGeometry should persist.
        if self.isVisible():
            self._save_timer.start()

    def _rescale_font(self) -> None:
        if self._base_font_pt is None or not isinstance(self._content, QLabel):
            return
        scale = self.height() / self._base_height
        scale = max(_MIN_FONT_SCALE, min(_MAX_FONT_SCALE, scale))
        font = self._content.font()
        font.setPointSizeF(self._base_font_pt * scale)
        self._content.setFont(font)

    def mousePressEvent(self, event) -> None:  # noqa: N802
        if not self.click_through and event.button() == Qt.MouseButton.LeftButton:
            self._drag_offset = event.globalPosition().toPoint() - self.pos()

    def mouseMoveEvent(self, event) -> None:  # noqa: N802
        if self._drag_offset is not None:
            self.move(event.globalPosition().toPoint() - self._drag_offset)

    def mouseReleaseEvent(self, event) -> None:  # noqa: N802
        if self._drag_offset is not None:
            self._drag_offset = None
            self._save_geometry()

    def _save_geometry(self) -> None:
        g = self.geometry()
        self.layout_state[self.key] = [g.x(), g.y(), g.width(), g.height()]
        self._on_save(self.layout_state)
