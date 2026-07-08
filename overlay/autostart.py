"""Windows Startup-folder shortcut for launch-at-login.

Creates or removes ``GridGlance.lnk`` under the current user's Startup folder.
Other platforms are no-ops (product is Windows / iRacing primary).
"""

from __future__ import annotations

import logging
import os
import sys

log = logging.getLogger(__name__)

_SHORTCUT_NAME = "GridGlance.lnk"


def _startup_dir() -> str | None:
    if not sys.platform.startswith("win"):
        return None
    appdata = os.environ.get("APPDATA")
    if not appdata:
        return None
    return os.path.join(
        appdata, "Microsoft", "Windows", "Start Menu", "Programs", "Startup")


def shortcut_path() -> str | None:
    """Full path to the Startup shortcut, or None when not on Windows."""
    d = _startup_dir()
    return os.path.join(d, _SHORTCUT_NAME) if d else None


def is_enabled() -> bool:
    """True if the Startup shortcut currently exists."""
    path = shortcut_path()
    return bool(path and os.path.isfile(path))


def _shortcut_arguments() -> str:
    """Args for the Startup shortcut.

    When overlay-on-launch is on, pass ``--no-settings`` so login does not
    force a settings window; overlay still starts via config.
    """
    from . import config
    if config.CFG.get("start_overlay_on_launch", False):
        return "--no-settings"
    return ""


def _create_windows_shortcut(path: str, target: str, arguments: str) -> None:
    """Write a .lnk via COM (pywin32 if present, else PowerShell)."""
    work_dir = os.path.dirname(target) or None
    try:
        import win32com.client  # type: ignore
        shell = win32com.client.Dispatch("WScript.Shell")
        link = shell.CreateShortCut(path)
        link.Targetpath = target
        link.Arguments = arguments
        if work_dir:
            link.WorkingDirectory = work_dir
        link.IconLocation = target
        link.save()
        return
    except Exception:  # noqa: BLE001
        pass

    # Fallback: PowerShell COM (no pywin32 required).
    import subprocess

    def _ps_quote(s: str) -> str:
        return "'" + s.replace("'", "''") + "'"

    ps = (
        "$ws = New-Object -ComObject WScript.Shell; "
        f"$s = $ws.CreateShortcut({_ps_quote(path)}); "
        f"$s.TargetPath = {_ps_quote(target)}; "
        f"$s.Arguments = {_ps_quote(arguments)}; "
    )
    if work_dir:
        ps += f"$s.WorkingDirectory = {_ps_quote(work_dir)}; "
    ps += f"$s.IconLocation = {_ps_quote(target)}; $s.Save()"
    subprocess.run(  # noqa: S603
        ["powershell", "-NoProfile", "-NonInteractive", "-Command", ps],
        check=True,
        capture_output=True,
        text=True,
    )


def set_enabled(on: bool) -> None:
    """Create or remove the Startup shortcut. No-op on non-Windows."""
    path = shortcut_path()
    if path is None:
        return
    if not on:
        try:
            if os.path.isfile(path):
                os.remove(path)
        except OSError as exc:
            log.warning("Could not remove Startup shortcut: %s", exc)
        return

    target = sys.executable
    arguments = _shortcut_arguments()
    try:
        os.makedirs(os.path.dirname(path), exist_ok=True)
        _create_windows_shortcut(path, target, arguments)
    except Exception as exc:  # noqa: BLE001
        log.warning("Could not create Startup shortcut: %s", exc)


def sync_from_preference(want: bool) -> None:
    """Ensure the filesystem shortcut matches the remembered preference."""
    if want != is_enabled():
        set_enabled(want)


def refresh_shortcut_if_enabled() -> None:
    """Rewrite an existing Startup shortcut (e.g. after overlay-on-launch flips)."""
    if is_enabled():
        set_enabled(True)
