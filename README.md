# LightSpeed Overlay

A native (non-browser) iRacing **Fuel & Delta** HUD built with PyQt6. It draws a
frameless, always-on-top, click-through panel and polls iRacing's shared-memory
telemetry via `pyirsdk`.

## Requirements

- **Windows** (iRacing telemetry shared memory is Windows-only)
- Python 3.10+
- iRacing running (the overlay shows `iRacing Disconnected` otherwise)

## Setup

```powershell
python -m venv .venv
.venv\Scripts\activate
pip install -r requirements.txt
```

## Project layout

```
run.py                 # entry point for the multi-widget HUD
overlay/               # the application package
  app.py               # HUD controller + telemetry loop (the old sim_hud.py)
  config.py            # config schema, defaults, JSON load/merge, unit helpers
  config_editor.py     # the visual settings editor
  panel.py             # frameless, movable, click-through panel window
  common.py            # iRacing SDK guard + Windows click-through helpers
  layout_store.py      # save/restore per-panel geometry
  demo_data.py         # FakeIRSDK telemetry simulator (--demo)
  svgpath.py           # minimal SVG <path> flattener
  widgets/             # custom-painted widgets
    dash.py  radar.py  relative.py  standings.py
    table.py  track_map.py  light_hud.py  icons.py
tools/                 # standalone helper scripts (not imported by the app)
  fetch_tracks.py  record_track.py  svg_to_track.py
assets/fonts/          # bundled Font Awesome TTF
tracks/                # track shape files keyed by iRacing TrackID
```

## Run

Multi-widget HUD (Timing Tower, Relative, Radar, 2D Track Map, Dash/RPM &mdash;
each an independent, movable, position-saving window):

```powershell
python run.py            # or:  python -m overlay
```

Simple fuel + delta HUD:

```powershell
python -m overlay.widgets.light_hud
```

Flags:

- `--demo` (both) &mdash; run with **simulated telemetry**; no iRacing required. A fake
  12-car field circulates with time-varying pace so the **running order keeps
  changing**, and a tight battle pack around your car weaves past on both sides
  so the **radar regularly shows left / right / both**. Every panel animates.
  Great for layout/styling work.
- `--gallons` (light HUD) &mdash; convert fuel from liters to US gallons (iRacing reports liters).
- `--no-clickthrough` (both) &mdash; "edit mode": windows become interactive so you can
  **drag them**, then relaunch without the flag to lock them.
- `--settings` (run.py) &mdash; open the **visual settings editor** alongside the
  overlay; changes apply live. See [Customize everything](#customize-everything-overlay_configjson).
- `--dump-config` (run.py) &mdash; write a full `overlay_config.json` template and exit.

### Moving and saving panel positions

The panels (Tower, Relative, Radar, Map, Dash) are **independent top-level
windows**. Launch in edit mode, drag each one wherever you like, and its position
is saved automatically to `overlay_layout.json`:

```bash
python3 run.py --demo --no-clickthrough   # drag/resize panels; layout persists
python3 run.py --demo                      # relaunch locked; layout restored
```

In edit mode each panel has a **resize grip in its bottom-right corner**. Resizing
the track map rescales the map; resizing a text panel scales its font. Both
position and size are saved to `overlay_layout.json`. Delete that file to reset to
the default layout.

Quit with `Ctrl+C` in the terminal you launched it from.

### Try it without iRacing

```bash
python run.py --demo --no-clickthrough
python -m overlay.widgets.light_hud --demo --no-clickthrough
```

Demo mode also works off-Windows (handy for development on macOS/Linux), since
it doesn't touch iRacing's Windows-only shared memory.

## Track map

The HUD draws a real 2D track shape (via `QPainter`) with one numbered,
colored dot per car, placed by `CarIdxLapDistPct`. Your car is the yellow dot.

iRacing does not export live X/Y for other cars, so the track *shape* is resolved
in this priority order:

1. **Bundled per-track file** keyed by iRacing's `TrackID` in `tracks/<id>.json`
   or `tracks/<id>.svg` (accurate, and can carry corner-name labels).
2. **Live GPS learning:** if no file exists for the current `TrackID`,
   `TrackPathBuilder` learns the layout from your own car's GPS (`Lat`/`Lon`)
   sampled by lap %. Drive one clean lap and the map fills in ("LEARNING TRACK…
   drive a lap" until ~90% sampled).
3. **Demo mode:** loads `tracks/_demo.json` immediately.

### Building the track library

Three ways to populate `tracks/`:

```bash
# A) Fetch real track maps (no iRacing Data API token needed). Pulls outlines
#    from a public mirror that is already keyed by iRacing's TrackID:
python3 tools/fetch_tracks.py --list            # show available TrackIDs
python3 tools/fetch_tracks.py --id 18           # one track  -> tracks/18.json
python3 tools/fetch_tracks.py --id 18 145 266   # several
python3 tools/fetch_tracks.py --all             # the whole library (~360 tracks)

# B) Record from your own lap (run with iRacing on track, drive one clean lap):
python3 tools/record_track.py        # -> tracks/<TrackID>.json

# C) Convert an iRacing track-outline SVG (drawn in driving direction from S/F):
python3 tools/svg_to_track.py track.svg <TrackID> "Track Name"   # -> tracks/<TrackID>.json
```

`tools/fetch_tracks.py` bypasses the official Data API entirely by downloading from
the community-maintained [`iTelemetry/iracing-tracks`](https://github.com/iTelemetry/iracing-tracks)
mirror, whose `svgs/<TrackID>.svg` files match this overlay's file convention.
Each SVG is flattened into the JSON schema below with its `start_finish` baked
in from the mirror's config, so there's no SVG parsing at runtime. Pass
`--raw-svg` to keep the original `.svg` instead, or `--force` to overwrite.
Files land in `tracks/` keyed by `TrackID`, so they auto-load the next time you
join that track &mdash; no token, no GPS-learning lap.

Track JSON schema:

```json
{
  "track_id": 18,
  "name": "Circuit de Barcelona-Catalunya",
  "start_finish": 0.0,
  "points": [[x, y], "... closed loop, driving direction ..."],
  "corners": [{"pct": 0.07, "label": "1"}, {"pct": 0.15, "label": "Repsol"}]
}
```

Add `corners` entries by hand to get on-map labels like the reference image. The
overlay only ships `tracks/_demo.json`; iRacing's official map SVGs are not
redistributed here, so use one of the tools above to build your own library
(`tools/fetch_tracks.py` is the fastest path).

## Styled panels

All four panels share one visual language (custom-painted, scale with the window).
Transitions are eased frame-to-frame (frame-rate independent), so table rows
**slide** to new slots and fade in when the order changes, and the radar bars and
glows **fade/grow** smoothly instead of popping.

- **Relative** (`overlay/widgets/relative.py`) — cars nearest you on track: status badge,
  position + class stripe, name, license (SR + class letter), iRating, relative
  gap. Player row highlighted; different-lap cars get a stopwatch badge + red gap
  band. Footer: RACE time, lap, incidents.
- **Standings / Tower** (`overlay/widgets/standings.py`) — full running order with gap to
  the leader (`+s` or `-NL` laps down) on the right.
- **Radar** (`overlay/widgets/radar.py`) — directional proximity warning: you are the white
  car in the center, red bars fade outward when a car is alongside
  (`CarLeftRight`), and a yellow-to-red glow appears ahead/behind scaled by how
  close the nearest car is, drawn on a rounded card that matches the dash.
- **Dash / RPM** (`overlay/widgets/dash.py`) — a multi-container dashboard with: a
  horizontal **shift/RPM bar** and a **status** readout in the top container, a
  **primary** block (a small + a big readout) plus two stacked **stat cells** in
  the bottom container, an orange **position box** as its own container, a
  floating **strip pill** with three items, and a floating **center medallion**.
  The medallion has two modes (`dash.center_mode`): `ring` shows the gear with a
  **concentric arc per selected input**; `pedals` shows the gear plus a
  **vertical bar per selected input**. Pick which inputs appear with
  `show_throttle` / `show_brake` / `show_clutch` — the selection drives *both*
  modes (e.g. throttle only, throttle + brake, or all three), and the brake
  arc/bar flashes amber when **ABS** is active. An optional
  thin **delta bar** (`dash.show_delta_bar`) runs across the top — green to the
  right when you're faster than your best, red to the left when slower
  (`delta_bar_range` is the seconds at full deflection). Every content slot —
  `top_right`, `primary_left`, `primary_right`, `stat_left`, `stat_right`, and
  `strip_left`/`strip_center`/`strip_right` — is picked from a metric by
  **dropdown**: speed (unit-aware), RPM, gear, position, lap (x/total), laps
  remaining, lap, fuel, fuel (+laps), fuel laps left, tire wear (L/R),
  incidents, last/best/current lap, delta, track temp, or air temp (or `none` to
  hide it). The shift bar, medallion, position box and delta bar have their own
  show/hide toggles. Speed, fuel and temps follow the global `units` setting
  (metric/imperial). All inputs map to real iRacing telemetry (`Throttle`,
  `Brake`, `Clutch`, `BrakeABSactive`).
- Relative + Standings share `overlay/widgets/table.py`'s `BaseTable` for row rendering.

Data sources: gaps from `CarIdxEstTime`/`CarIdxF2Time`, license/iRating from
`DriverInfo`, lap differences from `CarIdxLap`, radar proximity from
`CarIdxLapDistPct` + `CarLeftRight`, footer from `SessionTime`/`SessionTimeTotal`
and `PlayerCarMyIncidentCount`.

## Customize everything (`overlay_config.json`)

Every visual and behavioral parameter of every widget — colors, fonts, sizes,
column visibility, row counts, radar range, animation speeds, toggles — lives in
config and can be overridden by an **`overlay_config.json`** file next to the
scripts. Defaults match the built-in look, so nothing changes until you edit it.

### Visual settings editor (recommended)

A point-and-click editor exposes **every** key with the right control type
(color pickers with alpha, spin boxes, checkboxes, a track-palette editor),
grouped into tabs per widget:

```bash
python3 -m overlay.config_editor                      # standalone editor
python3 run.py --demo --no-clickthrough --settings    # editor + live overlay
```

It has a dark, modern layout with a **search box** at the top to instantly filter
to any setting by name (e.g. type "player", "pit", "radar"), tabs per widget, and
collapsed-into-cards groups. With **Apply live** on (default), changes repaint the
running overlay instantly so you can tune colors/sizes while watching them. With
**Auto-save** on (default), every change is written to `overlay_config.json`
automatically (debounced ~0.4 s, so dragging a spin box doesn't thrash the disk) —
no need to click Save. Uncheck Auto-save to make changes provisional and use the
**Save** button manually; **Reset** and **Reload** are one click. The editor is
generated from the config schema, so any key you add to the defaults automatically
gets the right control (color picker, dropdown, spin box, checkbox, or palette
editor).

> Why JSON, not SQLite? This config is a small nested document read once at
> startup and written occasionally from the editor. JSON is simpler, diffable,
> and stays hand-editable. SQLite only pays off for large/queried/concurrent
> data, which this isn't.

### Editing the JSON directly

Prefer a text editor? Generate a full template (all keys with their current
values):

```bash
python3 run.py --dump-config      # writes overlay_config.json
```

Then edit only the keys you want. The file is **deep-merged** over the defaults,
so you can keep just the handful of values you changed. Restart the overlay (or
hit Reload in the editor) to apply. Delete the file to return to defaults.

**Colors** accept any of: `"#RGB"`, `"#RRGGBB"`, `"#RRGGBBAA"` (with alpha),
`"rgba(r,g,b,a)"`, or `[r, g, b]` / `[r, g, b, a]` lists.

What you can change, by section:

| Section | Examples of what's customizable |
| --- | --- |
| `font_family` | Global font used by every panel. |
| `text_scale` | **Global** multiplier on all text sizes. Raise to enlarge everything, lower to shrink. Each widget also has its own `text_scale` (below) that multiplies on top of this. |
| `units` | `"metric"` (km/h, °C, L) or `"imperial"` (mph, °F, gal). Drives the unit-aware Dash readouts (`speed`, `track_temp`, `air_temp`, `fuel`) and the Light HUD's fuel. `speed_kph` / `speed_mph` stay fixed to their named unit regardless. |
| `table` | Shared row/cell/header styling for both tables: all colors, license-class colors, iRating cell colors, player/threat row tints, `corner_radius_frac`, `font_scale`, `gap_font_scale`, `row_ease_tau` / `fade_ease_tau` (animation speed), per-cell `widths`, `alt_row_shading`. |
| `relative` | `rows_ahead` / `rows_behind` (cars shown in front of / behind you); `center_on_player`; a **`column_order`** list that controls *which* columns show and *in what order* (add/remove/reorder them from the editor); `columns.stripe` (the position class-color stripe); `pit_mode`; `show_footer`; and a fully mappable **`header`** / **`footer`** (any slot item — see below). |
| `standings` | `rows_ahead` / `rows_behind` (window above/below you when centered), `rows` (size in top-N mode), `center_on_player`, `title`, `show_footer`; its own **`column_order`** list (independent of Relative); `columns.stripe`; `pit_mode`; and a fully mappable **`header`** / **`footer`** (any slot item, plus the standings-only `order_pill` / `title` / `count`). |

Columns are controlled entirely by **`column_order`**: in the settings editor's
**Column order** group you can drag rows to reorder, pick a column from the
dropdown and press **Add**, or select a row and press **Remove**. A column that
isn't in the list isn't shown (and its data isn't computed). The `name` column
always stretches to fill the leftover width.

The available columns are: **badge** (status/pit/lap marker), **position**,
**car_number**, **name**, **license** (class + SR), **irating**, **pit**,
**gap**, **last_lap** and **best_lap**. Tables start with a sensible subset; add
the rest from the editor as needed. Every column maps to real iRacing telemetry
(`CarIdxLastLapTime`, `CarIdxBestLapTime`, DriverInfo, etc.).

#### Per-widget text size

Besides the global `text_scale`, each text-bearing widget has its **own**
`text_scale` that multiplies on top of the global one, so you can make one panel's
text bigger or smaller without touching the rest:

| Key | Scales |
| --- | --- |
| `relative.text_scale` | the Relative table |
| `standings.text_scale` | the Standings tower |
| `dash.text_scale` | the Dash / RPM widget |
| `map.text_scale` | track-map corner labels + car numbers |
| `light_hud.text_scale` | the simple Fuel/Delta HUD |

Effective size = `text_scale` (global) × `<widget>.text_scale`. Both default to
`1.0` per widget, so nothing changes until you set one. Example — bigger Relative,
smaller Standings:

```json
{ "relative": { "text_scale": 1.3 }, "standings": { "text_scale": 0.85 } }
```

#### Header / footer sections

Both tables have a `header` **and** a `footer` (the standings footer can be
hidden with `standings.show_footer`). Each edge is split into **three sections**
— `left`, `center`, `right` — and you map any item into each (or `none`). In the
settings editor each section is a dropdown. The same item set is available in
every slot, so you can pin whatever you like wherever you like:

| Item | Shows |
| --- | --- |
| `sof` / `class_sof` | Strength of field (whole field / your car class) |
| `position` / `class_position` | Your overall / in-class position (`p/total`) |
| `session_time` | Time remaining in the session |
| `race_time` | Session time elapsed / total |
| `lap` | Current lap / estimated total |
| `incidents` | Your incident count (`11x`) |
| `track_name` | Track + layout (`Watkins Glen - Boot`) |
| `track_temp` / `air_temp` | Track / air temperature (unit-aware) |
| `best_lap` / `session_best` | Your best lap / the fastest lap in the lobby |
| `local_time` / `sim_time` | Real-world clock / in-sim time of day |
| `cpu` / `mem` | This machine's CPU% / memory% (needs `psutil`) |
| `order_pill` / `title` / `count` | Standings-only decorations |

```json
{
  "relative": {
    "header": { "left": "sof",       "center": "class_sof", "right": "position"  },
    "footer": { "left": "race_time", "center": "lap",       "right": "incidents" }
  },
  "standings": {
    "show_footer": true,
    "header": { "left": "order_pill", "center": "title",        "right": "count" },
    "footer": { "left": "track_temp", "center": "session_time", "right": "air_temp" }
  }
}
```

Set a section to `none` to leave it empty. An item that isn't placed in any
section isn't drawn (and isn't computed). `cpu` / `mem` need the optional
`psutil` package (see `requirements.txt`); without it they show `--`. Ping /
connection quality isn't included because iRacing's SDK doesn't expose it.

#### Labels vs. icons (Font Awesome)

Readouts use **Font Awesome icons** where a metric has one mapped. The bundled
font is `assets/fonts/fa-solid-900.ttf` (Font Awesome 6 Free, Solid). The dash
draws each slot's icon automatically next to its value (e.g. speed → a gauge,
fuel → a pump, lap time → a stopwatch).

The tables can show a **Font Awesome icon instead of a text label**, toggled per
header/footer section. In the settings editor these are checkboxes; in JSON they
are boolean maps that mirror the slot structure:

| Toggle | Controls |
| --- | --- |
| `relative.header_icons` / `relative.footer_icons` | the icon-vs-label choice for each `relative` header / footer section |
| `standings.header_icons` / `standings.footer_icons` | the same for the `standings` header / footer sections |

```json
{ "relative": { "header_icons": { "left": true, "center": false, "right": true } } }
```

If the font is missing or a metric has no mapped icon, that item falls back to
its text label automatically.

Both tables keep **you in the center row** by default (`center_on_player`), and
you choose **how many cars show ahead and behind** independently via `rows_ahead`
/ `rows_behind`. The Relative box pads with blank rows when there aren't that many
cars on a side (e.g. when you're leading), and the Standings tower shows a window
of the running order (`rows_ahead` + you + `rows_behind`) centered on your
position. Set `center_on_player` to `false` for a top-anchored Relative / top-N
Standings (`rows`) instead.

#### Pit column

Add `pit` to either table's `column_order` (via the editor's Column order group)
to show pit info per car. `pit_mode` (per table) selects what it shows:

| `pit_mode` | Shows | Example |
| --- | --- | --- |
| `laps_since` | laps run since their last stop | `3L` |
| `time_since` | time elapsed since their last stop | `4:12` |
| `at_lap` | the lap they last pitted on | `L18` |
| `at_time` | the race clock when they last pitted | `21:40` |

A car currently on pit road shows `PIT`; a car not yet seen pitting shows `—`.
iRacing exposes no per-car "last pit" value, so the overlay watches
`CarIdxOnPitRoad` (falling back to the pit-stall track surface) and records each
stop itself — meaning history starts when the overlay launches (or when you
add the column). Tracking only runs while the pit column is shown.
| `radar` | `range_pct` (ahead/behind detection window), `ease_side_tau` / `ease_glow_tau`, car/red/yellow/axis/nose colors, element `sizes` (car, bars, glow, nose), `show_nose`, `show_axis`. |
| `map` | Asphalt/outline/infield/player/corner colors, the car-dot `palette`, `asphalt_width`, `outline_width`, and `show_infield` / `show_corners` / `show_start_finish` toggles. |
| `light_hud` | `font_px`, text/accent/accent2/background colors for the simple fuel+delta HUD. |

Example `overlay_config.json` (red text in tables, a trimmed/reordered Relative,
5 standings rows with a pit column, wider radar range, neon-green light HUD):

```json
{
  "table": { "colors": { "text": "#ff4444" } },
  "relative": {
    "column_order": ["position", "name", "gap"],
    "show_footer": false
  },
  "standings": {
    "rows": 5, "title": "GRID",
    "column_order": ["badge", "position", "name", "license", "irating", "pit"]
  },
  "radar": { "range_pct": 0.05 },
  "light_hud": { "colors": { "accent": "#00ff66" } }
}
```

Note that `relative.column_order` and `standings.column_order` are
**independent**, so you can show iRating in the Standings tower while hiding it
in the Relative box.

Honest caveats: the radar's lateral placement is a heuristic (iRacing reports
only an aggregate `CarLeftRight`, not each rival's lateral offset); longitudinal
placement is real.

## iRacing API corrections baked into the HUD

The widely copied multi-widget template gets several telemetry details wrong;
this project fixes them:

| Template claim | Reality |
| --- | --- |
| Blind spot from `TrackToPlayerCarIdx` | No such variable. Use **`CarLeftRight`**. |
| `CarLeftRight` is a bitfield `0x02/0x04/0x06` | It's an **enum** (2=Left, 3=Right, 4=Both, 5=2-left, 6=2-right). |
| Standings/Relative "sorted in production" | Actually computed here: standings from `CarIdxPosition`, relative from `CarIdxEstTime` wrapped by the player's est lap time. |
| `import pyirsdk` | The module is **`irsdk`**. |
| Re-read `DriverInfo` every tick | Session YAML is cached and refreshed ~2x/sec. |
| Click-through *and* drag-to-position | Mutually exclusive; use `--no-clickthrough` to drag. |

## Honest notes on performance claims

The "uses <40 MB RAM / <1% CPU / matches frame timing" framing in many copies of
this template is marketing, not measurement:

- A PyQt6 process is genuinely lighter than a Chromium-based overlay, but actual
  RAM/CPU depends on your widgets and update rate. Measure with Task Manager
  rather than trusting a fixed number.
- `QTimer` is a regular software timer. The 16 ms interval is chosen to match
  iRacing's **60 Hz telemetry tick**, not your monitor's frame timing. It is not
  vsync-locked.
- Real click-through on Windows requires the `WS_EX_LAYERED | WS_EX_TRANSPARENT`
  extended window styles. Qt's `WA_TransparentForMouseEvents` alone is unreliable
  for frameless/translucent windows, so this project sets the Win32 styles via
  `ctypes` once the native window exists.
- `FuelLevel` is in **liters**. Use `--gallons` if you want it converted.

Performance work that *is* real (and keeps the animations):

- **Idle repaint skipping.** Each widget repaints only when its data changes or
  an animation is still settling. A steady widget drops toward 0 FPS instead of
  redrawing 60x/sec, but slides/fades/eases still play out fully whenever values
  move (verified: ~52 paints/sec while the field is moving, ~0 when static).
- **Color + font caching.** Parsed `QColor`s and `QFont`s are memoized by value,
  so hot paint paths don't re-parse `#rrggbbaa` strings or rebuild fonts every
  frame. Caches are keyed by the literal value, so changing a setting just makes
  a new key (no stale values), and they're size-capped to bound memory.
- **No per-tick YAML re-parse.** The estimated lap time is cached with the driver
  list (refreshed ~2x/sec) instead of re-parsing the `DriverInfo` session YAML on
  every 16 ms tick.

## Credits

Icons use [Font Awesome 6 Free](https://fontawesome.com) (Solid), bundled at
`assets/fonts/fa-solid-900.ttf`. The font is licensed under the SIL OFL 1.1 and
the icons under CC BY 4.0.
# racing-overlay
