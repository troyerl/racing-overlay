"""
Convert an iRacing track-outline SVG into a track JSON file for the overlay.

Usage:
    python3 svg_to_track.py <input.svg> <track_id> "<Track Name>" [out_dir]

This reads the first <path> from the SVG (iRacing's outline is drawn in driving
direction starting at the start/finish line), flattens it to points, and writes
tracks/<track_id>.json. Add corner labels by hand afterwards, e.g.:

    "corners": [{"pct": 0.07, "label": "1"}, {"pct": 0.15, "label": "Repsol"}]
"""

from __future__ import annotations

import json
import os
import sys

_ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
if _ROOT not in sys.path:
    sys.path.insert(0, _ROOT)

from overlay import svgpath


def main(argv: list[str]) -> int:
    if len(argv) < 4:
        print(__doc__)
        return 2

    svg_path, track_id, name = argv[1], argv[2], argv[3]
    out_dir = argv[4] if len(argv) > 4 else os.path.join(_ROOT, "tracks")

    with open(svg_path, "r", encoding="utf-8") as fh:
        d = svgpath.first_path_d(fh.read())
    if not d:
        print(f"No <path> element found in {svg_path}")
        return 1

    points = svgpath.flatten_path(d)
    if len(points) < 4:
        print("Path produced too few points; is this a track outline?")
        return 1

    os.makedirs(out_dir, exist_ok=True)
    out_path = os.path.join(out_dir, f"{track_id}.json")
    data = {
        "track_id": int(track_id) if track_id.isdigit() else track_id,
        "name": name,
        "start_finish": 0.0,
        "points": [[round(x, 3), round(y, 3)] for x, y in points],
        "corners": [],
    }
    with open(out_path, "w", encoding="utf-8") as fh:
        json.dump(data, fh, indent=2)
    print(f"Wrote {out_path} ({len(points)} points)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))
