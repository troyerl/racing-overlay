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
import urllib.error
import urllib.request

from PyQt6.QtCore import QObject, pyqtSignal

from . import version

_API = "https://api.github.com/repos/{repo}"
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


def _get_json(url: str, timeout: float):
    req = urllib.request.Request(url, headers=_HEADERS)
    with urllib.request.urlopen(req, timeout=timeout) as resp:
        return json.load(resp)


def _release_info(data: dict) -> dict:
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


def fetch_latest(timeout: float = 6.0) -> dict | None:
    """Return {version, url, notes, name} for the newest release, or None.

    Uses GitHub's /releases/latest endpoint, but that 404s when a repo has no
    *published, non-prerelease* release (only tags, drafts or pre-releases). In
    that case we fall back to the full release list and pick the newest one that
    isn't a draft, so pre-release builds are still offered. None means no
    release is available to compare against (not an error).
    """
    if not version.GITHUB_REPO:
        return None
    base = _API.format(repo=version.GITHUB_REPO)
    try:
        return _release_info(_get_json(base + "/releases/latest", timeout))
    except urllib.error.HTTPError as exc:
        if exc.code != 404:
            raise
    # No "latest" release -> look through all releases (newest first).
    releases = _get_json(base + "/releases", timeout)
    published = [r for r in releases if isinstance(r, dict) and not r.get("draft")]
    if not published:
        return None
    return _release_info(published[0])


def download(url: str, on_progress=None) -> str:
    """Download the installer to a temp file and return its path.

    If given, ``on_progress(bytes_done, bytes_total)`` is called as bytes
    arrive (bytes_total is 0 when the server doesn't send a Content-Length).
    """
    fd, path = tempfile.mkstemp(suffix=".exe", prefix="RacingOverlaySetup-")
    os.close(fd)
    req = urllib.request.Request(url, headers={"User-Agent": _HEADERS["User-Agent"]})
    with urllib.request.urlopen(req) as resp, open(path, "wb") as fh:
        total = int(resp.headers.get("Content-Length") or 0)
        done = 0
        while True:
            chunk = resp.read(65536)
            if not chunk:
                break
            fh.write(chunk)
            done += len(chunk)
            if on_progress is not None:
                on_progress(done, total)
    return path


class UpdateChecker(QObject):
    """Background update checker + installer downloader."""

    found = pyqtSignal(dict)        # a newer release is available
    up_to_date = pyqtSignal(str)    # already current; carries the version
    check_failed = pyqtSignal(str)  # the check itself failed / not configured
    downloaded = pyqtSignal(str)    # installer saved to this path
    progress = pyqtSignal(int, int)  # bytes_done, bytes_total (0 = unknown)
    failed = pyqtSignal(str)        # download error message

    def start(self) -> None:
        """Silent launch check (only speaks up when a newer release exists)."""
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

    def check_now(self) -> None:
        """User-initiated check that reports every outcome via signals."""
        threading.Thread(target=self._check_now, daemon=True).start()

    def _check_now(self) -> None:
        if not version.GITHUB_REPO:
            self.check_failed.emit(
                "This build isn't configured for automatic updates.")
            return
        try:
            info = fetch_latest()
        except Exception as exc:  # noqa: BLE001
            self.check_failed.emit(str(exc) or "Network error")
            return
        if info is None:
            self.check_failed.emit("No releases have been published yet.")
            return
        if not info.get("version"):
            self.check_failed.emit("Couldn't read the latest release from GitHub.")
            return
        if is_newer(info["version"], version.__version__):
            self.found.emit(info)
        else:
            self.up_to_date.emit(version.__version__)

    def download_async(self, url: str) -> None:
        threading.Thread(target=self._download, args=(url,), daemon=True).start()

    def _download(self, url: str) -> None:
        try:
            self.downloaded.emit(
                download(url, on_progress=lambda d, t: self.progress.emit(d, t)))
        except Exception as exc:  # noqa: BLE001
            self.failed.emit(str(exc))
