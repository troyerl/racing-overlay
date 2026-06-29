#!/usr/bin/env python3
"""Diagnose GridGlance's MongoDB sharing setup.

Prints whether sharing is enabled, which connection strings are configured,
and whether a live ping to Atlas succeeds for the read and read-write URIs --
surfacing the real error (auth, blocked IP, etc.) that the app otherwise logs
quietly. Optionally tries a real upload of one locally-scanned track.

    python3 tools/check_db.py                 # status only
    python3 tools/check_db.py --upload-test   # also push one local track

Credentials are read from the environment (or a local .env, auto-loaded):
  * GRIDGLANCE_MONGODB_URI       read-write (author) -- enables uploads
  * GRIDGLANCE_MONGODB_READ_URI  read-only override (optional)
"""

from __future__ import annotations

import argparse
import logging
import os
import sys

_ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
if _ROOT not in sys.path:
    sys.path.insert(0, _ROOT)

from overlay import config, paths, track_store  # noqa: E402


def _yn(v) -> str:
    return {True: "yes", False: "NO", None: "n/a"}[v]


def _local_ids() -> list:
    out = []
    try:
        names = os.listdir(_TRACKS)
    except OSError:
        return out
    for name in names:
        if name.endswith(".json") and not name.startswith("_"):
            stem = name[:-5]
            out.append(int(stem) if stem.lstrip("-").isdigit() else stem)
    return out


_TRACKS = paths.tracks_dir()


def main() -> int:
    ap = argparse.ArgumentParser(
        description=__doc__,
        formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--upload-test", action="store_true",
                    help="attempt to upload one local (non-demo) track")
    args = ap.parse_args()

    # Make the library's own warnings visible too.
    logging.basicConfig(level=logging.INFO, format="%(name)s: %(message)s")

    print("== GridGlance sharing diagnostics ==")
    print(f"  cloud sharing enabled (setting): {_yn(config.cloud_tracks())}")
    info = track_store.diagnose()
    print(f"  pymongo installed:               {_yn(info['pymongo'])}")
    print(f"  read URI:                        {info['read_uri']}")
    print(f"  write URI (GRIDGLANCE_MONGODB_URI): {info['write_uri']}")
    print(f"  can upload (write URI present):  {_yn(info['can_write'])}")
    print(f"  read  ping: {_yn(info['read_ping'])}"
          + (f"   -> {info['read_error']}" if info['read_error'] else ""))
    print(f"  write ping: {_yn(info['write_ping'])}"
          + (f"   -> {info['write_error']}" if info['write_error'] else ""))
    print(f"  tracks dir: {_TRACKS}")
    ids = _local_ids()
    print(f"  local track files: {ids if ids else '(none)'}")

    if not config.cloud_tracks():
        print("\n! Sharing is OFF in settings -- the app won't upload. Enable "
              "cloud tracks in the map settings.")
    if not info["can_write"]:
        print("\n! No write URI: uploads are disabled. Set GRIDGLANCE_MONGODB_URI "
              "to your read-write Atlas connection string (a read-write string "
              "in GRIDGLANCE_MONGODB_READ_URI does NOT enable uploads).")
    elif info["write_ping"] is False:
        print("\n! Write connection failed to ping -- see the error above "
              "(common causes: wrong password, or your IP isn't allowed under "
              "Atlas > Network Access).")

    if args.upload_test:
        print("\n-- upload test --")
        targets = [i for i in ids]
        if not targets:
            print("  no local tracks to upload (scan one first).")
            return 1
        tid = targets[0]
        doc = track_store.load_local(_TRACKS, tid)
        if not doc:
            print(f"  could not read local track {tid}")
            return 1
        ok = track_store.upload_doc(doc)
        print(f"  upload of track {tid}: {'OK' if ok else 'FAILED'}")
        if ok:
            print(f"  -> upserted into '{track_store._DB_NAME}."
                  f"{track_store._COLLECTION}' (created on first write).")
        return 0 if ok else 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
