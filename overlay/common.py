"""Shared helpers for the LightSpeed overlays.

Keeps the iRacing SDK import guard and the Windows click-through logic in one
place so both the simple fuel HUD and the multi-widget HUD agree on behavior.
"""

from __future__ import annotations

import sys

from PyQt6.QtWidgets import QWidget

# pyirsdk is Windows-only and exposes the module name "irsdk" (NOT "pyirsdk").
try:
    import irsdk

    IRSDK_AVAILABLE = True
except ImportError:
    irsdk = None  # type: ignore[assignment]
    IRSDK_AVAILABLE = False


# --- iRacing telemetry enums (these are enums, not bitfields) ---------------

# irsdk_TrackSurface
TRK_NOT_IN_WORLD = -1
TRK_OFF_TRACK = 0
TRK_IN_PIT_STALL = 1
TRK_APPROACHING_PITS = 2
TRK_ON_TRACK = 3

# irsdk_CarLeftRight (the real "spotter" blind-spot variable is CarLeftRight)
LR_OFF = 0
LR_CLEAR = 1
LR_CAR_LEFT = 2
LR_CAR_RIGHT = 3
LR_CAR_LEFT_RIGHT = 4
LR_2_CARS_LEFT = 5
LR_2_CARS_RIGHT = 6

CAR_ON_LEFT = {LR_CAR_LEFT, LR_CAR_LEFT_RIGHT, LR_2_CARS_LEFT}
CAR_ON_RIGHT = {LR_CAR_RIGHT, LR_CAR_LEFT_RIGHT, LR_2_CARS_RIGHT}


def make_irsdk(demo: bool = False):
    """Return a telemetry source.

    With demo=True, returns a FakeIRSDK that simulates traffic so the overlays
    work without iRacing. Otherwise returns a started real IRSDK instance, or
    None if pyirsdk isn't installed.
    """
    if demo:
        from .demo_data import make_demo_irsdk

        return make_demo_irsdk()
    if not IRSDK_AVAILABLE:
        return None
    ir = irsdk.IRSDK()
    ir.startup()
    return ir


def enable_windows_click_through(widget: QWidget) -> None:
    """Apply the Win32 extended styles required for genuine click-through.

    Qt's WA_TransparentForMouseEvents alone is unreliable for frameless/
    translucent windows on Windows, so we also set WS_EX_LAYERED |
    WS_EX_TRANSPARENT. No-op off Windows. Call after the native window exists
    (e.g. in showEvent), since winId() isn't valid in __init__.
    """
    if sys.platform != "win32":
        return
    try:
        import ctypes

        GWL_EXSTYLE = -20
        WS_EX_LAYERED = 0x00080000
        WS_EX_TRANSPARENT = 0x00000020

        hwnd = int(widget.winId())
        user32 = ctypes.windll.user32
        current = user32.GetWindowLongW(hwnd, GWL_EXSTYLE)
        user32.SetWindowLongW(
            hwnd, GWL_EXSTYLE, current | WS_EX_LAYERED | WS_EX_TRANSPARENT
        )
    except Exception:
        pass
