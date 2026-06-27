"""
Record a track shape from a live iRacing lap and save it as a track JSON file.

Run this with iRacing running and on track, then drive one clean lap. It samples
your car's GPS (Lat/Lon) by lap distance percentage and, once ~90% of the lap is
covered, writes tracks/<TrackID>.json keyed by the session's TrackID so the
overlay loads the accurate shape automatically next time.

Usage:
    python3 record_track.py
"""

from __future__ import annotations

import json
import os
import sys
import time

_ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
if _ROOT not in sys.path:
    sys.path.insert(0, _ROOT)

from overlay import common as oc
from overlay.widgets import track_map


def main() -> int:
    ir = oc.make_irsdk()
    if ir is None:
        print("pyirsdk not installed (Windows + iRacing required).")
        return 1

    print("Waiting for iRacing... drive one clean lap.")
    builder = track_map.TrackPathBuilder()
    track_id = None
    name = ""

    while not builder.ready:
        if not ir.is_connected and not ir.startup():
            time.sleep(0.5)
            continue
        weekend = ir["WeekendInfo"]
        if weekend:
            track_id = weekend.get("TrackID", track_id)
            name = weekend.get("TrackDisplayName", name)
        player = ir["PlayerCarIdx"]
        lap_pct = ir["CarIdxLapDistPct"]
        if player is not None and lap_pct:
            builder.add(lap_pct[player], ir["Lat"], ir["Lon"])
        time.sleep(1 / 30)

    tracks_dir = os.path.join(_ROOT, "tracks")
    os.makedirs(tracks_dir, exist_ok=True)
    out_path = os.path.join(tracks_dir, f"{track_id}.json")
    data = {
        "track_id": track_id,
        "name": name,
        "start_finish": 0.0,
        "points": [[round(x, 6), round(y, 6)] for x, y in builder.path],
        "corners": [],
    }
    with open(out_path, "w", encoding="utf-8") as fh:
        json.dump(data, fh, indent=2)
    print(f"Saved {out_path} for '{name}' (TrackID {track_id}).")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
