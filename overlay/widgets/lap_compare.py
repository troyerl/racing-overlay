"""
Lap compare -- your current lap vs your best lap, corner by corner.

LapCompareEngine records throttle / brake / steering / speed against track
position for each lap. The first clean lap becomes your benchmark; every lap
after is compared to it. It auto-detects the turns (from where you brake and
steer on the benchmark) and, for each turn, works out how much time you lost and
*why* -- braking too early, carrying less apex speed, getting back to throttle
late, etc. -- so the widget can spell out what to fix.

iRacing streams only *your* inputs, so this compares you against yourself; it
can't see an opponent's pedals. The widget shows a live delta to your best, a
delta-over-distance trace, and a ranked list of the corners costing you time.
"""

from __future__ import annotations

import math
from collections import deque

from PyQt6.QtCore import QPointF, QRectF, Qt
from PyQt6.QtGui import QColor, QPainter, QPainterPath, QPen
from PyQt6.QtWidgets import QSizePolicy, QWidget

from .. import config, lap_compare_store
from .chrome import col, draw_card, draw_dark_cell, draw_edge_band
from .chrome import draw_row_divider, resolve_row_height
from .fonts import data_font_bold, tabfont, tfont
from .formats import clock, signed_delta

_SECTION = "lap_compare"

N_BINS = 240  # track-position resolution (~one sample per 0.4% of the lap)


def _ffill(arr: list):
    """Forward-then-back fill Nones so a sparse lap becomes a continuous trace."""
    out = list(arr)
    last = None
    for i, v in enumerate(out):
        if v is None:
            out[i] = last
        else:
            last = v
    nxt = None
    for i in range(len(out) - 1, -1, -1):
        if out[i] is None:
            out[i] = nxt
        else:
            nxt = out[i]
    return out


class LapCompareEngine:
    """Owns the per-lap recording, benchmark selection and corner analysis."""

    # thr/brk in 0..1, str in 0..1 (0.5 centered), spd m/s, t lap-seconds,
    # lat/lon accel m/s^2, gear int, rpm.
    CH = ("thr", "brk", "str", "spd", "t", "lat", "lon", "gear", "rpm")

    def __init__(self):
        self._cur = self._blank()
        self._cur_pit = False
        self._cur_dirty = False     # off-track or incident this lap
        self._lap_start_inc = None  # incident count at the lap's start
        self._cur_bin = 0
        self._filled = 0
        self._prev_pct = None
        self._ref = None
        self._ref_time = None
        self._turns: list[dict] = []
        self._analysis: list[dict] = []
        self._last_delta = None
        self._is_new_best = False
        self._lap_started = False  # crossed S/F this stint -> live delta is valid
        self._corner_pcts: list = []
        self._track_len = 0.0
        self._redline = 0.0
        self._ident = None          # car+track key for persistence
        self._laps = deque(maxlen=12)  # recent valid lap times (consistency)

    @staticmethod
    def _blank() -> dict:
        return {k: [None] * N_BINS for k in LapCompareEngine.CH}

    def _reset_cur(self) -> None:
        self._cur = self._blank()
        self._cur_pit = False
        self._cur_dirty = False
        self._lap_start_inc = None
        self._filled = 0

    def set_identity(self, key, redline=0.0) -> None:
        """Tell the engine which car+track it's recording for. Switching loads
        that combo's persisted benchmark lap (if any)."""
        if redline:
            self._redline = float(redline)
        if not key or key == self._ident:
            return
        self._ident = key
        self._ref = None
        self._ref_time = None
        self._turns = []
        self._analysis = []
        self._last_delta = None
        self._is_new_best = False
        self._laps.clear()
        self._reset_cur()
        self._lap_started = False
        self._prev_pct = None
        self._load_ref(key)

    # -- recording ----------------------------------------------------------
    def update(self, pct, on_pit, throttle, brake, steer, speed, laptime,
               last_lap_time, lat=None, lon=None, gear=None, rpm=None,
               off_track=False, incidents=None, corner_pcts=None,
               track_len=0.0) -> None:
        if corner_pcts:
            self._corner_pcts = corner_pcts
        if track_len:
            self._track_len = float(track_len)
        if not isinstance(pct, (int, float)) or pct < 0:
            return
        pct = min(0.999999, max(0.0, pct))

        # Lap rollover: a big backward jump in track position = crossed S/F.
        if self._prev_pct is not None and pct + 0.5 < self._prev_pct:
            self._finish_lap(last_lap_time)
            self._reset_cur()
            self._lap_started = True  # timing now runs from the start/finish line
            self._prev_pct = pct
        self._prev_pct = pct

        # Clean-lap tracking: off-track or a fresh incident makes the lap dirty.
        if on_pit:
            self._cur_pit = True
        if off_track:
            self._cur_dirty = True
        if isinstance(incidents, (int, float)):
            if self._lap_start_inc is None:
                self._lap_start_inc = incidents
            elif incidents > self._lap_start_inc:
                self._cur_dirty = True

        b = min(N_BINS - 1, int(pct * N_BINS))
        self._cur_bin = b
        store = {"thr": throttle, "brk": brake, "str": steer, "spd": speed,
                 "t": laptime, "lat": lat, "lon": lon, "gear": gear, "rpm": rpm}
        for k, v in store.items():
            if isinstance(v, (int, float)):
                if self._cur[k][b] is None and k == "t":
                    self._filled += 1
                self._cur[k][b] = v

    def _finish_lap(self, last_lap_time) -> None:
        valid = (self._filled > N_BINS * 0.7
                 and isinstance(last_lap_time, (int, float))
                 and last_lap_time > 0 and not self._cur_pit
                 and not self._cur_dirty)
        if not valid:
            self._is_new_best = False
            return
        self._laps.append(last_lap_time)  # clean lap -> consistency sample
        lap = {k: _ffill(self._cur[k]) for k in self.CH}
        if self._ref is None or last_lap_time < self._ref_time:
            self._ref = lap
            self._ref_time = last_lap_time
            self._turns = self._detect_turns(lap)
            self._analysis = []
            self._last_delta = 0.0
            self._is_new_best = self._ref_time is not None
            self._save_ref()
        else:
            self._analysis = self._analyze(lap, self._ref, self._turns)
            self._last_delta = last_lap_time - self._ref_time
            self._is_new_best = False

    # -- persistence (best lap per car+track) -------------------------------
    def _save_ref(self) -> None:
        if not self._ident or self._ref is None:
            return
        lap_compare_store.save(self._ident, {
            "ref_time": self._ref_time,
            "track_len": self._track_len,
            "lap": self._ref,
        })

    def _load_ref(self, key) -> None:
        entry = lap_compare_store.load(key)
        if not isinstance(entry, dict):
            return
        lap = entry.get("lap")
        if not isinstance(lap, dict) or "t" not in lap:
            return
        self._ref = {k: list(lap.get(k, [None] * N_BINS)) for k in self.CH}
        self._ref_time = entry.get("ref_time")
        if entry.get("track_len"):
            self._track_len = float(entry["track_len"])
        self._turns = self._detect_turns(self._ref)

    # -- turn detection (from the benchmark lap) ----------------------------
    def _detect_turns(self, lap: dict) -> list[dict]:
        steer, brake, spd = lap["str"], lap["brk"], lap["spd"]
        active = []
        for i in range(N_BINS):
            s = steer[i] if steer[i] is not None else 0.5
            bk = brake[i] if brake[i] is not None else 0.0
            active.append(abs(s - 0.5) > 0.06 or bk > 0.15)
        min_len = int(N_BINS * 0.015)
        runs = self._group(active, merge_gap=int(N_BINS * 0.02),
                           min_len=min_len)
        # Ovals: T1-T2 (and T3-T4) are one continuous-steer region, so split a
        # run wherever the speed trace shows two apexes with a chute between.
        split: list = []
        for s, e in runs:
            split.extend(self._split_run(s, e, spd, min_len))
        runs = split
        turns = []
        for idx, (s, e) in enumerate(runs, 1):
            seg = [spd[i] for i in range(s, e + 1) if spd[i] is not None]
            apex = s
            if seg:
                lo = min(seg)
                for i in range(s, e + 1):
                    if spd[i] is not None and spd[i] <= lo + 1e-6:
                        apex = i
                        break
            turns.append({"s": s, "e": e, "apex": apex,
                          "label": self._label(apex, idx)})
        return turns

    @staticmethod
    def _group(active: list, merge_gap: int, min_len: int) -> list:
        runs = []
        i = 0
        n = len(active)
        while i < n:
            if active[i]:
                j = i
                while j + 1 < n and active[j + 1]:
                    j += 1
                runs.append([i, j])
                i = j + 1
            else:
                i += 1
        # Merge runs separated by a short gap (one corner, brief steer unwind).
        merged = []
        for r in runs:
            if merged and r[0] - merged[-1][1] <= merge_gap:
                merged[-1][1] = r[1]
            else:
                merged.append(r)
        return [tuple(r) for r in merged if r[1] - r[0] >= min_len]

    @staticmethod
    def _split_run(s: int, e: int, spd: list, min_len: int,
                   prom: float = 0.03) -> list:
        """Split one cornering region into separate turns at speed valleys.

        Paired oval corners (T1-T2, T3-T4) read as a single steer-and-brake
        region because the wheel never unwinds between them. Their two apexes
        still show as two speed minima with a small peak (the chute) between, so
        we cut the run at any peak that rises >= `prom` above the lower apex.
        """
        v = [spd[i] for i in range(s, e + 1)]
        if len(v) < 3 or any(x is None for x in v):
            return [(s, e)]
        w = max(2, min_len // 2)
        # Local minima (valleys), de-duplicated across flat stretches.
        minima: list[int] = []
        for k in range(len(v)):
            lo, hi = max(0, k - w), min(len(v), k + w + 1)
            if v[k] <= min(v[lo:hi]) + 1e-9 and not (minima and k - minima[-1] <= w):
                minima.append(k)
        if len(minima) <= 1:
            return [(s, e)]
        # Cut at the speed peak between consecutive prominent valleys.
        cuts: list[int] = []
        for a, b in zip(minima, minima[1:]):
            peak_k = max(range(a, b + 1), key=lambda k: v[k])
            base = max(v[a], v[b])
            if v[peak_k] > 0 and (v[peak_k] - base) / v[peak_k] >= prom:
                cuts.append(peak_k)
        if not cuts:
            return [(s, e)]
        runs, start = [], 0
        for c in cuts:
            runs.append((s + start, s + c))
            start = c + 1
        runs.append((s + start, e))
        return [(a, b) for a, b in runs if b - a >= min_len] or [(s, e)]

    def _label(self, apex_bin: int, seq: int) -> str:
        apex_pct = apex_bin / N_BINS
        best = None
        for pct, lab in self._corner_pcts:
            d = abs(((apex_pct - pct + 0.5) % 1.0) - 0.5)
            if d < 0.03 and (best is None or d < best[0]):
                best = (d, lab)
        return f"T{best[1]}" if best else f"T{seq}"

    # -- analysis -----------------------------------------------------------
    def _analyze(self, cur: dict, ref: dict, turns: list) -> list[dict]:
        out = []
        for tn in turns:
            s, e, apex = tn["s"], tn["e"], tn["apex"]
            t_lost = ((cur["t"][e] - cur["t"][s]) - (ref["t"][e] - ref["t"][s]))
            tips = self._tips(cur, ref, s, e, apex)
            out.append({"label": tn["label"], "t_lost": t_lost, "tips": tips,
                        "order": s})
        out.sort(key=lambda d: d["t_lost"], reverse=True)
        return out

    def _tips(self, cur, ref, s, e, apex) -> list[str]:
        tips = []
        margin = int(N_BINS * 0.04)

        # Apex (minimum) speed through the corner.
        cur_seg = [cur["spd"][i] for i in range(s, e + 1) if cur["spd"][i] is not None]
        ref_seg = [ref["spd"][i] for i in range(s, e + 1) if ref["spd"][i] is not None]
        if cur_seg and ref_seg:
            dv = config.conv_speed(min(cur_seg)) - config.conv_speed(min(ref_seg))
            if dv <= -1.5:
                tips.append((3.0 + abs(dv), f"{abs(dv):.0f} {config.speed_unit().lower()}"
                                            " slower at apex"))
            elif dv >= 2.5:
                tips.append((0.5, f"{dv:.0f} {config.speed_unit().lower()} faster at apex"))

        # Braking point: first hard brake near corner entry.
        cb = self._first(cur["brk"], max(0, s - margin), e, 0.15)
        rb = self._first(ref["brk"], max(0, s - margin), e, 0.15)
        if cb is not None and rb is not None:
            d = cb - rb
            if d <= -3:
                tips.append((2.5, "braking too early" + self._dist(d)))
            elif d >= 3:
                tips.append((1.2, "braking later" + self._dist(d)))

        # Getting back to throttle after the apex.
        ct = self._first(cur["thr"], apex, min(N_BINS - 1, e + margin), 0.9)
        rt = self._first(ref["thr"], apex, min(N_BINS - 1, e + margin), 0.9)
        if ct is not None and rt is not None and ct - rt >= 3:
            tips.append((2.2, "back to throttle late" + self._dist(ct - rt)))

        # Brake pressure / trail braking.
        cpk = max((cur["brk"][i] for i in range(s, e + 1)
                   if cur["brk"][i] is not None), default=0.0)
        rpk = max((ref["brk"][i] for i in range(s, e + 1)
                   if ref["brk"][i] is not None), default=0.0)
        if cpk - rpk >= 0.12:
            tips.append((1.0, "over-braking"))
        elif rpk - cpk >= 0.12:
            tips.append((0.8, "could brake harder"))

        # Coasting: time on neither pedal -- the most common hidden time loss.
        cc = self._coast_time(cur, s, e)
        rc = self._coast_time(ref, s, e)
        if cc - rc >= 0.04:
            tips.append((3.5, f"coasting +{cc - rc:.2f}s"))

        # Trail braking: are you carrying brake from entry to the apex?
        ce = self._avg(cur["brk"], s, apex)
        re_ = self._avg(ref["brk"], s, apex)
        if re_ is not None and ce is not None and re_ - ce >= 0.08:
            tips.append((1.6, "trail-brake to the apex"))

        # Combined grip: peak cornering load vs your best lap, phrased plainly.
        cl = self._peak_abs(cur["lat"], s, e)
        rl = self._peak_abs(ref["lat"], s, e)
        if cl is not None and rl is not None and rl > 0 and rl - cl >= 1.5:
            pct = max(0, min(100, round(cl / rl * 100)))
            tips.append((1.9, f"too cautious here, push harder mid-corner "
                              f"({pct}% of best grip)"))

        # Minimum-speed point: slowing too early = braking too deep / early apex.
        cmin = self._min_idx(cur["spd"], s, e)
        if cmin is not None and cmin <= apex - 4:
            tips.append((1.5, "slowest point too early" + self._dist(cmin - apex)))

        # Steering smoothness: extra corrections scrub speed.
        crev = self._reversals(cur["str"], s, e)
        rrev = self._reversals(ref["str"], s, e)
        if crev - rrev >= 3:
            tips.append((1.3, f"jerky steering ({crev} corrections)"))

        # Shift quality: short-shifting on exit or bouncing off the limiter.
        cup = self._upshift(cur, s, min(N_BINS - 1, e + margin))
        rup = self._upshift(ref, s, min(N_BINS - 1, e + margin))
        if cup is not None and rup is not None and rup - cup >= 400:
            tips.append((1.4, f"short-shifting ({cup:.0f} vs {rup:.0f} rpm)"))
        elif self._redline and cup is None:
            mx = self._peak_abs(cur["rpm"], s, e)
            if mx is not None and mx >= self._redline * 0.99:
                tips.append((1.7, "bouncing off the limiter"))

        tips.sort(key=lambda t: t[0], reverse=True)
        return [t[1] for t in tips[:2]]

    @staticmethod
    def _first(arr, s, e, thresh):
        for i in range(s, e + 1):
            v = arr[i]
            if v is not None and v >= thresh:
                return i
        return None

    @staticmethod
    def _coast_time(lap, s, e) -> float:
        total = 0.0
        for i in range(s, e):
            thr, brk = lap["thr"][i], lap["brk"][i]
            t0, t1 = lap["t"][i], lap["t"][i + 1]
            if (None not in (thr, brk, t0, t1) and thr < 0.04 and brk < 0.04
                    and t1 >= t0):
                total += t1 - t0
        return total

    @staticmethod
    def _avg(arr, s, e):
        vals = [arr[i] for i in range(s, e + 1) if arr[i] is not None]
        return sum(vals) / len(vals) if vals else None

    @staticmethod
    def _peak_abs(arr, s, e):
        vals = [abs(arr[i]) for i in range(s, e + 1) if arr[i] is not None]
        return max(vals) if vals else None

    @staticmethod
    def _min_idx(arr, s, e):
        best = None
        for i in range(s, e + 1):
            if arr[i] is not None and (best is None or arr[i] < arr[best]):
                best = i
        return best

    @staticmethod
    def _reversals(arr, s, e) -> int:
        seq = [arr[i] for i in range(s, e + 1) if arr[i] is not None]
        cnt, prev = 0, None
        for j in range(1, len(seq)):
            d = seq[j] - seq[j - 1]
            if abs(d) < 0.004:
                continue
            sgn = 1 if d > 0 else -1
            if prev is not None and sgn != prev:
                cnt += 1
            prev = sgn
        return cnt

    @staticmethod
    def _upshift(lap, s, e):
        for i in range(s + 1, e + 1):
            g0, g1 = lap["gear"][i - 1], lap["gear"][i]
            if g0 is not None and g1 is not None and g1 > g0:
                return lap["rpm"][i - 1]
        return None

    def _dist(self, bins: int) -> str:
        if not self._track_len:
            return ""
        m = abs(bins) * self._track_len / N_BINS
        if config.is_imperial():
            return f" (~{m * 3.28084:.0f} ft)"
        return f" (~{m:.0f} m)"

    def seed_demo(self) -> None:
        """Populate a fake benchmark + analysis so the demo shows content
        immediately (real laps recompute it once you start driving)."""
        self._redline = 7400.0

        def lap(slow=0.0, shift=0, drop=0.0, coast=False):
            d = self._blank()
            for i in range(N_BINS):
                pct = i / N_BINS
                base = 0.5 + 0.5 * math.sin(pct * 2 * math.pi * 3 - 1.2)
                corner = max(0.0, 1.0 - base)
                bp = (i - shift) / N_BINS
                bc = max(0.0, 1.0 - (0.5 + 0.5 * math.sin(bp * 2 * math.pi * 3 - 1.2)))
                thr = max(0.0, min(1.0, 0.12 + base))
                brk = max(0.0, min(1.0, bc * 1.5 - 0.45))
                if coast and corner > 0.82:  # brief lift-and-coast into a corner
                    thr = 0.0
                    brk = max(0.0, brk - 0.4)
                d["thr"][i] = thr
                d["brk"][i] = brk
                d["str"][i] = 0.5 + (3.6 * corner * math.sin(pct * 2 * math.pi * 1.5)) / 10.0
                d["spd"][i] = (90.0 + 150.0 * base) / 3.6 - drop * corner / 3.6
                d["t"][i] = (92.0 + slow) * (i / N_BINS)
                d["lat"][i] = corner * (16.0 - drop * 0.5)  # m/s^2 cornering load
                d["lon"][i] = (thr - brk) * 9.0
                d["gear"][i] = max(1, min(6, int(2 + base * 4)))
                d["rpm"][i] = 5200.0 + base * 2200.0
            return {k: _ffill(d[k]) for k in self.CH}

        self._track_len = 4000.0
        self._ref = lap()
        self._ref_time = 92.0
        self._turns = self._detect_turns(self._ref)
        cur = lap(slow=0.6, shift=7, drop=8.0, coast=True)
        self._analysis = self._analyze(cur, self._ref, self._turns)
        self._last_delta = 0.6
        self._is_new_best = False
        self._laps.extend([92.0, 92.31, 92.62, 92.18, 92.74, 92.45])
        self._reset_cur()
        self._lap_started = False
        self._prev_pct = None

    # -- snapshot for the widget --------------------------------------------
    def snapshot(self) -> dict:
        live = None
        if self._ref is not None and self._lap_started:
            b = self._cur_bin
            ct = self._cur["t"][b]
            rt = self._ref["t"][b]
            if isinstance(ct, (int, float)) and isinstance(rt, (int, float)):
                live = ct - rt
        graph = None
        if self._ref is not None and self._lap_started:
            graph = []
            step = max(1, N_BINS // 120)
            upto = self._cur_bin if self._last_delta is None else N_BINS - 1
            for i in range(0, upto + 1, step):
                ct = self._cur["t"][i]
                rt = self._ref["t"][i]
                if isinstance(ct, (int, float)) and isinstance(rt, (int, float)):
                    graph.append((i / N_BINS, ct - rt))
        spread = None
        if len(self._laps) >= 3:
            spread = max(self._laps) - min(self._laps)
        return {
            "have_ref": self._ref is not None,
            "ref_time": self._ref_time,
            "live_delta": live,
            "last_delta": self._last_delta,
            "is_new_best": self._is_new_best,
            "turns": self._analysis,
            "graph": graph,
            "consistency": spread,
            "laps": len(self._laps),
        }


class LapCompareWidget(QWidget):
    def __init__(self, parent=None):
        super().__init__(parent)
        self.data: dict = {}
        self.setMinimumSize(280, 200)
        self.setSizePolicy(QSizePolicy.Policy.Expanding, QSizePolicy.Policy.Expanding)

    def set_data(self, data: dict) -> None:
        data = data or {}
        if data == self.data:
            return
        self.data = data
        self.update()

    def _cfg(self) -> dict:
        return config.CFG["lap_compare"]

    def _col(self, key: str) -> QColor:
        return col(key, _SECTION)

    def _delta_color(self, d) -> QColor:
        if not isinstance(d, (int, float)) or abs(d) < 0.005:
            return self._col("muted")
        return self._col("faster") if d < 0 else self._col("slower")

    def paintEvent(self, event) -> None:  # noqa: N802
        config.use_section(_SECTION)
        p = QPainter(self)
        p.setRenderHint(QPainter.RenderHint.Antialiasing)
        w, h = float(self.width()), float(self.height())
        c = self._cfg()
        d = self.data or {}
        card, radius = draw_card(p, w, h, _SECTION)
        data_bold = data_font_bold(_SECTION)

        pad = max(7.0, min(w, h) * 0.06)
        x0, iw = pad, w - 2 * pad
        y = pad

        if not d.get("have_ref"):
            p.setFont(tfont(h * 0.07))
            p.setPen(self._col("muted"))
            p.drawText(QRectF(pad, pad, iw, h - 2 * pad),
                       Qt.AlignmentFlag.AlignCenter | Qt.TextFlag.TextWordWrap,
                       "Drive a clean lap to set your benchmark, "
                       "then every lap is compared to it.")
            return

        hh = h * 0.12
        band = QRectF(card.left(), y, card.width(), hh)
        draw_edge_band(p, band, "header_bg", _SECTION, bottom_line=True,
                       radius_top=radius)
        p.setFont(tfont(hh * 0.62, bold=True))
        p.setPen(self._col("accent"))
        p.drawText(QRectF(x0, y, iw * 0.5, hh),
                   Qt.AlignmentFlag.AlignVCenter | Qt.AlignmentFlag.AlignLeft,
                   "VS BEST")
        p.setFont(tabfont(hh * 0.52, bold=data_bold))
        p.setPen(self._col("muted"))
        p.drawText(QRectF(x0 + iw * 0.4, y, iw * 0.6, hh),
                   Qt.AlignmentFlag.AlignVCenter | Qt.AlignmentFlag.AlignRight,
                   clock(d.get("ref_time")))
        y += hh

        live = d.get("live_delta")
        show_live = c.get("show_live_delta", True) and isinstance(live, (int, float))
        big = live if show_live else d.get("last_delta")
        bh = h * 0.20
        if isinstance(big, (int, float)):
            p.setFont(tabfont(bh * 0.84, bold=data_bold))
            p.setPen(self._delta_color(big))
            p.drawText(QRectF(x0, y, iw, bh), Qt.AlignmentFlag.AlignCenter,
                       signed_delta(big, 2))
        if d.get("is_new_best"):
            p.setFont(tfont(bh * 0.26))
            p.setPen(self._col("faster"))
            p.drawText(QRectF(x0, y + bh * 0.78, iw, bh * 0.3),
                       Qt.AlignmentFlag.AlignCenter, "NEW BEST LAP")
        y += bh

        if c.get("show_graph", True) and d.get("graph"):
            gh = h * 0.16
            self._draw_graph(p, QRectF(x0, y, iw, gh), d["graph"])
            y += gh + pad * 0.4

        foot = 0.0
        spread = d.get("consistency")
        if isinstance(spread, (int, float)):
            foot = h * 0.09
            fr_top = h - pad - foot
            band = QRectF(card.left(), fr_top, card.width(), foot + pad * 0.5)
            draw_edge_band(p, band, "footer_bg", _SECTION, top_line=True,
                           radius_bottom=radius, opaque=True)
            fr = QRectF(x0, fr_top, iw, foot)
            p.setFont(tfont(foot * 0.52, bold=False))
            p.setPen(self._col("muted"))
            p.drawText(fr, Qt.AlignmentFlag.AlignVCenter | Qt.AlignmentFlag.AlignLeft,
                       "CONSISTENCY")
            ccol = (self._col("faster") if spread <= 0.3
                    else self._col("slower") if spread >= 0.8 else self._col("text"))
            p.setFont(tabfont(foot * 0.56, bold=data_bold))
            p.setPen(ccol)
            p.drawText(fr, Qt.AlignmentFlag.AlignVCenter | Qt.AlignmentFlag.AlignRight,
                       f"\u00b1{spread / 2:.2f}s / {d.get('laps', 0)} laps")

        self._draw_turns(p, QRectF(x0, y, iw, h - pad - foot - y),
                         d.get("turns") or [], c, panel_h=h)

    def _draw_graph(self, p, rect: QRectF, pts) -> None:
        draw_dark_cell(p, rect, _SECTION, radius=5)
        mid = rect.center().y()
        p.setPen(QPen(self._col("grid"), 1))
        p.drawLine(QPointF(rect.left(), mid), QPointF(rect.right(), mid))
        if not pts:
            return
        peak = max(0.15, max(abs(v) for _x, v in pts))
        path = QPainterPath()
        for i, (fx, v) in enumerate(pts):
            x = rect.left() + fx * rect.width()
            yv = mid - (v / peak) * (rect.height() / 2 - 2)
            path.moveTo(x, yv) if i == 0 else path.lineTo(x, yv)
        p.setPen(QPen(self._col("graph_line"), 1.8))
        p.setBrush(Qt.BrushStyle.NoBrush)
        p.drawPath(path)

    def _draw_turns(self, p, rect: QRectF, turns, c, *, panel_h: float) -> None:
        thresh = float(c.get("min_time_loss", 0.03) or 0.0)
        shown = [t for t in turns if abs(t.get("t_lost", 0.0)) >= thresh] or turns
        # `turns` is worst-first, so slice keeps the costliest corners, then we
        # re-order them by track position (first corner at the top).
        shown = shown[:int(c.get("max_turns", 6) or 6)]
        shown = sorted(shown, key=lambda t: t.get("order", 0))
        if not shown:
            p.setFont(tfont(rect.height() * 0.12))
            p.setPen(self._col("muted"))
            p.drawText(rect, Qt.AlignmentFlag.AlignCenter,
                       "Matching your best lap -- nice and tidy.")
            return
        fixed_rh = float(c.get("row_height_px", 0) or 0)
        if fixed_rh > 0:
            rh = fixed_rh
        else:
            rh = resolve_row_height(body_h=rect.height(), row_count=len(shown),
                                    panel_h=panel_h, cfg=c)
        rh = max(rh, 1.0)
        y = rect.top()
        for i, t in enumerate(shown):
            row = QRectF(rect.left(), y, rect.width(), rh)
            if c.get("alt_row_shading", True) and i % 2 == 1:
                p.setPen(Qt.PenStyle.NoPen)
                p.setBrush(col("row_alt", _SECTION))
                p.drawRect(row)
            self._turn_row(p, row, t)
            if c.get("row_dividers", True) and i < len(shown) - 1:
                draw_row_divider(p, rect.left(), y + rh, rect.width(), _SECTION)
            y += rh

    def _turn_row(self, p, rect: QRectF, t) -> None:
        h = rect.height()
        lost = t.get("t_lost", 0.0)
        # Corner chip.
        chip_w = rect.width() * 0.16
        chip = QRectF(rect.left(), rect.top() + h * 0.12, chip_w, h * 0.76)
        draw_dark_cell(p, chip, _SECTION, radius=5)
        p.setFont(tfont(h * 0.32, bold=True))
        p.setPen(self._col("text"))
        p.drawText(chip, Qt.AlignmentFlag.AlignCenter, str(t.get("label", "")))
        dt_w = rect.width() * 0.20
        p.setFont(tabfont(h * 0.34, bold=data_font_bold(_SECTION)))
        p.setPen(self._delta_color(lost))
        p.drawText(QRectF(chip.right() + rect.width() * 0.02, rect.top(), dt_w, h),
                   Qt.AlignmentFlag.AlignVCenter | Qt.AlignmentFlag.AlignLeft,
                   signed_delta(lost, 2))
        tip = " · ".join(t.get("tips", [])) or "on pace"
        p.setFont(tfont(h * 0.26, bold=False))
        p.setPen(self._col("muted"))
        tx = chip.right() + rect.width() * 0.02 + dt_w + rect.width() * 0.02
        tip_rect = QRectF(tx, rect.top(), rect.right() - tx, h)
        p.save()
        p.setClipRect(tip_rect)
        p.drawText(tip_rect,
                   int(Qt.AlignmentFlag.AlignVCenter | Qt.AlignmentFlag.AlignLeft
                       | Qt.TextFlag.TextWordWrap), tip)
        p.restore()
