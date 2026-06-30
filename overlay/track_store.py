"""Shared track maps via MongoDB Atlas.

Track maps are expensive to learn (you have to drive a clean lap), so this lets
them be shared: the author uploads learned tracks and everyone else downloads
them on demand into their local ``tracks/`` cache.

Two roles, distinguished purely by which credential is available:

* Author / dev -- runs from source with the read-write connection string in the
  ``GRIDGLANCE_MONGODB_URI`` environment variable. Can upload.
* Everyone else -- the shipped app, which carries an embedded *read-only*
  connection string (``_READ_URI_B64`` below). Can only download.

All Mongo I/O is blocking, so callers use ``TrackSync`` to run it on a worker
thread and marshal results back to the GUI thread via Qt signals (same pattern
as ``overlay/updater.py``). ``pymongo`` is imported lazily so the app still runs
if the dependency is missing -- cloud features just stay disabled.
"""

from __future__ import annotations

import json
import logging
import os
import sys
import threading
from datetime import datetime, timezone

from PyQt6.QtCore import QObject, pyqtSignal

log = logging.getLogger("gridglance.tracks")


def _load_dotenv() -> None:
    """Populate ``os.environ`` from a local ``.env`` file as a dev convenience.

    Real environment variables always win; we only fill in keys that aren't
    already set. We look in the current working directory, the repo root, and
    next to a frozen executable so it works both from source and from a build.
    """
    here = os.path.dirname(os.path.abspath(__file__))
    candidates = [
        os.path.join(os.getcwd(), ".env"),
        os.path.join(os.path.dirname(here), ".env"),  # repo root (../ of overlay/)
    ]
    if getattr(sys, "frozen", False):
        candidates.append(os.path.join(os.path.dirname(sys.executable), ".env"))

    seen: set[str] = set()
    for path in candidates:
        if path in seen or not os.path.isfile(path):
            continue
        seen.add(path)
        try:
            with open(path, "r", encoding="utf-8") as fh:
                for raw in fh:
                    line = raw.strip()
                    if not line or line.startswith("#") or "=" not in line:
                        continue
                    key, _, val = line.partition("=")
                    key = key.strip()
                    if key.startswith("export "):
                        key = key[len("export "):].strip()
                    val = val.strip().strip('"').strip("'")
                    if key and key not in os.environ:
                        os.environ[key] = val
        except OSError:
            pass


_load_dotenv()

# ---------------------------------------------------------------------------
# Credentials
# ---------------------------------------------------------------------------
# WHERE TO ADD CREDENTIALS:
#
# 1. Read-only (shipped to everyone) -- the read URI is taken from the
#    GRIDGLANCE_MONGODB_READ_URI environment variable if it's set; otherwise it
#    falls back to the hard-coded default below. Paste your Atlas *read-only*
#    user's SRV URI here. It is NOT a real secret (anyone can extract it from
#    the binary); it just must be scoped to read-only on the gridglance
#    database. Leave it empty to disable cloud downloads entirely.
#
# 2. Read-write (author/dev only) -- do NOT put this in code. Set it in your
#    shell or a local .env (auto-loaded by _load_dotenv) so it never ships:
#
#      GRIDGLANCE_MONGODB_URI="mongodb+srv://gg_dev:PASSWORD@cluster.xxxxx.mongodb.net/"
#
#    This is the ONLY variable that unlocks the scan/record (write) controls.
#    GRIDGLANCE_MONGODB_READ_URI only redirects reads and never grants write
#    access, even if the string you put there happens to have write permission.
#    A single GRIDGLANCE_MONGODB_URI is enough for an author: it drives both
#    reads and writes.
#
_READ_URI_DEFAULT = "mongodb+srv://GridGlanceUser:3w69ejWh1WGKenQa@gridglance.dguyept.mongodb.net/?appName=GridGlance"

_DB_NAME = "gridglance"
_COLLECTION = "tracks"

# Fail fast and hold few connections: clients connect, fetch, and effectively
# idle, which matters against Atlas connection caps when many users share one
# read-only user.
_CLIENT_KW = dict(
    serverSelectionTimeoutMS=3000,
    connectTimeoutMS=3000,
    socketTimeoutMS=8000,
    maxPoolSize=2,
    appname="GridGlance",
)

_read_client = None
_write_client = None
_clients_lock = threading.Lock()


def _read_uri() -> str:
    """The URI used for reads.

    Prefers the dedicated read override, then the author's read-write URI (so a
    dev only needs to set one variable), then the hard-coded default.
    """
    for var in ("GRIDGLANCE_MONGODB_READ_URI", "GRIDGLANCE_MONGODB_URI"):
        val = (os.environ.get(var) or "").strip()
        if val:
            return val
    return _READ_URI_DEFAULT.strip()


def _write_uri() -> str:
    """The author's read-write URI, supplied via env only (never bundled)."""
    return (os.environ.get("GRIDGLANCE_MONGODB_URI") or "").strip()


def can_write() -> bool:
    """True when a read-write credential is present (i.e. the author/dev)."""
    return bool(_write_uri())


def read_available() -> bool:
    """True when a read connection string is configured (read or write URI)."""
    return bool(_read_uri() or _write_uri())


def _mask_uri(uri: str) -> str:
    """Hide the password in a connection string for safe display/logging."""
    if not uri:
        return "(none)"
    try:
        scheme, rest = uri.split("://", 1)
        if "@" in rest:
            creds, host = rest.split("@", 1)
            user = creds.split(":", 1)[0]
            return f"{scheme}://{user}:***@{host}"
        return f"{scheme}://{rest}"
    except Exception:
        return "(set)"


def diagnose() -> dict:
    """Probe the sharing setup and return a structured status (for a CLI / log).

    Reports which URIs are configured and whether a live ping succeeds for the
    read and (if present) write connections, capturing the real error so a
    misconfigured credential or blocked Atlas IP is easy to spot.
    """
    info: dict = {
        "pymongo": False,
        "read_uri": _mask_uri(_read_uri()),
        "write_uri": _mask_uri(_write_uri()),
        "can_write": can_write(),
        "read_ping": None,
        "write_ping": None,
        "read_error": "",
        "write_error": "",
    }
    try:
        import pymongo  # noqa: F401
        info["pymongo"] = True
    except Exception as exc:
        info["read_error"] = f"pymongo import failed: {exc}"
        return info

    def _ping(write: bool):
        try:
            col = _collection(write=write)
            if col is None:
                return None, "no connection (URI unset?)"
            col.database.client.admin.command("ping")
            return True, ""
        except Exception as exc:
            return False, f"{type(exc).__name__}: {exc}"

    info["read_ping"], info["read_error"] = _ping(False)
    if _write_uri():
        info["write_ping"], info["write_error"] = _ping(True)
    return info


def _collection(write: bool):
    """A pymongo collection handle, or None if unconfigured / pymongo missing."""
    global _read_client, _write_client
    uri = _write_uri() if write else (_read_uri() or _write_uri())
    if not uri:
        return None
    try:
        from pymongo import MongoClient
    except Exception as exc:
        log.warning("pymongo not available (%s); install it with "
                    "'pip install pymongo[srv]'", exc)
        return None
    try:
        with _clients_lock:
            if write:
                if _write_client is None:
                    _write_client = MongoClient(uri, **_CLIENT_KW)
                client = _write_client
            else:
                if _read_client is None:
                    _read_client = MongoClient(uri, **_CLIENT_KW)
                client = _read_client
        return client[_DB_NAME][_COLLECTION]
    except Exception as exc:
        log.warning("could not create Mongo %s client: %s: %s",
                    "write" if write else "read", type(exc).__name__, exc)
        return None


def normalize(doc: dict) -> dict:
    """Coerce any of the writers' track dicts into one canonical shared shape."""
    out: dict = {
        "track_id": doc.get("track_id"),
        "name": doc.get("name") or "",
        "points": [[round(float(x), 9), round(float(y), 9)]
                   for x, y in (doc.get("points") or [])],
        "start_finish": float(doc.get("start_finish", 0.0) or 0.0),
        "corners": [{"pct": float(c["pct"]), "label": str(c["label"])}
                    for c in (doc.get("corners") or [])
                    if "pct" in c and "label" in c],
        "schema": 1,
    }
    for key in ("pit_span", "pit_speed", "source", "learned",
                "pit_in_pct", "pit_out_pct", "num_turns"):
        if doc.get(key) is not None:
            out[key] = doc[key]
    # The pit-lane geometry plus its entry/exit blend lines (open polylines).
    for key in ("pit_path", "pit_in", "pit_out"):
        seg = doc.get(key)
        if isinstance(seg, list) and len(seg) >= 2:
            out[key] = [[round(float(x), 9), round(float(y), 9)]
                        for x, y in seg]
    out["updated_at"] = datetime.now(timezone.utc).isoformat()
    return out


def fetch_track(track_id) -> dict | None:
    """Look up a single track by iRacing TrackID. None on miss / any error."""
    if track_id is None:
        return None
    col = _collection(write=False)
    if col is None:
        return None
    try:
        return col.find_one({"track_id": track_id}, {"_id": 0})
    except Exception:
        return None


def upload_doc(doc: dict) -> bool:
    """Upsert a track document (author only). False if not permitted / failed.

    The collection (``gridglance.tracks``) is created automatically by MongoDB
    on the first successful upsert. Failures are logged (not raised) so a
    misconfigured credential or blocked network is visible in the app console.
    """
    if not can_write():
        log.warning("track upload skipped: no read-write URI. Set "
                    "GRIDGLANCE_MONGODB_URI (NOT just GRIDGLANCE_MONGODB_READ_URI).")
        return False
    col = _collection(write=True)
    if col is None:
        return False
    try:
        clean = normalize(doc)
        if clean.get("track_id") is None or not clean.get("points"):
            log.warning("track upload skipped: missing track_id or points")
            return False
        col.update_one({"track_id": clean["track_id"]},
                       {"$set": clean}, upsert=True)
        log.info("uploaded track %s to %s.%s",
                 clean["track_id"], _DB_NAME, _COLLECTION)
        return True
    except Exception as exc:
        log.warning("track upload failed: %s: %s", type(exc).__name__, exc)
        return False


def load_local(tracks_dir: str, track_id) -> dict | None:
    """Read the on-disk JSON for a track id (used before uploading)."""
    if track_id is None:
        return None
    path = os.path.join(tracks_dir, f"{track_id}.json")
    try:
        with open(path, "r", encoding="utf-8") as fh:
            return json.load(fh)
    except (OSError, ValueError):
        return None


def write_local(tracks_dir: str, doc: dict) -> str | None:
    """Atomically cache a downloaded track to tracks/<id>.json (same schema)."""
    track_id = doc.get("track_id")
    if track_id is None or not doc.get("points"):
        return None
    os.makedirs(tracks_dir, exist_ok=True)
    path = os.path.join(tracks_dir, f"{track_id}.json")
    clean = {k: v for k, v in doc.items() if k != "_id"}
    tmp = f"{path}.tmp"
    with open(tmp, "w", encoding="utf-8") as fh:
        json.dump(clean, fh)
    os.replace(tmp, path)
    return path


def remote_manifest() -> dict | None:
    """Map of {track_id: updated_at} for every shared track (None on error).

    Only the id + timestamp are fetched (not the geometry), so the startup sync
    can cheaply decide which tracks actually changed before downloading them.
    """
    col = _collection(write=False)
    if col is None:
        return None
    try:
        out: dict = {}
        for d in col.find({}, {"_id": 0, "track_id": 1, "updated_at": 1}):
            tid = d.get("track_id")
            if tid is not None:
                out[tid] = d.get("updated_at") or ""
        return out
    except Exception:
        return None


# Cap on how many *downloaded* tracks the local cache keeps. Each map is only
# tens of KB, so this is generous; it just guarantees the cache can't grow
# without bound as you visit more and more tracks. Bundled/learned files (those
# without an "updated_at" stamp) don't count and are never evicted.
MAX_CACHED_TRACKS = 40


def _cloud_cache(tracks_dir: str) -> list[tuple]:
    """List (track_id, updated_at, path, mtime) for cloud-downloaded cache files.

    Only files carrying an ``updated_at`` (written by ``write_local`` from a
    cloud doc) qualify, so bundled maps and the author's own learned files are
    excluded -- they're never refreshed against the cloud nor evicted.
    """
    out: list[tuple] = []
    try:
        names = os.listdir(tracks_dir)
    except OSError:
        return out
    for name in names:
        if not name.endswith(".json"):
            continue
        path = os.path.join(tracks_dir, name)
        try:
            with open(path, "r", encoding="utf-8") as fh:
                doc = json.load(fh)
        except (OSError, ValueError):
            continue
        if not isinstance(doc, dict) or "updated_at" not in doc:
            continue
        try:
            mtime = os.path.getmtime(path)
        except OSError:
            mtime = 0.0
        out.append((doc.get("track_id"), doc.get("updated_at") or "", path, mtime))
    return out


def touch(tracks_dir: str, track_id) -> None:
    """Mark a cached track as just-used (bumps mtime for LRU eviction)."""
    path = os.path.join(tracks_dir, f"{track_id}.json")
    try:
        os.utime(path, None)
    except OSError:
        pass


def enforce_cache_limit(tracks_dir: str, max_tracks: int = MAX_CACHED_TRACKS,
                        protect=()) -> int:
    """Evict the least-recently-used downloaded tracks past the cap.

    Only cloud-downloaded files are candidates; ``protect`` ids (e.g. the track
    you're driving) are never removed. Returns how many files were deleted.
    """
    cache = _cloud_cache(tracks_dir)
    if len(cache) <= max_tracks:
        return 0
    keep = {str(p) for p in protect}
    cache.sort(key=lambda r: r[3])  # oldest (least recently used) first
    over = len(cache) - max_tracks
    removed = 0
    for tid, _ts, path, _mtime in cache:
        if over <= 0:
            break
        if str(tid) in keep:
            continue
        try:
            os.remove(path)
            removed += 1
            over -= 1
        except OSError:
            pass
    return removed


def sync_down(tracks_dir: str, force: bool = False) -> int:
    """Refresh *already-cached* tracks from the cloud; return how many changed.

    Only tracks you've previously downloaded are checked: if the cloud's
    ``updated_at`` differs from the cached copy it's re-downloaded. New tracks
    are not bulk-pulled here -- they're fetched on demand when you join them
    (which keeps the cache bounded). Transfers only the lightweight manifest
    when nothing changed.
    """
    cache = _cloud_cache(tracks_dir)
    if not cache:
        return 0
    manifest = remote_manifest()
    if manifest is None:
        return 0
    changed = 0
    for tid, local_ts, _path, _mtime in cache:
        remote_ts = manifest.get(tid)
        if remote_ts is None:
            continue
        if not force and remote_ts and local_ts == remote_ts:
            continue
        doc = fetch_track(tid)
        if doc and write_local(tracks_dir, doc):
            changed += 1
    return changed


class TrackSync(QObject):
    """Runs Mongo reads/writes on a worker thread; emits results to the GUI."""

    # (track_id, doc-or-None) -- a download attempt finished.
    fetched = pyqtSignal(object, object)
    # number of cached tracks refreshed by a startup sync.
    synced = pyqtSignal(int)

    def fetch_async(self, track_id) -> None:
        if not read_available():
            return
        threading.Thread(target=self._fetch, args=(track_id,),
                         daemon=True).start()

    def _fetch(self, track_id) -> None:
        doc = fetch_track(track_id)
        self.fetched.emit(track_id, doc)

    def sync_down_async(self, tracks_dir: str, force: bool = False) -> None:
        """Background: refresh the whole local cache from the cloud on startup."""
        if not read_available():
            return
        threading.Thread(target=self._sync_down,
                         args=(tracks_dir, force), daemon=True).start()

    def _sync_down(self, tracks_dir: str, force: bool) -> None:
        changed = sync_down(tracks_dir, force)
        enforce_cache_limit(tracks_dir)
        self.synced.emit(changed)

    def upload_local_async(self, tracks_dir: str, track_id) -> None:
        """Author-only: push the on-disk track for this id up to the cloud."""
        if not can_write():
            return
        threading.Thread(target=self._upload_local,
                         args=(tracks_dir, track_id), daemon=True).start()

    def _upload_local(self, tracks_dir: str, track_id) -> None:
        doc = load_local(tracks_dir, track_id)
        if doc:
            upload_doc(doc)
