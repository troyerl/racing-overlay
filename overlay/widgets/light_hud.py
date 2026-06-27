"""
LightSpeed Overlay - a native (non-browser) iRacing Fuel & Delta HUD.

This is a frameless, always-on-top, click-through overlay built with PyQt6.
It polls the iRacing shared-memory telemetry feed via pyirsdk.

Notes on the corrections made vs. the common template:
  * True click-through on Windows needs the WS_EX_LAYERED | WS_EX_TRANSPARENT
    extended window styles. Qt's WA_TransparentForMouseEvents alone is NOT
    reliable for frameless/translucent windows on Windows, so we also set the
    Win32 styles via ctypes when available.
  * iRacing reports FuelLevel in LITERS, not gallons. We label liters by
    default and offer an explicit conversion flag.
  * QTimer is a regular software timer, not a hardware/vsync timer. Polling at
    ~60 Hz matches iRacing's 60 Hz telemetry tick, which is the real reason to
    use 16 ms (not frame timing).
"""

from __future__ import annotations

import signal
import sys

from PyQt6.QtCore import Qt, QTimer
from PyQt6.QtWidgets import (
    QApplication,
    QLabel,
    QVBoxLayout,
    QWidget,
)

from .. import common as oc
from .. import config

LITERS_PER_GALLON = 3.785411784


def _hex(color) -> str:
    c = config.qcolor(color)
    return f"#{c.red():02x}{c.green():02x}{c.blue():02x}"


def _rgba(color) -> str:
    c = config.qcolor(color)
    return f"rgba({c.red()}, {c.green()}, {c.blue()}, {c.alpha() / 255:.3f})"


class LightSpeedOverlay(QWidget):
    def __init__(
        self,
        use_gallons: bool = False,
        click_through: bool = True,
        demo: bool = False,
    ):
        super().__init__()

        self.use_gallons = use_gallons
        self.click_through = click_through

        self.ir = oc.make_irsdk(demo=demo)

        flags = (
            Qt.WindowType.WindowStaysOnTopHint
            | Qt.WindowType.FramelessWindowHint
            | Qt.WindowType.Tool  # keep it out of the Alt-Tab / taskbar
        )
        self.setWindowFlags(flags)
        self.setAttribute(Qt.WidgetAttribute.WA_TranslucentBackground, True)
        if self.click_through:
            self.setAttribute(Qt.WidgetAttribute.WA_TransparentForMouseEvents, True)

        self.init_ui()

        # ~60 Hz to match iRacing's telemetry update rate (not "frame timing").
        self.timer = QTimer(self)
        self.timer.timeout.connect(self.update_telemetry)
        self.timer.start(16)

    def init_ui(self) -> None:
        layout = QVBoxLayout()
        layout.setContentsMargins(10, 10, 10, 10)
        layout.setSpacing(4)

        lc = config.CFG["light_hud"]
        font_px = max(6, round(lc['font_px'] * config.text_scale_for("light_hud")))
        cols = lc['colors']
        self.style_sheet = f"""
            QLabel {{
                color: {_hex(cols['text'])};
                font-family: '{config.CFG.get('font_family', 'Segoe UI')}', Arial, sans-serif;
                font-size: {font_px}px;
                font-weight: bold;
                background: qlineargradient(x1:0, y1:0, x2:0, y2:1,
                    stop:0 {_rgba(cols['bg_top'])}, stop:1 {_rgba(cols['bg_bottom'])});
                border: 1px solid {_rgba(cols['border'])};
                border-left: 4px solid {_hex(cols['accent'])};
                padding: 8px 14px;
                border-radius: 10px;
            }}
        """

        self.fuel_label = QLabel("FUEL: ----")
        self.fuel_label.setStyleSheet(self.style_sheet)
        layout.addWidget(self.fuel_label)

        self.delta_label = QLabel("DELTA: ----")
        self.delta_label.setStyleSheet(
            self.style_sheet.replace(_hex(lc['colors']['accent']),
                                     _hex(lc['colors']['accent2']))
        )
        layout.addWidget(self.delta_label)

        self.setLayout(layout)
        self.setGeometry(100, 100, 240, 100)

    def showEvent(self, event) -> None:  # noqa: N802 (Qt naming)
        super().showEvent(event)
        # winId() is only valid once the native window exists.
        if self.click_through:
            oc.enable_windows_click_through(self)

    def update_telemetry(self) -> None:
        if self.ir is None:
            self.fuel_label.setText("pyirsdk not installed")
            self.delta_label.setText("DELTA: --")
            return

        if not self.ir.is_connected:
            if not self.ir.startup():
                self.fuel_label.setText("iRacing Disconnected")
                self.delta_label.setText("DELTA: --")
                return

        # FuelLevel is in liters. Convert only if the user asked for gallons.
        fuel = self.ir["FuelLevel"]
        lap_delta = self.ir["LapDeltaToSessionBest"]

        if fuel is None:
            fuel_str = "FUEL: --"
        elif self.use_gallons or config.is_imperial():
            fuel_str = f"FUEL: {fuel / LITERS_PER_GALLON:.2f} G"
        else:
            fuel_str = f"FUEL: {fuel:.2f} L"

        delta_str = (
            f"DELTA: {lap_delta:+.3f}" if lap_delta is not None else "DELTA: --"
        )

        self.fuel_label.setText(fuel_str)
        self.delta_label.setText(delta_str)


def main() -> int:
    use_gallons = "--gallons" in sys.argv
    # Pass --no-clickthrough to make the window interactive (e.g. to drag it).
    click_through = "--no-clickthrough" not in sys.argv
    # Pass --demo to run with simulated telemetry (no iRacing required).
    demo = "--demo" in sys.argv

    app = QApplication(sys.argv)

    # Let Ctrl+C in the launching terminal actually quit the Qt event loop.
    signal.signal(signal.SIGINT, lambda *_: app.quit())
    keepalive = QTimer()
    keepalive.start(200)
    keepalive.timeout.connect(lambda: None)

    overlay = LightSpeedOverlay(
        use_gallons=use_gallons, click_through=click_through, demo=demo
    )
    overlay.show()
    return app.exec()


if __name__ == "__main__":
    sys.exit(main())
