#!/usr/bin/env python3
"""Bulk-sync the local ``tracks/`` folder with the shared MongoDB library.

Two directions:

    python3 tools/sync_tracks.py --download          # pull every cloud track
    python3 tools/sync_tracks.py --upload             # push every local track
    python3 tools/sync_tracks.py --upload --id 18 266 # push only these TrackIDs

Credentials (see overlay/track_store.py for the full notes):

* Download uses the read URI (the embedded one, or GRIDGLANCE_MONGODB_READ_URI).
* Upload requires the read-write URI in GRIDGLANCE_MONGODB_URI; without it the
  script refuses to upload.

Existing local files are skipped on download unless ``--force`` is given.
"""

from __future__ import annotations

import argparse
import os
import sys

# Allow running directly (python3 tools/sync_tracks.py) by putting the repo root
# on the path so the `overlay` package is importable.
_ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
if _ROOT not in sys.path:
    sys.path.insert(0, _ROOT)

from overlay import paths, track_store  # noqa: E402

# Use the same per-user tracks directory the app reads/writes (e.g.
# %LOCALAPPDATA%/GridGlance/tracks), so --upload finds maps you scanned in-app.
_TRACKS = paths.tracks_dir()


def _local_ids() -> list:
    out = []
    for name in os.listdir(_TRACKS):
        if name.endswith(".json"):
            stem = name[:-5]
            out.append(int(stem) if stem.lstrip("-").isdigit() else stem)
    return out


def download(force: bool) -> int:
    col = track_store._collection(write=False)
    if col is None:
        print("No read connection configured (set the read URI). Aborting.")
        return 1
    os.makedirs(_TRACKS, exist_ok=True)
    n = 0
    for doc in col.find({}, {"_id": 0}):
        tid = doc.get("track_id")
        if tid is None:
            continue
        path = os.path.join(_TRACKS, f"{tid}.json")
        if os.path.exists(path) and not force:
            print(f"  skip {tid} (exists; use --force)")
            continue
        if track_store.write_local(_TRACKS, doc):
            n += 1
            print(f"  saved {tid} -> {os.path.basename(path)}")
    print(f"Downloaded {n} track(s).")
    return 0


def upload(ids: list | None) -> int:
    if not track_store.can_write():
        print("No read-write URI (set GRIDGLANCE_MONGODB_URI). Aborting.")
        return 1
    targets = ids if ids else _local_ids()
    n = 0
    for tid in targets:
        doc = track_store.load_local(_TRACKS, tid)
        if not doc:
            print(f"  miss {tid} (no local file)")
            continue
        if track_store.upload_doc(doc):
            n += 1
            print(f"  pushed {tid}")
        else:
            print(f"  fail {tid}")
    print(f"Uploaded {n} track(s).")
    return 0


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    g = ap.add_mutually_exclusive_group(required=True)
    g.add_argument("--download", action="store_true",
                   help="pull every track from the cloud into tracks/")
    g.add_argument("--upload", action="store_true",
                   help="push local tracks up to the cloud (needs write URI)")
    ap.add_argument("--id", nargs="+", type=int, default=None,
                    help="restrict --upload to these TrackIDs")
    ap.add_argument("--force", action="store_true",
                    help="overwrite existing local files on --download")
    args = ap.parse_args()
    if args.download:
        return download(args.force)
    return upload(args.id)


if __name__ == "__main__":
    raise SystemExit(main())
