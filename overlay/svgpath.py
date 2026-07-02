"""
Minimal SVG <path d="..."> flattener.

Turns an SVG path string into a flat list of (x, y) points so iRacing's
official track-map SVGs can be used as the overlay's track shape.

Supports M/m L/l H/h V/v C/c S/s Q/q T/t Z/z. Elliptical arcs (A/a) are
approximated as straight lines (iRacing track outlines don't use them).
"""

from __future__ import annotations

import re

_TOKEN_RE = re.compile(r"[MmLlHhVvCcSsQqTtAaZz]|-?\d*\.?\d+(?:[eE][-+]?\d+)?")


def _tokenize(d: str):
    return _TOKEN_RE.findall(d)


def _cubic(p0, p1, p2, p3, steps):
    out = []
    for i in range(1, steps + 1):
        t = i / steps
        mt = 1 - t
        x = (
            mt**3 * p0[0]
            + 3 * mt**2 * t * p1[0]
            + 3 * mt * t**2 * p2[0]
            + t**3 * p3[0]
        )
        y = (
            mt**3 * p0[1]
            + 3 * mt**2 * t * p1[1]
            + 3 * mt * t**2 * p2[1]
            + t**3 * p3[1]
        )
        out.append((x, y))
    return out


def _quad(p0, p1, p2, steps):
    out = []
    for i in range(1, steps + 1):
        t = i / steps
        mt = 1 - t
        x = mt**2 * p0[0] + 2 * mt * t * p1[0] + t**2 * p2[0]
        y = mt**2 * p0[1] + 2 * mt * t * p1[1] + t**2 * p2[1]
        out.append((x, y))
    return out


_SUBPATH_SPLIT_RE = re.compile(r"(?=[Mm])")


def split_subpaths(d: str, bezier_steps: int = 18) -> list[list[tuple[float, float]]]:
    """Flatten each M/m-started subpath in ``d`` separately.

    iRacing track outlines often pack the outer loop plus inner shading bands
    into one ``d`` attribute; flattening the whole string draws chords between
    unrelated subpaths.
    """
    parts = [p.strip() for p in _SUBPATH_SPLIT_RE.split(d.strip()) if p.strip()]
    if not parts:
        return []
    out: list[list[tuple[float, float]]] = []
    for part in parts:
        try:
            pts = flatten_path(part, bezier_steps=bezier_steps)
        except (ValueError, IndexError):
            pts = []
        if len(pts) >= 2:
            out.append(pts)
            continue
        moveto = re.match(
            r"^[Mm]\s*(-?\d*\.?\d+(?:[eE][-+]?\d+)?)"
            r"[,\s]+(-?\d*\.?\d+(?:[eE][-+]?\d+)?)",
            part,
        )
        if moveto:
            p = (float(moveto.group(1)), float(moveto.group(2)))
            out.append([p, (p[0] + 2.0, p[1])])
    return out


def flatten_path(d: str, bezier_steps: int = 18) -> list[tuple[float, float]]:
    tokens = _tokenize(d)
    i = 0
    cx = cy = 0.0
    sx = sy = 0.0
    pts: list[tuple[float, float]] = []
    cmd = None
    prev_cubic_ctrl = None
    prev_quad_ctrl = None

    def num():
        nonlocal i
        val = float(tokens[i])
        i += 1
        return val

    while i < len(tokens):
        tok = tokens[i]
        if re.match(r"[A-Za-z]", tok):
            cmd = tok
            i += 1
            if cmd in ("Z", "z"):
                pts.append((sx, sy))
                cx, cy = sx, sy
                prev_cubic_ctrl = prev_quad_ctrl = None
                continue
        rel = cmd.islower()

        if cmd in ("M", "m"):
            x, y = num(), num()
            cx, cy = (cx + x, cy + y) if rel else (x, y)
            sx, sy = cx, cy
            pts.append((cx, cy))
            cmd = "l" if cmd == "m" else "L"  # subsequent pairs are lineto
            prev_cubic_ctrl = prev_quad_ctrl = None
        elif cmd in ("L", "l"):
            x, y = num(), num()
            cx, cy = (cx + x, cy + y) if rel else (x, y)
            pts.append((cx, cy))
            prev_cubic_ctrl = prev_quad_ctrl = None
        elif cmd in ("H", "h"):
            x = num()
            cx = cx + x if rel else x
            pts.append((cx, cy))
            prev_cubic_ctrl = prev_quad_ctrl = None
        elif cmd in ("V", "v"):
            y = num()
            cy = cy + y if rel else y
            pts.append((cx, cy))
            prev_cubic_ctrl = prev_quad_ctrl = None
        elif cmd in ("C", "c"):
            x1, y1, x2, y2, x, y = (num() for _ in range(6))
            if rel:
                x1, y1, x2, y2, x, y = (
                    cx + x1, cy + y1, cx + x2, cy + y2, cx + x, cy + y
                )
            pts.extend(_cubic((cx, cy), (x1, y1), (x2, y2), (x, y), bezier_steps))
            prev_cubic_ctrl = (x2, y2)
            prev_quad_ctrl = None
            cx, cy = x, y
        elif cmd in ("S", "s"):
            x2, y2, x, y = (num() for _ in range(4))
            if rel:
                x2, y2, x, y = cx + x2, cy + y2, cx + x, cy + y
            if prev_cubic_ctrl:
                x1, y1 = 2 * cx - prev_cubic_ctrl[0], 2 * cy - prev_cubic_ctrl[1]
            else:
                x1, y1 = cx, cy
            pts.extend(_cubic((cx, cy), (x1, y1), (x2, y2), (x, y), bezier_steps))
            prev_cubic_ctrl = (x2, y2)
            prev_quad_ctrl = None
            cx, cy = x, y
        elif cmd in ("Q", "q"):
            x1, y1, x, y = (num() for _ in range(4))
            if rel:
                x1, y1, x, y = cx + x1, cy + y1, cx + x, cy + y
            pts.extend(_quad((cx, cy), (x1, y1), (x, y), bezier_steps))
            prev_quad_ctrl = (x1, y1)
            prev_cubic_ctrl = None
            cx, cy = x, y
        elif cmd in ("T", "t"):
            x, y = num(), num()
            if rel:
                x, y = cx + x, cy + y
            if prev_quad_ctrl:
                x1, y1 = 2 * cx - prev_quad_ctrl[0], 2 * cy - prev_quad_ctrl[1]
            else:
                x1, y1 = cx, cy
            pts.extend(_quad((cx, cy), (x1, y1), (x, y), bezier_steps))
            prev_quad_ctrl = (x1, y1)
            prev_cubic_ctrl = None
            cx, cy = x, y
        elif cmd in ("A", "a"):
            # rx ry rot large sweep x y -> approximate with a line to endpoint.
            _rx, _ry, _rot, _large, _sweep, x, y = (num() for _ in range(7))
            cx, cy = (cx + x, cy + y) if rel else (x, y)
            pts.append((cx, cy))
            prev_cubic_ctrl = prev_quad_ctrl = None
        else:
            i += 1  # unknown token, skip defensively

    return pts


def first_path_d(svg_text: str) -> str | None:
    """Return the 'd' attribute of the first <path> in an SVG document."""
    m = re.search(r"<path[^>]*\bd\s*=\s*\"([^\"]+)\"", svg_text, re.IGNORECASE)
    if m:
        return m.group(1)
    m = re.search(r"<path[^>]*\bd\s*=\s*'([^']+)'", svg_text, re.IGNORECASE)
    return m.group(1) if m else None
