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
import os
import threading
from datetime import datetime, timezone

from PyQt6.QtCore import QObject, pyqtSignal

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
#    shell/.env so it never ships:
#
#      export GRIDGLANCE_MONGODB_URI="mongodb+srv://gg_dev:PASSWORD@cluster.xxxxx.mongodb.net/"
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
    """The read-only URI: the env var if set, else the hard-coded default."""
    env = os.environ.get("GRIDGLANCE_MONGODB_READ_URI")
    if env and env.strip():
        return env.strip()
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


def _collection(write: bool):
    """A pymongo collection handle, or None if unconfigured / pymongo missing."""
    global _read_client, _write_client
    uri = _write_uri() if write else (_read_uri() or _write_uri())
    if not uri:
        return None
    try:
        from pymongo import MongoClient
    except Exception:
        return None
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
    for key in ("pit_span", "pit_speed", "source", "learned"):
        if doc.get(key) is not None:
            out[key] = doc[key]
    if isinstance(doc.get("pit_path"), list) and len(doc["pit_path"]) >= 2:
        out["pit_path"] = [[round(float(x), 9), round(float(y), 9)]
                           for x, y in doc["pit_path"]]
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
    """Upsert a track document (author only). False if not permitted / failed."""
    if not can_write():
        return False
    col = _collection(write=True)
    if col is None:
        return False
    try:
        clean = normalize(doc)
        if clean.get("track_id") is None or not clean.get("points"):
            return False
        col.update_one({"track_id": clean["track_id"]},
                       {"$set": clean}, upsert=True)
        return True
    except Exception:
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
