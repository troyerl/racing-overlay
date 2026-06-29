#!/usr/bin/env python3
"""
Fetch real iRacing track maps without the iRacing Data API.

The official iRacing Data API requires auth/token creation that often fails.
This script bypasses it entirely by pulling track outlines from a public,
community-maintained mirror that is already keyed by iRacing's *TrackID*:

    https://github.com/iTelemetry/iracing-tracks
        svgs/<TrackID>.svg      a single <path class="track-surface"> outline
        configs/<TrackID>.json  {"baseline": <start/finish lap pct>, "clockwise": ...}

Because the overlay already auto-loads ``tracks/<TrackID>.json`` (and ``.svg``)
by the live ``WeekendInfo.TrackID``, fetched files "just work" the next time you
join that track -- no API token, no GPS-learning lap required.

By default each SVG is flattened into the overlay's native JSON track schema
(``points`` + ``start_finish``) so there is no SVG parsing at runtime and the
start/finish line is aligned. Use ``--raw-svg`` to drop the original ``.svg``
instead (the overlay can parse it on the fly).

Examples
--------
    python3 fetch_tracks.py --list                 # show available TrackIDs
    python3 fetch_tracks.py --id 18                 # one track (Barcelona)
    python3 fetch_tracks.py --id 18 145 266         # several tracks
    python3 fetch_tracks.py --all                   # everything in the mirror
    python3 fetch_tracks.py --id 18 --raw-svg       # keep the original .svg
    python3 fetch_tracks.py --all --force           # re-download / overwrite

Find a track's TrackID by joining the session and reading WeekendInfo.TrackID,
or run the overlay once -- it prints the TrackID it could not find a file for.
"""

from __future__ import annotations

import argparse
import json
import os
import ssl
import sys
import urllib.error
import urllib.request

# Allow running this script directly (python3 tools/fetch_tracks.py) by putting
# the repo root on the path so the `overlay` package is importable.
_ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
if _ROOT not in sys.path:
    sys.path.insert(0, _ROOT)

from overlay import svgpath

_DEFAULT_TRACKS = os.path.join(_ROOT, "tracks")

REPO = "iTelemetry/iracing-tracks"
RAW = f"https://raw.githubusercontent.com/{REPO}/main"
API_CONTENTS = f"https://api.github.com/repos/{REPO}/contents"
_HEADERS = {"User-Agent": "gridglance-fetch-tracks"}


def _ssl_context() -> ssl.SSLContext:
    """A verifying SSL context that works even when the system CA bundle is
    missing (a common issue with python.org builds on macOS)."""
    try:
        import certifi
        return ssl.create_default_context(cafile=certifi.where())
    except Exception:  # noqa: BLE001 - certifi optional; fall back to system
        return ssl.create_default_context()


_SSL_CTX = _ssl_context()


def _get(url: str, *, binary: bool = False):
    req = urllib.request.Request(url, headers=_HEADERS)
    with urllib.request.urlopen(req, timeout=20, context=_SSL_CTX) as resp:
        data = resp.read()
    return data if binary else data.decode("utf-8")


def list_ids() -> list[int]:
    """Return the sorted TrackIDs available in the mirror."""
    entries = json.loads(_get(f"{API_CONTENTS}/svgs"))
    ids: list[int] = []
    for entry in entries:
        name = entry.get("name", "")
        if name.lower().endswith(".svg"):
            stem = name[:-4]
            if stem.isdigit():
                ids.append(int(stem))
    return sorted(ids)


def _baseline_for(track_id: int) -> float:
    """Start/finish lap percentage for a track (0.0 if unknown)."""
    try:
        cfg = json.loads(_get(f"{RAW}/configs/{track_id}.json"))
        return float(cfg.get("baseline", 0.0) or 0.0)
    except (urllib.error.HTTPError, urllib.error.URLError, ValueError):
        return 0.0


def fetch_one(track_id: int, out_dir: str, *, force: bool = False,
              raw_svg: bool = False) -> str:
    """Fetch a single track. Returns 'ok', 'skip', or an error string."""
    out_svg = os.path.join(out_dir, f"{track_id}.svg")
    out_json = os.path.join(out_dir, f"{track_id}.json")
    target = out_svg if raw_svg else out_json
    if os.path.exists(target) and not force:
        return "skip"

    try:
        svg_text = _get(f"{RAW}/svgs/{track_id}.svg")
    except urllib.error.HTTPError as exc:
        if exc.code == 404:
            return "missing (no SVG in mirror)"
        return f"http {exc.code}"
    except urllib.error.URLError as exc:
        return f"network error: {exc.reason}"

    if raw_svg:
        with open(out_svg, "w", encoding="utf-8") as fh:
            fh.write(svg_text)
        return "ok"

    d = svgpath.first_path_d(svg_text)
    if not d:
        return "no <path> in SVG"
    points = svgpath.flatten_path(d)
    if len(points) < 8:
        return "too few points"

    data = {
        "track_id": track_id,
        "name": f"Track {track_id}",
        "points": [[round(x, 1), round(y, 1)] for x, y in points],
        "start_finish": round(_baseline_for(track_id), 4),
        "source": REPO,
    }
    with open(out_json, "w", encoding="utf-8") as fh:
        json.dump(data, fh, separators=(",", ":"))
    return "ok"


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(
        description="Fetch real iRacing track maps (no Data API token needed).",
        epilog="Tracks are written keyed by iRacing TrackID so the overlay "
               "auto-loads them on the matching track.",
    )
    group = ap.add_mutually_exclusive_group(required=True)
    group.add_argument("--list", action="store_true",
                       help="list TrackIDs available in the mirror and exit")
    group.add_argument("--id", type=int, nargs="+", metavar="TRACKID",
                       help="one or more iRacing TrackIDs to fetch")
    group.add_argument("--all", action="store_true",
                       help="fetch every track in the mirror")
    ap.add_argument("--out", default=_DEFAULT_TRACKS, metavar="DIR",
                    help="output directory (default: the repo's tracks/)")
    ap.add_argument("--force", action="store_true",
                    help="overwrite files that already exist")
    ap.add_argument("--raw-svg", action="store_true",
                    help="save the original .svg instead of flattened JSON")
    args = ap.parse_args(argv)

    if args.list:
        try:
            ids = list_ids()
        except Exception as exc:  # noqa: BLE001 - surface any fetch failure
            print(f"Failed to list tracks: {exc}", file=sys.stderr)
            return 1
        print(f"{len(ids)} tracks available in {REPO}:")
        print("  " + " ".join(str(i) for i in ids))
        return 0

    os.makedirs(args.out, exist_ok=True)

    if args.all:
        try:
            track_ids = list_ids()
        except Exception as exc:  # noqa: BLE001
            print(f"Failed to list tracks: {exc}", file=sys.stderr)
            return 1
        print(f"Fetching {len(track_ids)} tracks into {args.out}/ ...")
    else:
        track_ids = args.id

    ok = skipped = failed = 0
    for tid in track_ids:
        result = fetch_one(tid, args.out, force=args.force, raw_svg=args.raw_svg)
        if result == "ok":
            ok += 1
            print(f"  [ok]   {tid}")
        elif result == "skip":
            skipped += 1
            print(f"  [skip] {tid} (already exists; use --force to overwrite)")
        else:
            failed += 1
            print(f"  [fail] {tid}: {result}")

    print(f"\nDone: {ok} fetched, {skipped} skipped, {failed} failed "
          f"-> {os.path.abspath(args.out)}")
    return 1 if failed and not ok else 0


if __name__ == "__main__":
    raise SystemExit(main())
