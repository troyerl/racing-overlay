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


_APP_DIR_NAME = "Racing Overlay"
# Files that used to live next to the code; migrated once into the user folder.
_LEGACY_FILES = ("overlay_config.json", "overlay_layout.json",
                 "lap_compare_best.json")
_migrated = False


def _user_base() -> str:
    """Platform-appropriate per-user data root."""
    if sys.platform.startswith("win"):
        return os.environ.get("LOCALAPPDATA") or os.path.expanduser("~")
    if sys.platform == "darwin":
        return os.path.join(os.path.expanduser("~"), "Library",
                            "Application Support")
    return (os.environ.get("XDG_DATA_HOME")
            or os.path.join(os.path.expanduser("~"), ".local", "share"))


def data_dir() -> str:
    """A writable directory for config, layout and learned tracks.

    Always a per-user folder (e.g. ``%LOCALAPPDATA%/Racing Overlay`` on Windows,
    ``~/Library/Application Support/Racing Overlay`` on macOS) so settings live
    outside the code tree and survive app updates/reinstalls. Settings that used
    to sit next to the code are migrated here once.
    """
    d = os.path.join(_user_base(), _APP_DIR_NAME)
    os.makedirs(d, exist_ok=True)
    _migrate_legacy(d)
    return d


def _migrate_legacy(dst: str) -> None:
    """Copy any pre-existing settings from the old code-relative location into
    the per-user folder, once, without clobbering files already there."""
    global _migrated
    if _migrated:
        return
    _migrated = True
    if os.path.abspath(_REPO_ROOT) == os.path.abspath(dst):
        return
    for name in _LEGACY_FILES:
        old = os.path.join(_REPO_ROOT, name)
        new = os.path.join(dst, name)
        if os.path.exists(old) and not os.path.exists(new):
            try:
                shutil.copy2(old, new)
            except OSError:
                pass


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
