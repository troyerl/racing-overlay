"""Application-modal busy dialog with a timer-driven spinner (no progress bar)."""

from __future__ import annotations

from PyQt6.QtCore import QRectF, Qt, QTimer
from PyQt6.QtGui import QColor, QPainter, QPen
from PyQt6.QtWidgets import QDialog, QHBoxLayout, QLabel, QVBoxLayout, QWidget


class SpinnerWidget(QWidget):
    """Paints a rotating arc; advances on a QTimer so it keeps spinning."""

    def __init__(self, parent=None, *, size: int = 28) -> None:
        super().__init__(parent)
        self._angle = 0
        self._size = size
        self.setFixedSize(size, size)
        self._timer = QTimer(self)
        self._timer.setInterval(16)
        self._timer.timeout.connect(self._tick)

    def start(self) -> None:
        if not self._timer.isActive():
            self._timer.start()

    def stop(self) -> None:
        self._timer.stop()

    def _tick(self) -> None:
        self._angle = (self._angle + 10) % 360
        self.update()

    def paintEvent(self, _event) -> None:  # noqa: N802
        p = QPainter(self)
        p.setRenderHint(QPainter.RenderHint.Antialiasing)
        margin = 2.5
        rect = QRectF(margin, margin, self._size - 2 * margin, self._size - 2 * margin)
        pen = QPen(QColor("#6b7280"))
        pen.setWidthF(2.5)
        pen.setCapStyle(Qt.PenCapStyle.RoundCap)
        p.setPen(pen)
        p.drawEllipse(rect)
        pen.setColor(QColor("#e5e7eb"))
        p.setPen(pen)
        # Sweep a short arc that rotates with _angle.
        p.drawArc(rect, int((-self._angle) * 16), 90 * 16)
        p.end()


class BusySpinnerDialog(QDialog):
    """Modal “Switching preset” dialog with an animated spinner + message."""

    def __init__(self, message: str = "Loading preset\u2026", parent=None) -> None:
        super().__init__(parent)
        self.setWindowTitle("Switching preset")
        self.setWindowModality(Qt.WindowModality.ApplicationModal)
        self.setModal(True)
        self.setWindowFlags(
            Qt.WindowType.Dialog
            | Qt.WindowType.CustomizeWindowHint
            | Qt.WindowType.WindowTitleHint
        )
        self.setFixedWidth(320)

        self._spinner = SpinnerWidget(self)
        self._label = QLabel(message, self)
        self._label.setWordWrap(True)
        self._label.setAlignment(
            Qt.AlignmentFlag.AlignLeft | Qt.AlignmentFlag.AlignVCenter)

        row = QHBoxLayout()
        row.setContentsMargins(16, 16, 16, 16)
        row.setSpacing(12)
        row.addWidget(self._spinner, 0, Qt.AlignmentFlag.AlignVCenter)
        row.addWidget(self._label, 1)

        root = QVBoxLayout(self)
        root.setContentsMargins(0, 0, 0, 0)
        root.addLayout(row)

    def set_message(self, message: str) -> None:
        self._label.setText(message)

    def start(self) -> None:
        self._spinner.start()

    def stop(self) -> None:
        self._spinner.stop()

    def showEvent(self, event) -> None:  # noqa: N802
        super().showEvent(event)
        self.start()

    def closeEvent(self, event) -> None:  # noqa: N802
        self.stop()
        super().closeEvent(event)
