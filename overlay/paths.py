"""Filesystem paths that work both in a dev checkout and a frozen Windows exe.

When packaged with PyInstaller, code and bundled assets live in a read-only
location (``sys._MEIPASS``) while user data (config, saved layout, learned
tracks) must be written somewhere persistent next to the executable. These
helpers paper over that difference so the rest of the app can stay oblivious.
"""

from __future__ import annotations

import os
import shutil
import sys

_REPO_ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))


def frozen() -> bool:
    return bool(getattr(sys, "frozen", False))


def resource_dir() -> str:
    """Where read-only bundled resources (assets/, bundled tracks/) live."""
    if frozen():
        return getattr(sys, "_MEIPASS", os.path.dirname(sys.executable))
    return _REPO_ROOT


def data_dir() -> str:
    """A writable directory for config, layout and learned tracks.

    For a packaged build this is a per-user folder (so it stays writable and
    survives reinstalls/updates even if the app is installed under Program
    Files). In a dev checkout it's the repo root, preserving existing behavior.
    """
    if frozen():
        base = os.environ.get("LOCALAPPDATA") or os.path.expanduser("~")
        d = os.path.join(base, "Racing Overlay")
    else:
        d = _REPO_ROOT
    os.makedirs(d, exist_ok=True)
    return d


def data_file(name: str) -> str:
    return os.path.join(data_dir(), name)


def resource_file(*parts: str) -> str:
    return os.path.join(resource_dir(), *parts)


def tracks_dir() -> str:
    """Writable tracks directory, seeded once from any bundled tracks."""
    dst = os.path.join(data_dir(), "tracks")
    os.makedirs(dst, exist_ok=True)
    src = os.path.join(resource_dir(), "tracks")
    if os.path.isdir(src) and os.path.abspath(src) != os.path.abspath(dst):
        for name in os.listdir(src):
            target = os.path.join(dst, name)
            if not os.path.exists(target):
                try:
                    shutil.copy2(os.path.join(src, name), target)
                except OSError:
                    pass
    return dst


def install_dir() -> str:
    """Directory the app is installed/running from (where the exe lives)."""
    if frozen():
        return os.path.dirname(sys.executable)
    return _REPO_ROOT


def uninstaller_path() -> str | None:
    """Path to the Inno Setup uninstaller for an installed Windows build.

    Inno Setup drops an ``unins###.exe`` next to the program. Returns it (the
    highest-numbered one if several exist) or None in a dev checkout / when no
    uninstaller is present, so callers can hide the in-app uninstall option.
    """
    if not frozen():
        return None
    base = install_dir()
    try:
        names = sorted(
            n for n in os.listdir(base)
            if n.lower().startswith("unins") and n.lower().endswith(".exe"))
    except OSError:
        return None
    return os.path.join(base, names[-1]) if names else None


def app_icon() -> str | None:
    """Path to the app icon (png preferred for Qt), or None if not bundled."""
    for name in ("assets/app.png", "assets/icon.png", "assets/app.ico"):
        path = resource_file(*name.split("/"))
        if os.path.exists(path):
            return path
    return None
