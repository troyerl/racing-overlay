# Map wiki (GridGlance)

Living notes for map widget work. **Update this file whenever you change map motion, rendering, track loading, or labels** so the next pass does not repeat dead ends.

## How to verify you are on a new build

1. Fully quit GridGlance (system tray icon too). Single-instance will otherwise activate the **old** process and exit the new `cargo run`.
2. Release builds use `windows_subsystem = "windows"` — `eprintln!` is invisible unless `--perf` attaches a console (see below) or you use a debug build.
3. With `--perf`, look for `map_motion_rev=N` in the perf line. If that number did not bump, you are not running the new binary.

```powershell
# Quit tray instance first, then:
cargo run --release -p gridglance-overlay -- --settings --perf
```

Agent/sandbox note: Cursor agent shells may set `CARGO_TARGET_DIR` to a temp path. Builds intended for the user must write to `target/release/`.

---

## Current architecture (hot path)

| Piece | Role |
|--------|------|
| `ensure_path_cached` | Load track JSON / trigger cloud fetch |
| `tick_car_motion` → `advance_car_pcts` | Coast `pct+=vel*dt`; soft pct+vel sync on telem Δ (rev 27) |
| `MapPaintMode::StaticOnly` | Track chrome only, captured into the CPU bg cache |
| `SkiaPanelHost::paint_map_hot` | Seed cached `Canvas` from that bg, then `MapPaintMode::DynamicOnly` |
| `MapPaintMode::Full` | Edit / authoring path — track and cars in one pass |

Live (non-edit) reuses one `Canvas` across frames: `Canvas::write_bgra` seeds it
with the cached static track, Skia paints only the dynamic layer over the top,
and `Canvas::read_bgra_into` reads back into a persistent scratch buffer. Both
exist to keep the hot path allocation-free.

The hot path **is** the real renderer, not a reduced one. It used to be a CPU
blitter (`layered::composite_map_cars` + `build_car_sprites`), which is why dots
lost their labels, marker rings, player ring and status badges the moment a
track finished loading. It also de-duplicated stacked cars, so dots blinked in
and out whenever two cars closed up. Anything drawn in `Full` must also appear in
`DynamicOnly`; the only difference between them is which elements are static.

---

## Track cache / wrong map

| Date | What happened | Outcome |
|------|----------------|---------|
| 2026-07-21 | Local track JSONs cleared so Mongo would re-fetch | Cache nearly empty; only `166.json` (Okayama) remained |
| 2026-07-21 | User saw wrong / missing map | Expected — `sync_down` only **refreshes existing** files |
| 2026-07-21 | Added `cloud::sync_library` + `--sync-tracks`; sparse cache auto-pulls full library | Restored **407** tracks under `%LOCALAPPDATA%\GridGlance\tracks` |

**Do not wipe the tracks dir** without running `--sync-tracks` (or waiting for sparse auto-pull) afterward.

---

## Members-SVG import — markup varies per export

`start_finish: 0.0` is the *correct* stored value. The offset is not a field —
`align_loop_from_sf` rotates the point list so index 0 **is** the S/F line, then
orients it to the members direction arrow. So a track whose dots read "ahead"
with `sf=0.000` is not uncalibrated; alignment silently failed at import.

Charlotte Roval (554) exposed three parser gaps, all from markup the S/F and
turns layers are allowed to use but the parsers did not accept:

| Layer markup | Was | Effect |
|---|---|---|
| stripe `<rect>`, arrow `<polygon>` | only `<path d>` was read | no stripe *and* no arrow → loop rotated to bottom-most point (~11 % of a lap early) and direction fell back to `ensure_ccw` (**backwards** here) |
| labels wrapped in `<tspan>` | regex needed bare text before `</text>` | 0 of 15 corners parsed |
| arrow heading | ranked arrow vertices by distance from the *stripe* | picks a barb when the arrow sits beside the stripe rather than along it |

Rules for this pipeline:

- Accept `<path d>`, `<rect>`, `<polygon>`, `<polyline>` in the S/F layer. Sort
  by bbox area — stripe is the small shape, arrow the large one.
- Do not emit a repeated closing vertex for a `<rect>`; `sf_stripe_centroid`
  averages vertices and a 5th point drags the centre off the stripe.
- Arrow heading = vertex mean → farthest vertex. The dart's tail/barb cluster
  drags the mean back, so the farthest vertex is the tip.
- Turn labels may split across `<tspan>`s ("11" is usually two). Strip tags and
  rejoin instead of reading bare text.
- Only flip label numbering when it actually counts *down* the lap
  (`labels_run_backwards`). Turn count cannot distinguish a 4-turn oval from a
  4-turn road course. Synthesized `oval_corners` keep the old unconditional flip.

- The turns layer may also hold corner *names* ("Coca-Cola Corner", "Yokohama
  Bridge") as `<text>` siblings of the numbers. `is_turn_label` keeps only turn
  designations (`7`, `12`, `6a`, `7/8`); without it Oran Park imported 28 turns.

Verify an import by geometry, never by eye: point 0 must land on the stripe
(Charlotte: `x_norm≈0.50`, not the path's own first vertex at `0.61`), `points[1]`
must advance the way the arrow points, and turn numbers must climb with lap %.
Covered by `charlotte_roval_aligns_and_labels`.

### Tracks that cross over themselves (Oran Park GP, 202)

The config layer is a *stroke outline*, not a centreline. For a normal circuit
that outline is an annulus and `pick_best_subpath` takes the outer boundary,
which laps once. Where a track passes under itself the exporter **breaks the
stroke** so the overpass reads visually — Oran Park leaves an 80.7-unit gap at
Yokohama Bridge, marked by the only two radius-10 round caps in the path, at
`(1642, 545)` and `(1563, 529)`.

That break makes the circuit topologically an open arc, so its outline is one
closed ribbon with **no enclosed area** instead of an annulus. Used verbatim the
dot runs the full lap out along one kerb, folds at the bridge, and runs back
along the other — two laps of travel per lap of `LapDistPct`.

`tracks::ribbon::collapse_to_centerline` detects and undoes this:

- A fold satisfies `|signed area| = width × length/2`, so `2|A|/L` recovers the
  track width. Cross-check it against the median nearest-far-index distance;
  both must agree within 50 %, and ≥85 % of samples must have a partner. A
  genuine loop with a narrow neck fails the area test, which is what keeps
  `leaves_a_pinched_loop_alone` honest.
- Under a fold `i + partner(i)` is constant, so the involution's two fixed
  points are the caps: at `sum/2` and `sum/2 + m/2`.
- Pair each point against the *opposite polyline*, not the opposite vertex.
  Uniform arc-length sampling drifts a few samples through corners because the
  outer edge of a bend is longer than the inner one.
- Closing the returned ring spans the gap, which restores the missing underpass.

The recovered lap genuinely self-intersects, and that must survive — do not
"repair" it. Consequences downstream:

- `point_at` is arc-length based, so the dot crosses the bridge at constant
  speed even though the closing segment is ~6× a normal sample step.
- `pct_on_loop` snaps corner labels to the *nearest* branch. Turn 6 sits at the
  crossing and resolves correctly only because its label is offset outward.
  Check label pcts are monotonic after touching anything here.
- Ear clipping assumes a simple polygon. `track_path::loop_self_crossing` is
  computed once per track load into `map.cached_self_crossing`; Skia then uses
  `fill_closed_path` (winding fill) and egui skips the infield entirely, since
  it can only draw meshes.

Covered by `oran_park_bridge_is_folded_to_one_lap`, which asserts the fold is
gone, the crossing is present, and no sample-to-sample jump exceeds 0.1.

**Re-importing does not fix an already-saved track** — the bad geometry is in
the JSON, locally and in Mongo. Import the HTML again and press *Save track*.

After alignment, `start_finish` becomes a *fine offset* rather than the primary
calibration: `loop_frac = pct − start_finish`, so raising it pulls dots back
along the loop. Track Scan exposes it as **Map position offset (%)** in 0.1 %
steps, which is the tool for the residual lead left by outline arc length not
matching iRacing's `LapDistPct`. Click-to-calibrate writes the same field but
is far too coarse for a sub-percent nudge.

---

## Car motion — symptom

Cars crawl ~1–2s then stop, repeat around the track (even when sim cars move). Later: “got better but still not smooth.”

### Approaches tried (chronological)

| Rev / attempt | Idea | Result |
|---------------|------|--------|
| Coast + invented `coast_vel` floor | Always advance even when telem quiet | Field crawled on grid |
| Pin when `speed_mps < 0.75` or telem quiet 0.25s | Stop grid crawl | **Move/stop stutter** (quiet telem between samples) |
| Follow-ease toward telem only (no coast) | Avoid overshoot | Stops between SDK ticks when error→0 |
| Measured-vel coast + correct on telem change; decay vel if quiet >0.75s | Bridge gaps without floor | **0.75s decay = move/stop** |
| Same + remove 0.75s decay; soft-lock every frame to telem | Continuous lock | Soft-lock **cancels coast** when telem quiet → stutter again |
| Coast; correct **only** on telem change; pin if quiet >3s | Avoid cancel | Better |
| **Clock jump if `last_telem` age >2s** zeroed vel | Intended for pause/resync | **Matched 1–2s stutter period** — raised to 15s |
| Vel sample window `0.008..2.0` (exclusive end) | — | Dropped vel updates near 2s gaps → widened to `0.004..3.5` |
| Predict `last_telem + vel * age`, ease display toward it (`PCT_FOLLOW_TAU≈0.055`) | Continuous target between samples | Still stuttered (rev 1) |
| Composite screen-space ease (`COMPOSITE_SCREEN_TAU`) | Hide quantization | Rubber-band; **removed on composite in rev 2** |
| **Rev 2: pure integrator** `pct += vel*dt` | EMA vel; light blend+**err→vel** on telem sample; raw composite pts | Still stuttered — perf showed ~60 map presents/s (`map_dt≈0.016`), so **not ULW** |
| **Rev 3: dead-reckon only** | Vel **only** from `d(pct)/dt` EMA; no err→vel / no telem blend; teleport snap only | Between-stops smoother; **periodic stops remain** (`map_motion_rev=3`, presents OK) |
| **Rev 4: meaningful telem + anim grace** | Ignore micro LapDistPct jitter for vel EMA; keep `car_anim` across brief car-list dropouts; snap without zeroing vel | Still stuttered; **cars ran ahead of live location** |
| **Rev 5: session-time vel + weak live pull** | Vel from `session_time` when possible; `|vel|` capped via `lap_est`; weak every-frame pull to live (`tau=0.55`, not strong soft-lock / not err→vel) | Still **faster than sim** |
| **Rev 6: hard lead clamp** | Cap display to ≤`PCT_MAX_LEAD` ahead of live LapDistPct; tighter pull (`tau=0.14`); vel cap = `1/lap_est` (no 1.35×) | Still faster + stutter |
| **Rev 7: ease-only to live** | **No coast** — short ease onto live `LapDistPct` only (cannot lead SDK); `rem_euclid` for pct read | Still **faster + stutter** (`map_motion_rev=7`) — surge across each sample jump then freeze |
| **Rev 8: segment interp** | On telem change, lerp display→new telem over measured sample dt; never past latest sample; player uses scalar `LapDistPct` | Still too fast + stutter |
| **Rev 9: rev1 predict+ease + lap_est cap** | Restore `last_telem+vel*age` ease (`tau=0.055`); cap `|vel|` to `1.08/lap_est`; session_time dt; player scalar pct | Still fast + stutter — **`CarIdxSpeed` is not an SDK var**, so speed pin forced raw telem; “ahead” was path sampling |
| **Rev 10: telem + index path + screen lag** | Pct = live telem; index sampling; screen ease | Still inaccurate — imports are **arc** outlines, not LapDistPct racing-line; index change was wrong root cause |
| **Rev 11: calibrate + reverse** | Arc `point_at` again; `reverse_path` setting; S/F edit click = “I am here”; no composite screen ease | **Location OK** (user); stutter remains |
| **Rev 12: never-lead segment + 60 Hz present** | Segment-lerp display→telem over sample dt; light composite screen lag; cap hot ULW to ~60 Hz | Still stuttered — segment **hold at sample end** = move/stop |
| **Rev 13: lag-only follow** | Ease toward live telem (`tau=0.09`, never leads); stronger screen lag; cheaper 4-dir label halo | Still stuttered — **`dt<=0` treated as pin** on host+sprites double tick → snapped to staircased telem every present |
| **Rev 14: Python parity** | Keep eased pct when `dt≈0`; racing-line dots use eased % only (no XY double-lag); route keeps screen ease; hot present ~30 Hz; `panel_animating` from composite | Still stuttered — lag-only **freezes between staircased LapDistPct samples**; also prior predicts often **rewrote `last_telem*` every frame** (age=0) |
| **Rev 15: predict, timestamps on change** | `predict = last_telem + vel*age`; update telem clock **only on meaningful change**; no soft-lock to held sample; `|vel|` cap via `lap_est`; player-only composite labels; ~60 Hz present | Still stuttered in practice — **`pre_green` pin** (SessionState&lt;4) forced raw telem during warmup/practice |
| **Rev 16: no pre_green pin** | Same predict; pin only demo + OnPitRoad; AttachConsole to parent for `--perf`; sess= in perf line | Still stuttered at sess=4; perf: paints=0, presents≈35/s, **ulw≈120ms/s**, map_dt≈0.029 |
| **Rev 17: no hot composite + integrator** | Disable CPU composite (classic GL paint like Python); `pct += vel*dt` between samples; quiet decay 20s (was 2.5s) | **Worse** — map readback ~200ms/s, presents ~25/s (`map_dt≈0.04`) |
| **Rev 18: composite back + Python ease** | Re-enable hot composite; pct ease τ=0.09 (no coast); no labels/AA stroke on hot present; present every 8ms | Presents≈60 OK but **ulw≈230–330ms/s** — ULW hitch *is* the stutter |
| **Rev 19: half-res ULW @ 30 Hz** | Cache half-res BG; composite+present at ½ res via `present_bgra_scaled`; `MAP_HOT_PRESENT_MS=33` | presents≈18–20, ulw≈90ms — **scaled path still ULWs full HWND**; half did not cut upload |
| **Rev 20: dirty-rect ULW** | Full-res composite; `UpdateLayeredWindowIndirect` with car dirty rects only | ulw≈28–33ms/s OK, but presents≈18–22/s + lag-ease freeze |
| **Rev 21: coast + 60 Hz** | Keep dirty ULW; restore `pct+=vel*dt` coast (timestamps only on real telem Δ); soft lead clamp; `MAP_HOT_PRESENT_MS=16` | presents≈33 (16ms gate on ~70fps), ulw≈93ms; **lead clamp = move/stop** |
| **Rev 22: no lead clamp + every-tick + per-car dirty** | Remove lead freeze; `MAP_HOT_PRESENT_MS=0`; per-car Indirect | presents≈70 OK, ulw≈110ms/s; **still stuttered** (coast+TELEM_CORRECT yank) |
| **Rev 23: wall-clock predict+ease** | `predict=last_telem+vel*wall_age` (not SessionTime age); ease τ=0.055; no hard blend; present ~33ms | pending user test |
| **Rev 24: continuous predict bias** | On telem Δ, set `predict_bias` so predict does not dart when age resets; softer VEL_EMA/τ; hot present 16ms while animating | pending user test |
| **Rev 25: pure integrator** | `pct += vel*dt` coast; vel EMA + 10% telem correct only on Δ; no predict/bias (bias decay pulsed speed) | ahead + sample yank stutter |
| **Rev 26: ahead-only correct** | Coast with `vel_cap=1/lap_est`; on telem Δ snap+bleed vel only when ahead; no TELEM_CORRECT; Skia presents in `--perf` | pending user test |
| **Rev 30: per-frame ahead pull** | `AHEAD_PULL=0.40` every frame when coast leads telem | Killed the lead but pinned the dot to the staircase — move/stop returned |
| **Rev 31: critically damped tracker** | One unconditional 2nd-order filter + measured-vel feed-forward; `timeBeginPeriod(1)`; even 16 ms host period | Motion "better, still not smooth" — presents still 37–47/s |
| **Rev 31b: no timer wake with map open** | `request_repaint()`; the loop is vsync-locked and the timer sleep was costing ~40 % of refreshes | pending user test |

### Path sampling (critical)

Track JSON comes from **members-site SVG outlines** (arc-resampled on import), **not** iRacing racing-line LapDistPct samples. Same approach as other overlays (iRaceHUD): need per-track **`start_finish` offset** + optional **`reverse_path`**.

**Calibrate:** Track Scan → “Calibrate map position” → while driving, click where you are on the map → Save track. If cars still run the wrong way, enable Map → Layout → `reverse_path` and recalibrate.

### `pct_map` — lap % is not the drawn arc fraction (2026-08-05)

`point_at(loop, lap_pct)` treats lap % as a fraction of the drawn loop's **arc
length**. That holds only if the drawing distributes length exactly like the real
track. Members SVGs get the shape and the overall scale right but not the
distribution, so the dot leads through some parts of the lap and lags through
others. `start_finish` cannot fix it — that is a constant shift, and this error
changes sign around the lap.

Measured at Charlotte Roval (554) from three driven laps: **19 m ahead at worst,
45 m behind at worst**, with the dot covering drawn track between **0.82× and
1.41×** the correct rate. The fast stretch is 5–20 % of the lap, which is what
"the dot is quicker than the car on the straights" actually was.

`--log-track-path` records three laps and solves a **`pct_map`**: 256 knots giving
the loop arc fraction at each lap %. Stored on the track document, read by
`track_path::parse_pct_map`, applied in `widgets::map::loop_frac_for_pct` so every
lap-%-to-position conversion (cars, markers, corners, sectors, S/F tick, DRS)
goes through it. Absent `pct_map`, behaviour is exactly as before.

Cross-validated by fitting the table on one lap and scoring the next:
**rms 14.1 m → 1.4 m, worst 43.9 m → 5.0 m**. It transfers between laps, so the
curve is a property of the drawing rather than dead-reckoning noise.

Things to know:

- **iRacing does not expose `Lat`/`Lon`/`Alt` in live telemetry** — the variables
  are absent, not zero. The path is dead reckoned from `VelocityX`/`VelocityY` and
  `Yaw` instead, which closes a lap to ~4.5 m over 3.6 km before the residual is
  rubber-banded out. All three are seated-car variables, so spectating records
  nothing.
- The table bakes in the `start_finish` and `reverse_path` of the moment it was
  solved. Change either and recalibrate.
- `--calibrate-track-path <csv>` re-solves from a recording already on disk, so a
  solver change does not cost another driving session.
- Some Members drawings are **wound against LapDistPct**. On a symmetric oval
  that looks the same as a left/right mirror to Procrustes — calibration
  **prefers reversing** the polyline (S/F kept at index 0) so the familiar
  outline is preserved. A true mirror bake is used only when it fits clearly
  better. Chirality is decided by **LapDistPct↔arc alignment** after the
  Procrustes fit (not dead-reckon yaw alone) — yaw-only reverse flipped Iowa
  when the drawing was already correct. Corner labels for 2–4 turn ovals are
  rebuilt only when the outline actually changes (quadrant detection otherwise
  collapses T1/T2 on D-ovals).

#### The projection window has to stay narrow

`project_monotone` walks the driven points around the drawn loop, taking the
nearest dense vertex to each. Points arrive one even lap-% step apart, so the
window is sized in **knot steps** — 1.5 back, 3 ahead — rather than as a fixed
slice of the lap.

This is not a tuning preference. Charlotte's infield hairpin folds back within a
few metres of itself while being ~100 m away along the lap, so any window that
reaches that far ahead finds the return leg closer than the leg the car is
actually on. The walk jumps the gap, doubles back once past it, and the lap is
rejected as non-monotone: three complete laps produced no table at all. An
`ahead` of a sixth of the loop is 600 m against a 14 m knot step.

Backward steps are now clamped out of the table rather than voiding the lap —
fit noise nudges the odd knot backwards on any real recording. `MAX_BACKTRACK`
(2 % of a lap, cumulative) is what still rejects a drawing the walk cannot
follow.

Residual after this fix, scoring the saved table against a recording it was not
fitted on: **rms ~2 m, within ±5 m for the whole lap** bar a ~12 m blip at the
hairpin apex, where the drawing is sharply short and the exact lap % of the
feature moves with the racing line. 256 knots cannot resolve that, and chasing
it would fit one line rather than the drawing.

### Perf notes (2026-07-21)

Rev 11: location OK after calibrate.
Rev 12: segment hold + heavy 8-dir font halos likely kept stutter/ULW spikes.
Rev 13: lag-only pct + cheaper labels — **still stuttered**: `dt<=0` pin on double `tick_car_motion` wiped ease every composite frame.
Rev 18: composite restored; presents≈60 but ulw≈300ms/s (stutter = ULW).
Rev 19: half-res composite + scaled ULW; ~30 Hz — ULW still full-frame upload.
Rev 20: dirty-rect UpdateLayeredWindowIndirect for car regions — ulw cheap, presents still ~20Hz.
Rev 21: coast + 16ms present — lead clamp froze cars; presents≈33/s.
Rev 22: no lead clamp; present every host tick; per-car dirty ULW — presents OK, motion still stuttered.
Rev 23: wall-clock predict+ease (SessionTime age was freezing predict); ~30 Hz present.
Rev 24: continuous predict via decaying `predict_bias` on telem Δ; follow τ=0.075; hot present 16ms while moving.
Rev 25: pure integrator `pct+=vel*dt`; light telem-only correct (0.10); dropped predict/bias (bias decay = speed pulses).
Rev 26: ahead-only snap+vel bleed on telem Δ; vel_cap 1.0/lap_est (was 1.08×); Skia presents counted in `--perf`.

### Track load — wrong map stuck

| Date | Issue | Fix |
|------|--------|-----|
| 2026-08-03 | Live session kept previous Track Scan / imported `cached_track_id` because host overwrote SDK `frame.track_id` every telem tick | Only stamp authoring ID when demo/disconnected/no SDK TrackID; invalidate cache when live ID differs |

### Why rev 31 is shaped differently

Revs 1–30 all shared one structure: a free-running coast plus **conditional**
corrections (snap above an error threshold, one blend when behind, a stronger
one when ahead, a per-frame pull, vel bleed). Every condition assigns display
velocity directly, so each telem sample produces a step change in speed — and
a step in speed is exactly what the eye reads as stutter. Tuning the constants
only moves the step around.

Rev 31 removes every branch except the teleport snap and runs one critically
damped tracker each frame:

```text
a       = ω²·(telem − pct) + 2ω·(vel_telem − vel_disp)
vel_disp += a·h        (substepped, h ≤ 10 ms)
pct      += vel_disp·h
```

Properties that matter here:

- **C1 output.** Velocity is only ever integrated, never assigned, so there is
  no frame where the dot changes speed discontinuously.
- **Zero steady-state lag.** With the feed-forward term the filter settles on
  `vel_disp = vel_telem`, `err = 0` — it cannot persistently lead (rev 30's
  complaint) or trail (rev 27's).
- **The staircase is filtered, not followed.** ω ≈ 20 rad/s (~3.2 Hz) against
  60 Hz samples attenuates the step to well under a pixel.
- **A stopped car settles.** Feed-forward bleeds (τ = 0.30 s) once LapDistPct
  has been held for 0.10 s, so a parked car does not hover ahead of its mark.

### Present cadence — the loop is vsync-locked (rev 31)

**Do not schedule a timer wake while the map is open.** eframe's root-window
swap blocks on vsync, so the host loop is already paced by the display. Any
`request_repaint_after` sleeps *first*, and because the wake is quantized to
the ~15.6 ms system tick the frame then runs past the next refresh boundary
and the swap eats a whole extra period.

How to tell the two causes apart from a `--perf` log — compare cycle time
(`1000/fps`) against frame time:

| Cause | Prediction | Rev 30 log (frame 4.8–6.1 ms) |
|-------|------------|-------------------------------|
| Timer tick | cycle = `frame + 15.6`, tight 20.4–21.7 ms band | ✗ |
| Vsync drops | cycle = mix of 16.7 / 33.3 ms, wide spread | ✓ observed 20.4–28.0 ms |

The wide spread at near-constant frame cost rules out the timer: we were
hitting ~60 % of refreshes and missing the rest, i.e. alternating 16.7/33.3 ms
present spacing. That is judder no matter how good the motion model is.

Fix: `ctx.request_repaint()` (zero delay) when the map is open, so the swap is
the only gate and presents land one per refresh. `timeBeginPeriod(1)` also
went in (`main.rs::request_high_res_timer`) — it does not affect the map path
but keeps non-map wakes honest.

### The real stall was `nvidia-smi` (rev 32)

Rev31b did not land 60 fps: presents stayed 31–45/s with `map_dt max=0.14–0.21 s`
— a **150–200 ms freeze of the whole overlay, twice a second**. No motion model
survives that, and no amount of vsync tuning explains a gap that large.

The per-second `telem=` counter (added for exactly this) alternated 4 ms / 10 ms
and averaged ~7 ms, matching ~2 × 150 ms of blocking work per second amortised
over the frames. Source: `sysstats::probe_gpu` shelled out to **`nvidia-smi`, a
child-process spawn, synchronously on the UI thread, every 500 ms**, from inside
`tick_telemetry`. Machines *without* an NVIDIA GPU paid it too — the spawn still
has to fail.

Fix: `SysStats` now owns a background thread that samples CPU/memory at 500 ms,
probes the GPU at 3 s, and gives up after 3 consecutive misses. `sample_into`
is a `try_lock` + string clone. Cheap values publish *before* the GPU probe so a
slow `nvidia-smi` cannot delay them.

**Rule: nothing that spawns a process, touches the network, or hits disk may run
on the host frame.** Sample it on a thread and read the last snapshot.

### Open questions

- Mid-lap residual error after calibrate: was blamed on “SVG outline ≠ racing
  line”, but it is mostly the arc-length mismatch — see `pct_map` above.
- Do not gate motion on `CarIdxSpeed` (not in SDK).
- Do not pin on `dt<=0`. Do not soft-lock every frame to held telem.
- Do not rewrite `last_telem_session` every frame (kills predict age).
- Do not full-size ULW at 60+ Hz (rev 18 cost).
- Do not reintroduce conditional position corrections — see rev 31 above.

### Code pointers

- Motion: `widgets/map.rs` → `advance_car_pcts`, `loop_frac_for_pct`, `sf_for_player_at`
- Path: `track_path.rs` → `load_points`, `point_at` (arc)
- Calibration: `tracks/probe.rs` (record, `calibrate_from_csv`), `tracks/calibrate.rs` (solve)
- Labels: `layered.rs` → `draw_car_labels_fonts`
- Host tick: `host.rs`; calibrate UI: `settings/scan.rs`

---

## Map numbers / labels

| Attempt | Result |
|---------|--------|
| 5×7 bitmap glyphs | Blocky; “squares in curves” |
| Supersampled bitmap | Better, still chunky |
| egui font atlas blit in `layered.rs` (`draw_car_labels_fonts`) | Smoother AA; user said numbers improved |
| 8-dir halo every present | Expensive; cut to 4-dir in rev 13 |
| Opponent labels every present | Dropped on hot composite (player only) in rev 15 |

### Qt parity pass (2026-08-04)

The Rust port had drifted from `main`'s Python/Qt look. Reference is
`overlay/widgets/track_map.py` on `main` (`_draw_car_number_label`,
`_draw_stroked_center_text`, `_draw_corners`, `_draw_start_finish`).

| Element | Rust had | Qt / now |
|---|---|---|
| On-dot number size | 10.0 / 8.5, min 7 | **9.0 / 7.5, min 6, rounded** |
| On-dot ink | `contrast_text(dot_fill)` — flipped dark on light dots | **always white** |
| On-dot halo | black α230 *or* white α220 | **black α220 (α160 for pace)** |
| Halo width | 1.35 / 1.15 | **1.2 player / 1.0 other** |
| Halo passes | 8-way for everyone | **8-way player+pace, 4-way others** |
| Corner pill | hard-coded `rgba(15,18,22,200)`, no border | **`draw_dark_cell`** (`cell_dark` + 1 px `cell_border`) |
| Corner ink | white | **`corner_text` `#d6dce2`** (now a real config default) |
| Corner box width | `text_w.max(sz+4) + 12` | **`(text_w + 12).max(box_height)`** |
| S/F tick | always 7 / 3 | **9 / 4 while `sf_edit`, else 7 / 3** |

Shared so the two paths cannot drift again: `emap::car_label_metrics`,
`CAR_LABEL_RICH` / `CAR_LABEL_PLAIN`, `CORNER_CELL_RADIUS`, `CORNER_TEXT`,
`sf_tick_style`.

**Deliberately not copied:** Qt pixel-snaps the label centre
(`round(c.x()), round(c.y())`) to stop glyph pulsing. Now that presents land on
every refresh, snapping makes the label crawl against a subpixel-eased dot, so
the Rust paths keep the dot's exact centre. Skia measures glyph width only, so
corner pill height uses `sz * CORNER_LINE_HEIGHT` to match egui's galley.

---

## Who the map calls "you"

`widgets::map::focus_car_idx` resolves it: if your seated car is still a live
competitor, focus stays on you even when the camera wanders; pure spectators
(ghost seated entry) follow the iRacing camera car, else the seated player. It
drives dot radius, the green player fill, the centre ring, label weight, draw
order, and which car the leader / ahead / behind markers hang off.

`frame.cars[].is_player` is deliberately left as the *seated* player — timing
tables already take the same approach via `apply_table_focus`, which mutates a
clone (and also keeps focus on a live seated racer while watching). Two things
must keep using it:

- The `player_lap_dist_pct` override in the pct feed. It is a higher-precision
  value for the seated car only; applying it to a spectated car places the dot
  at the wrong lap %.
- The S/F edit click ("click where YOU are") and its hint, which can only mean
  the seated car's own telemetry.

Before this, spectating drew the followed car as anonymous traffic *and* showed
no traffic markers at all — `select_marker_candidates` looked up `is_player`, and
a spectating player's ghost car has `position == 0`, which returned early before
even the leader slot. It now takes `focus_idx` explicitly. When the camera is on
your own car the focus resolves to the same car, so racing is byte-identical;
`camera_on_own_car_matches_seated_behaviour` pins that.

---

## Infield fill

| Issue | Fix |
|-------|-----|
| Centroid-fan fill leaked on concave tracks (worse on cached composite BG) | Ear-clip triangulation (`fill_infield` / `earcut_triangles`) |
| Stale broken BG after leaving edit | Invalidate `map_bg` when `edit_mode` flips (`last_edit_mode` in `host.rs`) |
| Ear clipping needs a simple polygon; bridge layouts are not | `map.cached_self_crossing` → Skia `fill_closed_path` (winding), egui skips |

---

## Static BG cache + dirty ULW — the rule

**Any frame that recaptures `map_bg` must present the FULL window.** Dirty
rects only cover car sprites, so pushing a fresh background through them
uploads the new track *only where a car has been*. Symptom: on first load the
map "draws itself in" behind the player dot and stays blank elsewhere.

- `host.rs` (egui path) gets this right via `clear_map_bg()`, which also clears
  `map_ulw_dirty` (empty = full present).
- `skia_ui/manager.rs::paint_map` did **not** — fixed 2026-08-04 by forcing
  `dirty = []` whenever `need_recapture` is true.

`bg_fingerprint` changes (track load finishing, config/colour edits, resize)
all land here, so this is not just a first-load path.

---

## Related non-map fixes (same stretch)

- Standings while spectating: no center window; lap tints follow presentation focus
- Demo map dots: pin to continuous demo telem; don’t let coast fight demo feed

---

## Changelog for agents

Append a short bullet each time you change map behavior:

- **2026-07-21 (motion rev 1–9):** See prior bullets (coast / predict experiments).
- **2026-07-21 (motion rev 10):** Tried index LapDistPct sampling — wrong for SVG arc imports.
- **2026-07-21 (motion rev 11):** Location fix: arc sampling + click-calibrate S/F + `reverse_path`.
- **2026-07-21 (motion rev 12):** Never-lead segment interp + 60 Hz ULW cap — still stuttered (hold at sample end).
- **2026-07-21 (motion rev 13):** Lag-only pct ease (`tau=0.09`); cheaper label halo; keep screen lag + 60 Hz cap.
- **2026-07-21 (motion rev 14):** Root cause: second `tick_car_motion` with `dt≈0` **pinned** all cars to raw telem (wiped ease). Fix: keep eased pct on re-entry; Python placement (pct from eased % only, XY ease for route only); hot ULW ~30 Hz; set `panel_animating` from composite.
- **2026-07-21 (motion rev 15):** Lag-only still froze between samples. Predict `last_telem+vel*age` again, but **only bump telem timestamps on meaningful change** (earlier predicts zeroed age every frame). No soft-lock; lap_est vel cap; composite labels for player only; 60 Hz present.
- **2026-07-21 (motion rev 16):** Removed `SessionState < 4` pin — practice/warmup (state 2) was forcing staircased raw telem. `--perf` attaches parent console; perf prints `sess=`.
- **2026-07-21 (motion rev 17):** Perf on rev16: `paints=0` hot composite, `ulw≈120ms/s`. Disabled hot composite (GL paint path). Integrator coast; quiet decay 20s (2.5s matched stutter period).
- **2026-07-21 (motion rev 18):** Rev17 worse (`map≈200ms/s` readback, ~25 presents/s). Restored hot composite; Python pct ease; strip hot labels/AA; 8ms present cadence.
- **2026-07-21 (motion rev 19):** Rev18 ulw≈300ms/s at 60 presents. Half-res BG cache + scaled ULW; present ~30 Hz (`MAP_HOT_PRESENT_MS=33`).
- **2026-07-21 (motion rev 20):** Rev19 still ~5ms/ULW (scaled still full HWND). Dirty-rect `UpdateLayeredWindowIndirect` around car sprites.
- **2026-07-21 (motion rev 21):** Rev20 presents≈20/s + lag-ease freeze. Coast `pct+=vel*dt` between samples (no every-frame timestamp rewrite); soft lead clamp; dirty ULW @ ~16ms cadence.
- **2026-07-21 (motion rev 22):** Rev21 lead clamp froze after ~0.28s coast; 16ms gate → ~33 presents. Removed clamp; present every tick; per-car dirty Indirect.
- **2026-07-21 (motion rev 23):** Rev22 presents≈70 but coast+35% telem yank still stuttered. Wall-clock `predict=last+vel*age` + ease; SessionTime age was a false freeze; present ~33ms.
- **2026-08-03 (motion rev 24):** Rev23 predict darted when LapDistPct samples reset age→0. Absorb jump into decaying `predict_bias`; softer vel EMA + follow τ=0.075; map hot present 16ms while cars animate.
- **2026-08-03 (motion rev 25):** Rev24 bias decay pulsed predict speed each sample. Pure integrator `pct+=vel*dt`; vel EMA + 10% position correct only on telem Δ; remove predict/bias.
- **2026-08-03 (motion rev 26):** Rev25 coast ran ahead (1.08× vel cap) then yanked 10% each sample. Ahead-only snap+vel bleed; vel_cap=1/lap_est; `--perf` counts Skia presents/ulw/map_dt.
- **2026-08-03 (motion rev 27):** Rev26 ahead-snap collapsed vel → dots behind + staircased. Soft pct blend (0.04) + soft vel correction on telem Δ only; no hard snap. Skia `MAP_MS=16`, map painted first outside budget; `animating` while connected with on-track cars. Target `map_dt≈0.016–0.02`.
- **2026-08-03 (motion rev 28):** Rev27 still `map_dt≈0.05` — `MAP_MS=16` + ~11ms host frames skipped every other present (~20 Hz). Paint map every host sync; when map open request repaint in 1–8ms. Soft sync unchanged.
- **2026-08-03 (motion rev 29):** Rev28 full Skia every sync → `frame≈19ms` / `map_dt≈0.04`. Skia hot path: StaticOnly BG cache + `composite_map_cars` + dirty ULW (same as egui). Tick once before present. Target `map_dt≈0.016–0.025`.
- **2026-08-04 (motion rev 30):** Rev29 smooth (~60 Hz) but dots ahead of the car. Per-frame ahead-only soft pull toward telem (no vel bleed); stronger telem-Δ blend when ahead.
- **2026-08-04 (motion rev 31):** Rev30's per-frame ahead-pull pinned dots to the staircase. Replaced the whole coast+conditional-correct scheme with a single critically damped tracker (ω=20) plus measured-vel feed-forward — no branches, C1 position, zero steady-state lag. Also fixed present spacing: `timeBeginPeriod(1)` (winit was rounding sub-16 ms waits to the 15.6 ms system tick) and an even 16 ms host period while the map is open.
- **2026-08-04 (motion rev 31b):** Rev31 motion was better but presents were still 37–47/s. Cycle-vs-frame time in the perf log showed the loop is **vsync-locked**, not timer-bound, and the `request_repaint_after` sleep was pushing work past the refresh boundary (~40 % of refreshes missed → 16.7/33.3 ms alternation). Map open now calls `request_repaint()` with no delay.
- **2026-08-04 (motion rev 32):** Rev31b still 31–45 fps with `map_dt max=0.14–0.21 s`. The new `telem=` counter caught it: `sysstats::probe_gpu` spawned **`nvidia-smi` synchronously on the UI thread every 500 ms** (~150 ms/spawn, paid even with no NVIDIA GPU). `SysStats` moved to a background thread — CPU/mem at 500 ms, GPU at 3 s with a 3-miss giveup; `sample_into` is now a `try_lock` + clone.
- **2026-08-04 (svg import):** Charlotte (554) dots read ~11 % ahead *and* ran backwards. Not calibration — the S/F layer drew its stripe as `<rect>` and arrow as `<polygon>`, but `sf_paths_sorted` only read `<path d>`, so `align_loop_from_sf` found neither and fell back to bottom-most point + `ensure_ccw`. Added rect/polygon/polyline parsing, changed arrow heading to vertex-mean→farthest-vertex, parsed `<tspan>` turn labels (0 → 15 corners), and made the oval label flip conditional on `labels_run_backwards`.
- **2026-08-04 (bg cache):** Skia hot path presented a freshly captured `map_bg` through car-only dirty rects, so a newly loaded track only appeared where the player had driven. Recapture frames now present full-window.
- **2026-08-03 (track load):** Host no longer overwrites live SDK `TrackID` with `cached_track_id` every tick (stuck wrong map after Track Scan / prior session). Authoring stamp only when demo/disconnected; invalidate when live ID changes.
- **2026-08-04 (hot path parity):** Dots blinked out, showed no numbers, and had no player / leader / ahead / behind markers. Root cause was not the renderer but the *hot path*: `skia_ui::manager::paint_map` composited cars with `layered::composite_map_cars`, a CPU circle blitter that hardcodes `label = String::new()`, has no marker rings, no player ring and no status badges — and `build_car_sprites` dropped the worse-placed car of any stacked pair, which is what made dots vanish and return. Since rev 32 freed ~12 ms/frame, the hot path now seeds a **reused** `Canvas` (`Canvas::new` reloads three typefaces, so it must not be per-frame) with the cached static BGRA via `write_bgra`, then runs the real car renderer through the new `MapPaintMode::DynamicOnly`. Everything in `paint_inner` before the cars is skipped because it is already in the bg cache. Dirty-rect ULW was dropped with it: traffic-marker badges and pills sit well outside the car radii, so car-anchored rects can't cover them. Watch `ulw=` in `--perf` — it was ~0.6 ms/present with dirty rects.
- **2026-08-04 (spectator focus):** Spectating drew the followed car as plain traffic and showed no leader/ahead/behind markers, because both keyed off `is_player` — the seated car, which while spectating has `position == 0` and `lap_dist_pct < 0`. Added `widgets::map::focus_car_idx` (camera car → seated player) and threaded it through both renderers plus `build_car_sprites`; `car_fill` and `select_marker_candidates` now take it as an argument instead of reading `is_player`. Also extended `car_on_route`'s ApproachingPits branch to the focus car so a spectated pit entry blends like your own. `is_player` still governs the `player_lap_dist_pct` override and the S/F edit click.
- **2026-08-05 (motion rev 34):** Iowa oval: dot off when fast, matched when slow. Root cause was the critically damped tracker chasing a *held* `LapDistPct` while feed-forward vel stayed nonzero — that DE settles at `telem + 2·vel/ω` (~5–15 m at oval speed, ~0 when stopped). Rev 34 tracks the predicted setpoint `last_telem + vel·age` instead, so steady state is on the coasting prediction with no speed-proportional bias.
- **2026-08-09 (motion rev 35):** Direction OK after calibrate fix, but dots **lagged behind above ~85 mph** (all cars). Feed-forward `vel` was clamped to `1/lap_est` (lap-average pace); oval straights are faster than that average, so measured `d(pct)/dt` was clipped and the predicted setpoint trailed the car. Cap is now `2/lap_est`.
- **2026-08-10 (motion rev 36):** Dots still **lagged when the car sped up** (throttle / oval straight). Symmetric vel EMA (0.25) trailed true pace under acceleration, so `last_telem+vel·age` sat behind the car. Rev 36 uses asymmetric EMA (`0.55` up / `0.22` down) and adds a short positive-accel term `½·a·t²` to the predicted setpoint.
- **2026-08-10 (motion rev 37):** Rev 36 overshot — dots **led** the car. Removed `½·a·t²`; softened EMA to `0.35` up / `0.25` down (rev 35 setpoint + mild speed-up bias only).
- **2026-08-10 (motion rev 38):** Still **led**. Root: tracker inertia overshot past `last_telem+vel·age`, and 2× vel-cap left room to run hot. Symmetric EMA `0.28`, vel-cap `1.45/lap_est`, pred age `0.10 s`, plus a hard ceiling so display % cannot lead `last_telem` by more than `vel·age_max`.
- **2026-08-10 (motion rev 39):** Still **led at high speed**. Real bug from rev 34 maths: once coast age hits `age_max`, target freezes but damper still got `vel_ff=vel`, so equilibrium is `target + 2·vel/ω` (grows with speed). Rev 39 sets `vel_ff=0` when prediction is saturated; restores `age_max=0.20` for sparse multiplayer samples.
- **2026-08-10 (HTML import dropped `pct_map`):** Iowa still led after rev 39 with healthy presents (`map_dt≈0.018`). First guess was “import left `cached_pct_map=None`”; restoring the old table did **not** fix live aheadness.
- **2026-08-10 (HTML import + stale/wrong cal):** Re-import replaced Iowa’s outline; the saved `pct_map` was nearly identity and did not match the new winding. Import now keeps cal only when loop points still match; otherwise status is `(cal cleared — recalibrate)`. Save preserves `pct_map` only for an unchanged loop. `--perf` prints `cal=N` (table knots, or 0).
- **2026-08-10 (Iowa false reverse/mirror):** `--calibrate-track-path` baked `LoopFix::Reverse` then `Mirror` from dead-reckoned yaw on Iowa 559; live dots ran the wrong way / flipped D. Calibrate no longer rewrites outline winding — it only writes `pct_map` (and undoes a stale `map_mirror` bake). Restored Members HTML outline + fresh calibration.
- **2026-08-10 (motion rev 40):** Iowa still **ahead** with `cal=256`, correct direction, `map_dt≈0.018`. Coast setpoint + vel feed-forward was still leading live LapDistPct. Rev 40 tracks telem only (`vel_ff=0`) and hard-clips any damper overshoot past the live sample.
- **2026-08-10 (motion rev 41):** Tried `telem − vel·0.045s` for Charlotte’s hair-ahead — user reported **more** ahead. Reverted.
- **2026-08-10 (motion rev 42):** Back to rev 40 (track telem, `vel_ff=0`, never-lead clip). Residual Charlotte offset is treated as geometry/`pct_map`, not motion lag.
- **2026-08-10 (motion rev 43):** Still slightly ahead on straights while accelerating. Racing-line dots now use **raw LapDistPct** (no spring/coast). Recalibrated Charlotte 554 `pct_map` from `trackpath-554-20260805-123723.csv` (up to 1.17% of a lap).
- **2026-08-11 (pit lane dots):** Exit merge geometry was fine; dots along `pit_path` were not. Dropped loop/pit arc-cal on the lane (it bunched cars at the exit on short schematics). OnPitRoad placement now projects the calibrated racing-line point onto `pit_path` (`nearest_point_on_open`) so lap-% matches frontstretch traffic. Also keep latched mid-lane + InPitStall on the route. `map_motion_rev=45`.
- **2026-08-10 (pit enter/exit):** Iowa HTML has exit merge but no `pit_in` entry. Cleanup: seed latches only on `OnPitRoad` (stop false-latching traffic in the long oval pit wrap); screen-ease pit XY instead of hard snap; synthesize a short racing→path entry when entry poly is missing; clamp exit-latch so a late clear cannot open a near-full-lap exit wrap.
- **2026-08-05 (map calibration):** "The dot moves faster than the car on the straights" was neither the motion model nor the overall map scale. A critically damped tracker has zero steady-state error and only `a/ω²` (~5e-6 lap) under acceleration, and the Charlotte drawing measures 0.472 m per SVG unit — a believable 9.4 m track width. What is wrong is that `point_at` reads lap % as a **drawn arc fraction**, and imported SVGs do not distribute length like the real track. Dead reckoning three driven laps from `VelocityX`/`VelocityY`/`Yaw` (iRacing exposes no `Lat`/`Lon`) put the dot 19 m ahead at worst and 45 m behind at worst, running 0.82–1.41× the correct rate. `--log-track-path` now solves a 256-knot `pct_map` (Procrustes shape fit + monotone projection, `tracks::calibrate`) and writes it to the track document; `loop_frac_for_pct` applies it to every lap-%-to-position conversion. Fit on one lap and scored on the next: rms 14.1 m → 1.4 m, worst 43.9 m → 5.0 m.
- **2026-08-05 (calibration rejected every lap):** A clean three-lap recording at Charlotte produced no `pct_map` — "laps did not match the drawn loop well enough". The shape fit was fine (14 m rms on a 3.6 km lap); the projection was not. `project_monotone` searched `m/6` of the loop ahead of the current vertex — 600 m against a 14 m knot spacing — and at the infield hairpin, whose legs run a few metres apart but ~100 m apart along the lap, the return leg won. The walk jumped the gap and doubled back on the way out, and the strict "no backward step" gate threw both laps away. The window is now sized in knot steps (1.5 back, 3 ahead), and backward steps are clamped out of the table instead of voiding the lap, with cumulative backtracking over 2 % of a lap still rejecting it. Also added `--calibrate-track-path <csv>` so an existing recording can be re-solved without driving again. Scored on the *other* recording: rms 14.3 m → 2 m, worst 44.8 m → 12 m, the remainder a single blip at the hairpin apex.
- **2026-08-04 (self-crossing import):** Oran Park GP (202) laps twice per lap of `LapDistPct`. The exporter breaks the stroke at Yokohama Bridge to draw the overpass, which makes the circuit an open arc, so its outline is a single zero-area ribbon rather than an annulus and `pick_best_subpath` had no outer boundary to pick. New `tracks::ribbon::collapse_to_centerline` folds the ribbon onto its centreline (area-derived width cross-checked against nearest-far-index pairing, opposite-*polyline* projection to absorb corner drift) and closes the ring across the gap. The recovered lap self-intersects on purpose: `track_path::loop_self_crossing` flags it so the infield uses a winding fill instead of ear clipping. Also filtered the turns layer with `is_turn_label` — Oran Park puts corner names in it, which imported as 28 turns.
