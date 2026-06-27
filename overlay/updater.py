"""In-app update check against GitHub Releases.

On launch (for a packaged build with a known repo) we ask GitHub for the latest
release. If it's newer than the running version we raise ``found`` so the app can
offer to download and run the installer. All network work happens off the GUI
thread; Qt signals marshal the result back.
"""

from __future__ import annotations

import json
import os
import re
import tempfile
import threading
import urllib.request

from PyQt6.QtCore import QObject, pyqtSignal

from . import version

_API = "https://api.github.com/repos/{repo}/releases/latest"
_HEADERS = {
    "Accept": "application/vnd.github+json",
    "User-Agent": "RacingOverlay-Updater",
}


def _parse(v: str) -> tuple:
    return tuple(int(n) for n in re.findall(r"\d+", v or ""))


def is_newer(remote: str, current: str) -> bool:
    r, c = _parse(remote), _parse(current)
    if r and c:
        return r > c
    return bool(remote) and remote != current


def fetch_latest(timeout: float = 6.0) -> dict | None:
    """Return {version, url, notes, name} for the latest release, or None."""
    if not version.GITHUB_REPO:
        return None
    url = _API.format(repo=version.GITHUB_REPO)
    req = urllib.request.Request(url, headers=_HEADERS)
    with urllib.request.urlopen(req, timeout=timeout) as resp:
        data = json.load(resp)
    tag = (data.get("tag_name") or "").lstrip("vV")
    asset_url = None
    for a in data.get("assets", []):
        if str(a.get("name", "")).lower().endswith(".exe"):
            asset_url = a.get("browser_download_url")
            break
    return {
        "version": tag,
        "url": asset_url,
        "notes": data.get("body", "") or "",
        "name": data.get("name", "") or "",
    }


def download(url: str) -> str:
    """Download the installer to a temp file and return its path."""
    fd, path = tempfile.mkstemp(suffix=".exe", prefix="RacingOverlaySetup-")
    os.close(fd)
    req = urllib.request.Request(url, headers={"User-Agent": _HEADERS["User-Agent"]})
    with urllib.request.urlopen(req) as resp, open(path, "wb") as fh:
        while True:
            chunk = resp.read(65536)
            if not chunk:
                break
            fh.write(chunk)
    return path


class UpdateChecker(QObject):
    """Background update checker + installer downloader."""

    found = pyqtSignal(dict)       # a newer release is available
    downloaded = pyqtSignal(str)   # installer saved to this path
    failed = pyqtSignal(str)       # download error message

    def start(self) -> None:
        """Check for a newer release (no-op if no repo is configured)."""
        if not version.GITHUB_REPO:
            return
        threading.Thread(target=self._check, daemon=True).start()

    def _check(self) -> None:
        try:
            info = fetch_latest()
        except Exception:
            return
        if info and info.get("version") and is_newer(info["version"], version.__version__):
            self.found.emit(info)

    def download_async(self, url: str) -> None:
        threading.Thread(target=self._download, args=(url,), daemon=True).start()

    def _download(self, url: str) -> None:
        try:
            self.downloaded.emit(download(url))
        except Exception as exc:  # noqa: BLE001
            self.failed.emit(str(exc))
