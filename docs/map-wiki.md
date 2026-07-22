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
| `tick_car_motion` → `advance_car_pcts` | Wall-clock predict+ease between LapDistPct samples |
| `build_car_sprites` | Map dots for CPU composite (player label only on hot path) |
| `layered::composite_map_cars` | Blit dots + font atlas labels onto cached BG |
| `host` map hot path | Full-res CPU composite + **per-car dirty** ULW ~30 Hz |

Live (non-edit) uses **cached static BG + CPU car composite**. Edit / authoring uses full GL paint.

---

## Track cache / wrong map

| Date | What happened | Outcome |
|------|----------------|---------|
| 2026-07-21 | Local track JSONs cleared so Mongo would re-fetch | Cache nearly empty; only `166.json` (Okayama) remained |
| 2026-07-21 | User saw wrong / missing map | Expected — `sync_down` only **refreshes existing** files |
| 2026-07-21 | Added `cloud::sync_library` + `--sync-tracks`; sparse cache auto-pulls full library | Restored **407** tracks under `%LOCALAPPDATA%\GridGlance\tracks` |

**Do not wipe the tracks dir** without running `--sync-tracks` (or waiting for sparse auto-pull) afterward.

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

### Path sampling (critical)

Track JSON comes from **members-site SVG outlines** (arc-resampled on import), **not** iRacing racing-line LapDistPct samples. Same approach as other overlays (iRaceHUD): need per-track **`start_finish` offset** + optional **`reverse_path`**.

**Calibrate:** Track Scan → “Calibrate map position” → while driving, click where you are on the map → Save track. If cars still run the wrong way, enable Map → Layout → `reverse_path` and recalibrate.

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

### Open questions

- Mid-lap residual error after calibrate: SVG outline ≠ racing line (inherent).
- Do not gate motion on `CarIdxSpeed` (not in SDK).
- Do not pin on `dt<=0`. Do not soft-lock every frame to held telem.
- Do not rewrite `last_telem_session` every frame (kills predict age).
- Do not full-size ULW at 60+ Hz (rev 18 cost).

### Code pointers

- Motion: `widgets/map.rs` → `advance_car_pcts`, `loop_frac_for_pct`, `sf_for_player_at`
- Path: `track_path.rs` → `load_points`, `point_at` (arc)
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

---

## Infield fill

| Issue | Fix |
|-------|-----|
| Centroid-fan fill leaked on concave tracks (worse on cached composite BG) | Ear-clip triangulation (`fill_infield` / `earcut_triangles`) |
| Stale broken BG after leaving edit | Invalidate `map_bg` when `edit_mode` flips (`last_edit_mode` in `host.rs`) |

---

## Related non-map fixes (same stretch)

- Standings while spectating: no center window; clear lapping/lap_ahead tints when `!in_car`
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
