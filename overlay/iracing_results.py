"""Optional iRacing /data/results helpers (env credentials only).

Used to resolve registration split number when WeekendInfo does not expose it.
Credentials are read from ``IRACING_USERNAME`` / ``IRACING_EMAIL`` and
``IRACING_PASSWORD``. Without credentials this module is a no-op.
"""

from __future__ import annotations

import hashlib
import http.cookiejar
import json
import os
import urllib.error
import urllib.parse
import urllib.request

_AUTH_URL = "https://members-ng.iracing.com/auth"
_RESULTS_URL = "https://members-ng.iracing.com/data/results/get"
_UA = "GridGlance/1.0"


def _credentials() -> tuple[str, str] | None:
    user = (os.environ.get("IRACING_USERNAME")
            or os.environ.get("IRACING_EMAIL") or "").strip()
    password = (os.environ.get("IRACING_PASSWORD") or "").strip()
    if not user or not password:
        return None
    return user, password


def _password_hash(email: str, password: str) -> str:
    # iRacing auth expects SHA256(password + email.lower()).
    raw = (password + email.lower()).encode("utf-8")
    return hashlib.sha256(raw).hexdigest()


def _opener() -> urllib.request.OpenerDirector:
    jar = http.cookiejar.CookieJar()
    return urllib.request.build_opener(urllib.request.HTTPCookieProcessor(jar))


def _follow_data_link(opener: urllib.request.OpenerDirector, url: str,
                      timeout: float = 12.0) -> dict | None:
    req = urllib.request.Request(url, headers={"User-Agent": _UA})
    try:
        with opener.open(req, timeout=timeout) as resp:
            payload = json.loads(resp.read().decode("utf-8"))
    except (OSError, urllib.error.URLError, ValueError, TypeError):
        return None
    if not isinstance(payload, dict):
        return None
    link = payload.get("link")
    if isinstance(link, str) and link:
        req2 = urllib.request.Request(link, headers={"User-Agent": _UA})
        try:
            with opener.open(req2, timeout=timeout) as resp2:
                data = json.loads(resp2.read().decode("utf-8"))
            return data if isinstance(data, dict) else None
        except (OSError, urllib.error.URLError, ValueError, TypeError):
            return None
    return payload


def _authenticate(opener: urllib.request.OpenerDirector,
                   email: str, password: str) -> bool:
    body = urllib.parse.urlencode({
        "email": email,
        "password": _password_hash(email, password),
    }).encode("utf-8")
    req = urllib.request.Request(
        _AUTH_URL, data=body, method="POST",
        headers={
            "User-Agent": _UA,
            "Content-Type": "application/x-www-form-urlencoded",
        })
    try:
        with opener.open(req, timeout=12.0) as resp:
            raw = resp.read().decode("utf-8")
        data = json.loads(raw) if raw else {}
        return bool(isinstance(data, dict) and data.get("authcode"))
    except (OSError, urllib.error.URLError, ValueError, TypeError):
        return False


def split_info_for_subsession(subsession_id: int) -> tuple[int, int] | None:
    """Return ``(1-based split index, total splits)`` ranked by SOF.

    Returns ``None`` when credentials are missing or the lookup fails.
    """
    creds = _credentials()
    if not creds:
        return None
    try:
        sid = int(subsession_id)
    except (TypeError, ValueError):
        return None
    if sid <= 0:
        return None
    email, password = creds
    opener = _opener()
    if not _authenticate(opener, email, password):
        return None
    url = f"{_RESULTS_URL}?{urllib.parse.urlencode({'subsession_id': sid})}"
    data = _follow_data_link(opener, url)
    if not data:
        return None
    return _split_info_from_results(data, sid)


def split_number_for_subsession(subsession_id: int) -> int | None:
    """Compatibility wrapper returning only the 1-based split index."""
    info = split_info_for_subsession(subsession_id)
    return info[0] if info else None


def _split_info_from_results(
        data: dict, subsession_id: int) -> tuple[int, int] | None:
    """Rank ``session_splits`` by event SOF; return index and total."""
    for key in ("session_splits", "sessionSplits"):
        splits = data.get(key)
        if isinstance(splits, list) and splits:
            break
    else:
        splits = None
    if not splits:
        # Single-split or unknown shape: treat as split 1 when ids match.
        own = data.get("subsession_id") or data.get("subsessionId")
        try:
            if own is not None and int(own) == int(subsession_id):
                return (1, 1)
        except (TypeError, ValueError):
            pass
        return None

    ranked: list[tuple[int, int]] = []
    for entry in splits:
        if not isinstance(entry, dict):
            continue
        ss = entry.get("subsession_id", entry.get("subsessionId"))
        sof = entry.get("event_strength_of_field",
                        entry.get("eventStrengthOfField", 0))
        try:
            ranked.append((int(ss), int(sof or 0)))
        except (TypeError, ValueError):
            continue
    if not ranked:
        return None
    ranked.sort(key=lambda t: (-t[1], t[0]))
    for i, (ss, _) in enumerate(ranked, start=1):
        if ss == int(subsession_id):
            return (i, len(ranked))
    return None


def _split_from_results(data: dict, subsession_id: int) -> int | None:
    """Compatibility helper used by existing callers/tests."""
    info = _split_info_from_results(data, subsession_id)
    return info[0] if info else None
