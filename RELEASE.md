# Changelog

This file is the source of truth for the app version and release notes. The CI
release workflow reads the **topmost** `## <version>` section below: that version
becomes the git tag / installer version, and the bullet points become the GitHub
Release notes. To cut a release, add a new section to the top and push.

## 1.69.0 - 2026-07-20

- **Rust-only app.** Python package, tests, tools, and PyInstaller packaging
  removed; the product is `gridglance-overlay` only.
- **Settings + tray.** Settings live in the Rust/egui host; system tray
  Settings / left-click / double-click reopen Settings; close Settings can
  minimize to tray.
- **No console on release builds.** Installed `gridglance-overlay` runs without
  a terminal window.
- **Panel styles.** Per-widget Data vs Elegant layouts with content-fit sizing;
  denser Elegant widgets (fuel, sector, delta, and related panels).

## 1.68.0 - 2026-07-12

- **Rust overlay (hybrid).** Race widgets run in a Rust/egui process
  (`overlay-rs`); settings and Track Scan stay in Python and talk over local
  JSON-RPC (port 19847). When `gridglance-overlay` is built, it is the default
  backend; use `--python` for the legacy PyQt overlay.
- **IPC live apply.** Settings Apply Live / Save push CFG to the Rust host;
  Track Scan authoring commands (`map.set_pit_edit`, etc.) go over the same
  channel via `RemoteOverlay`.

## 1.67.6 - 2026-07-11

- **Fix: iRating projection rounding.** Projected change uses round(start +
  change) − start (half away from zero), matching the community calculator so
  values are less often off by one vs the official result.

## 1.67.5 - 2026-07-11

- **Fix: pit-lane map dots.** Cars on pit road move along the full pit polyline
  instead of sticking at the end (lane mapping + even path progress).

## 1.67.4 - 2026-07-11

- **Fix: telemetry crash.** `slot_in_use` accepts multiple header/footer keys
  again, so checking weather / incident limit / race split together no longer
  TypeErrors every tick.

## 1.67.3 - 2026-07-11

- **Import drivers from results.** Professional drivers and driver-group members
  can import display names from an iRacing `event_result` JSON file; duplicates
  (name or alias) are skipped.
- **Track Scan settings clarity.** Pit edit, clear, and save are grouped with
  short captions; metadata and HTML import copy is tightened.

## 1.67.2 - 2026-07-11

- **Fix: garage while spectating.** Profile switches on IsGarageVisible as well
  as IsInGarage, so the garage layout applies when you are not in the car.
- **Fix: spectator widgets.** Focus car uses CamCarIdx (and DriverCarIdx) when
  PlayerCarIdx is invalid (-1); Relative/Map keep painting; radio tower shows
  transmitters without a local ego car.

## 1.67.1 - 2026-07-11

- **Fix: mid-race restart.** Opening the app during a session loads the map
  without waiting on car telemetry, Relative keeps painting with a lap-time
  fallback, and Standings uses grid positions when live ranks are still zeros.
- **Garage / race context follows the sim.** Settings preview pin no longer
  blocks IsInGarage auto-switching the live overlay profile.
- **Profile switch spinner.** Race ↔ garage context changes show the same
  loading spinner as preset switches.

## 1.67.0 - 2026-07-11

- **Race split slot.** Relative / Standings header and footer can show
  registration split (`Split N`) when resolvable from WeekendInfo or optional
  iRacing results credentials; demo shows Split 2.
- **Dropdown ordering.** None / empty options sort to the top of settings
  combos; the Default preset stays first in the preset picker.
- **Driver groups.** App settings: create personal groups (league mates),
  pick icon + color, add members/aliases. Matching drivers get that badge in
  Relative and Standings (pro star still wins).
- **Radio tower badges.** Speakers show the same pro / group icons; the tower
  also appears when you transmit even without a race position.

## 1.66.3 - 2026-07-11

- **Save pit.** Track Scan gains Save pit — updates pit geometry locally and to
  the cloud even when the TrackID is already in the shared library (Save track
  remains first-publish only).
- **Entry links to pit road.** Drawing a new entry onto an existing pit road
  auto-joins at the pit-road start.
- **Dash primary right-align.** Equal columns kept; icon+value packs are
  right-aligned within each column.

## 1.66.2 - 2026-07-11

- **Dash primary columns.** Left/right primary metrics sit in two equal columns,
  each left-aligned (replaces the packed right-aligned pair).
- **Shift blink timeout.** Flashing stops after 3 seconds at redline
  (`shift_blink_max_sec`); resumes after RPM drops and climbs again.
- **Fuel calc PIT strip.** Strip keeps its original weighted height; unused
  stats space is no longer poured into the timeline bars.
- **Flag title / subtext spacing.** Slightly more vertical gap between the flag
  label and context line on the dash flag bar.

## 1.66.1 - 2026-07-11

- **Fix: preset loading modal.** Loading UI is preset-only (not race/garage),
  dismisses after the switch, and no longer sticks on launch with Settings open.
- **Animated preset spinner.** Replaces the frozen indeterminate progress bar
  with a timer-driven spinner that keeps moving while the preset applies.

## 1.66.0 - 2026-07-11

- **Profile switch loading.** Preset / race–garage context switches show a modal
  “Loading profile…” dialog until layout + deferred repaint finish (overlay and
  standalone Settings).
- **Global Font drives numbers.** Empty Tabular font now inherits Font (no
  longer SF Mono / Consolas); map (and related) labels use the same font
  helpers. Settings label: “Same as Font”.
- **Dash primary hugs the ring.** When both left and right primary metrics are
  set, the pair right-aligns into the ring gap; a single active slot stays
  centered.
- **Fuel calc layout.** Stats grid shrink-wraps so the PIT strip sits just under
  the table instead of under empty weighted space.
- **Smaller pro-driver star.** Relative / Standings badge is ~0.32 row height
  with a tighter gap.

## 1.65.0 - 2026-07-11

- **Map pit without a lane.** If pit display is off or there is no saved pit
  polyline, the map no longer draws a fake pit-span lane. Cars on pit road stay
  on the racing line, offset inward and greyed.
- **Ahead / behind by race position.** Map traffic markers target the cars one
  place ahead and behind in the standings (not nearest by lap %). If that car is
  in the pits or off-track, the marker is hidden — no fallthrough.
- **Dash primary layout.** Left/right primary metrics use the same value size
  and are centered in the strip; a single active slot (or single stat) is
  centered in its section.
- **Export / import presets.** Settings preset bar can export and import a
  shared `.ggprofile.json` so layouts can be passed between installs.
- **Professional drivers (authors).** With Mongo write access, App settings can
  manage a shared pro-driver list (name + aliases). Matching drivers show a star
  badge and accented name in Relative and Standings.
- **Fix: wrong name through qual into the race.** DriverInfo cache now clears on
  session change; player name prefers `DriverCarIdx` / `DriverUserID` so a stale
  CarIdx map cannot label you as someone else until lap 1.

## 1.64.0 - 2026-07-10

- **Dash delta reference (independent).** Dash settings gain a Reference lap
  control (`delta_bar_mode`) for the dash delta bar and delta metric slot,
  separate from the standalone Delta Bar widget.
- **Dash metric icons.** Metrics that used text labels (RPM, OIL, AHD, etc.)
  now show Font Awesome icons instead; numeric values stay.
- **Fuel calc labels.** Stats grid rows are MAX/MIN (was HIGH/LOW); USAGE and
  REFUEL headers and cell values no longer append Gal/L (gauge/add box still
  show units).
- **Smoother map and tables with large fields.** Map car labels and layout
  work are cheaper at ~40 cars; animation repaints are capped ~30 Hz. Dense
  standings/relative tables snap sooner and ease faster.
- **Independent other-car map dots.** New Other cars dot size slider; My car
  dot size only affects you.

## 1.63.3 - 2026-07-09

- **Fix: live delta bars frozen in test drive.** The overlay read
  `LapDeltaToSessionBest`, but iRacing exposes `LapDeltaToSessionBestLap` — so
  both the standalone delta bar and dash strip always showed `--.--` while the
  in-sim delta worked. A centralized `read_lap_delta()` reader now uses the
  correct SDK keys, honors `*_OK` validity flags when present, and feeds both
  widgets.
- **Lap-% for delta consumers.** `CarIdxLapDistPct` is read whenever a delta bar
  is enabled (standalone or dash), so pit-hold release and sector timing work
  even when other widgets are hidden.

## 1.63.2 - 2026-07-09

- **Fix: delta bars stuck with no time or bar movement.** Pit-hold logic could
  stay latched after leaving pit road (especially late in a lap), keeping both
  the standalone delta bar and the dash strip at `--.--` / empty. Hold now
  clears on lap rollover and after a safety timeout off pit road.
- **Delta telemetry hardening.** SDK delta values are coerced via `as_float()`
  so pyirsdk/numpy scalars are not rejected as invalid.
- **`last_lap` mode without laptime log.** Last-lap reference time is tracked
  whenever the delta bar is active, not only when the laptime log widget is shown.

## 1.63.1 - 2026-07-09

- **Fix: map car dots missing.** Dual-pit-lane added a 13th field to each car
  tuple, but map painting still unpacked only 12 values — causing a paint error
  that hid every car dot (player and opponents). `_draw_cars` now handles the
  full tuple; regression test added.

## 1.63.0 - 2026-07-09

- **Demo mode paint stability.** Map painting tolerates partial `map.colors`
  overrides (no more `KeyError` on missing keys like `pit_car` or `corner_text`).
  Uncaught exceptions now print a full traceback to the terminal, and telemetry
  tick failures log which widget was updating.
- **Thread-safe settings workers.** HTML loop import and Community demo track
  save marshal results back to the GUI via Qt signals instead of calling
  `QTimer.singleShot` from background threads (fixes timer warnings and
  intermittent crashes).
- **Demo cloud fetch dedup.** Repeated requests for the same TrackID while a
  cloud fetch is already in flight are skipped, reducing terminal spam and
  load when demo track settings change.

## 1.62.0 - 2026-07-09

- **Dual pit lanes (Bristol-style).** Tracks can now carry an optional second pit
  road with its own entry, lane, and merge. Track Scan adds a **Lane 1 / Lane 2**
  toggle; lane 2 is optional at save time and draws in green/cyan while editing.
  On the map, pit cars pick the lane whose lap-% interval matches (or the nearer
  pit path when intervals overlap). Single-lane tracks are unchanged.
- **Skip save when track is already in cloud.** Save loop / Save track no longer
  writes locally or uploads when that TrackID already exists in the shared library
  — you get a flash hint and the save is skipped instead of overwriting.

## 1.61.0 - 2026-07-09

- **Map layout on preset switch.** Switching profiles no longer flashes the map at
  the wrong size for a moment — layout applies first, the static cache is
  invalidated, and repaints wait until panel geometry has settled.
- **Clear pit phase.** Track Scan uses a phase dropdown (Entry / Pit road / Merge
  with point counts) plus **Clear selected**. Saved pit preview geometry no longer
  ghosts over what you are editing, and clearing a phase wipes both edit buffers
  and the live preview.
- **Entry ↔ pit road link.** Optional yellow entry end and pit road start stay
  joined (like pit road end and merge start): switching to Pit road seeds from
  entry, the first road click continues from entry end, and a shared handle drags
  both points.
- **My session best lap.** New **My session best** metric for dash slots and
  standings/relative header/footer — your fastest lap this session
  (`CarIdxBestLapTime`), distinct from personal best and lobby session best.
  Slot labels clarify personal best vs session best (lobby).

## 1.60.9 - 2026-07-08

- **Lower CPU and memory use.** Widgets repaint less often without dropping
  animations: dash and radar ease via self-scheduled paint loops, inputs skip
  no-op updates, and dash/sector/flags/tire/ERS/leaderboard feeds dedupe
  unchanged telemetry before repainting.
- **Map static cache.** Track asphalt, pit, zones, corners, and start/finish
  render to a cached pixmap; only car dots, traffic markers, wind, and overlays
  redraw each frame. Car targets use a small lap-% epsilon so easing still runs
  smoothly.
- **Targeted settings repaints.** Live config tweaks update only the affected
  widget section instead of all 19 panels; global font changes still refresh
  everything.
- **Demo telemetry reuse.** Fake iRacing arrays are built once per tick and
  reused across reads, cutting allocations in layout/demo mode.

## 1.60.8 - 2026-07-08

- **Map player on pit lane.** Your car dot stays obvious on pit road and during
  pit exit: full opacity, class color, and the same glow/ring as on track instead
  of blending into gray pit traffic.

## 1.60.7 - 2026-07-08

- **Pit lane map editing.** Enabling pit edit (or corner / start-finish edit) makes
  the map panel accept mouse and scroll again while the rest of the overlay stays
  click-through — zoom, pan, and drag handles work without `--no-clickthrough`.
- **Per-phase pit camera.** Auto-fit focuses on the active phase (entry, pit road,
  or merge) so a distant optional entry no longer shrinks handles or breaks zoom.
- **Clear pit controls.** Track Scan adds **Clear all pit** (edit buffers + saved
  preview) and **Clear phase** (wipe only the selected segment).

## 1.60.6 - 2026-07-08

- **Settings App Launch card.** Opening Settings → App no longer crashes: Launch
  toggle tooltips now pass the required `help_for` arguments.

## 1.60.5 - 2026-07-08

- **Start overlay on launch.** Settings → App → Launch can start widgets as soon
  as GridGlance opens (same idea as `--start`), without waiting for Start Overlay.
- **Start at Windows login.** Optional Startup-folder shortcut so GridGlance runs
  when you sign in. If overlay-on-launch is on, login uses `--no-settings` so
  Settings does not pop up unprompted. Uninstall clears the shortcut.
- **Single-instance app.** A second launch (taskbar / desktop double-click)
  activates the running tray app and opens Settings instead of starting another
  process.
- **Launch update Yes fix.** Accepting an update prompt at startup now runs on
  the GUI thread (same pattern as Settings → Check for Updates), so Yes no
  longer fails while the settings-page updater worked.
- **Optional pit entry (Track Scan).** Yellow Entry phase is optional; saves only
  write `pit_in` when you draw one (no auto-seed from pit road). HTML import
  skips degenerate entry stubs.
- **Preset auto-switch.** Empty/unknown car at connect no longer falls through
  to Default and desyncs the Settings preset combo from the live overlay.

## 1.60.4 - 2026-07-08

- **Dual race/garage layouts.** Each preset now keeps separate on-track and
  in-garage widget positions. Moving a panel in the garage no longer overwrites
  its track placement; switching On track / In garage (or the live garage
  context) applies the matching layout.
- **Map car numbers.** Stabilized number size (no double text-scale, pixel-snapped
  labels, text antialiasing, consistent player font in pits) so numbers no longer
  flicker while cars ease.
- **Map pit transitions.** Cars on pit road stay on the pit path across the
  start/finish gap (no racing-line blink). Competitors remain visible through the
  brief APPROACHING_PITS window after leaving pit road.
- **Dash left fonts.** Shift bar and primary readouts size against the base ring
  clearance so mph–iRating spacing no longer shrinks left-side text.
- **Pit advisor visibility.** Quiet during the green-resume window after yellow
  clears. The panel fully hides when there is nothing actionable to show (no empty
  card chrome).

## 1.60.3 - 2026-07-07

- **Standings inactive rows.** Drivers in the garage or disconnected are greyed
  out in the standings table (muted text + `inactive_row` tint). On-track, pit,
  and off-track rows are unchanged.
- **iRating projection fix.** Registered field size now includes DNS and
  non-starters so projected deltas match iRacing more closely mid-race.
- **Dash delta bar after pit exit.** Whole-lap delta is held through pit road
  and until the first sector after exit so stale SDK delta does not flash on
  rejoin.
- **Dash layout polish.** Flag and delta bars span the full dash width; the
  center ring is centered on the full panel (including the position box); mph
  spacing matches the ring-to-iRating gap on the stats side.

## 1.60.2 - 2026-07-07

- **Fuel calc units and clarity.** Add box and USAGE/REFUEL stats now respect
  **Settings → Units → Imperial** (gallons). Column headers show units; burn
  rows are labeled **HIGH/LOW** instead of MAX/MIN so laps-on-fuel is easier
  to read.
- **Fuel calc history fix.** Pit advisor no longer wipes per-lap burn history on
  yellow→green; fuel projections stay stable across cautions.
- **Map speaking z-order.** Drivers on team radio render above overlapping car
  dots so the green ring and mic badge stay visible.
- **Pit advisor pits open.** Uses iRacing **PitsOpen** telemetry instead of
  inferring closed pits from the caution-waving flag — fixes HOLD advice when
  pits are open under yellow.
- **Pit advisor green-run gating.** Caution outlook and “caution may save a stop”
  nudges only appear on green after **green_run_caution_bias_laps** (default 15).

## 1.60.1 - 2026-07-07

- **Performance panel GPU on Windows.** Hardened PDH GPU engine sampling
  (`PdhExpandWildCardPathW`, machine-prefix paths) and added **nvidia-smi**
  fallback when PDH returns nothing — fixes GPU stuck at 0% on NVIDIA systems.
- **Performance panel network row.** Offline/stale `ChanLatency=0` no longer
  blocks WiFi fallback; network shows OS WiFi signal (or `--` when wired) instead
  of a misleading lone `0 ms`. Windows also tries **netsh** when wlanapi is
  unavailable.

## 1.60.0 - 2026-07-07

- **Pit advisor widget.** New Session panel recommends when to pit on green and
  under caution — fuel window, tire wear, undercut/cover gaps, field pitting,
  reentry traffic, and caution outlook in one call (PIT NOW, PIT NEXT LAP, STAY
  OUT, HOLD, MARGINAL).
- **Opponent tire inference.** Tracks each car's pit stops and stint length to
  guess who is out of tire sets; recommends strategic early pits when you can
  gain net positions by stopping before bankrupt cars ahead are forced in.
- **Measured pit loss.** After your stops, pit duration EMA refines undercut and
  position-cost math; optional splash-pit detection skips fuel-only stops when
  tuning opponent tire counts.
- **Pit menu hard gate.** PIT NOW / PIT NEXT LAP waits until fuel and/or tires
  are queued on the iRacing pit menu when the option is enabled.
- **Race settings accordion.** Pit Advisor settings split **Race** (tire set
  limit, pit loss, stint length, wetness — per session/track) from **Content**
  (display and strategy thresholds).

## 1.59.0 - 2026-07-07

- **Performance panel widget.** New Session panel shows machine CPU, memory, and
  GPU usage, iRacing frame rate, and network status. Online sessions display
  channel quality and latency; otherwise the panel falls back to OS WiFi signal
  on Windows and Linux.
- **Performance panel polish.** Right-aligned values, thin usage bars for
  CPU/MEM/GPU, full text labels, and an optional Font Awesome icon mode.
- **GPU % table slot.** Standings and relative header/footer slots now support
  **GPU usage %** alongside existing CPU and memory readouts (Windows PDH;
  optional `nvidia-smi` on Linux).

## 1.58.0 - 2026-07-06

- **Radio tower widget.** New Session panel shows who is on team radio right
  now, with race position and driver name/car number in one line (e.g.
  `5 - Logan Troyer #12`). The row highlights green while they transmit; the
  panel hides when the channel is silent.

## 1.57.0 - 2026-07-06

- **IMS scoring-pylon leaderboard strip.** Restyled the leaderboard strip like
  the Indianapolis scoring pylon: black background, dot separator, white position
  numbers, and amber 7-segment LED car numbers with discrete bulb gaps.
- **Full-field standings tower.** `rows: 0` (the new default) shows the entire
  classified field; set a positive row count to cap at top N. LAP, MPH, name, and
  gap rows are optional and off by default.
- **Tighter strip layout.** Position numbers are right-aligned in a column sized
  to the widest digit; car numbers get the reclaimed width.
- **Panels stay on screen.** Saved panel geometry is clamped when a widget would
  open mostly off-screen; preset changes and show events re-check visibility.
- **Live track load reliability.** Cloud track fetch retries after failures, stale
  local maps load immediately while refresh runs in the background, and alias
  TrackIDs match session tracks from MongoDB.

## 1.56.1 - 2026-07-06

- **Smoother pit entry on ovals.** When `OnPitRoad` engages on schematic tracks
  like Indianapolis, your car dot now eases onto the pit entry line instead of
  freezing on the racing line and jumping to the pit lane.

## 1.56.0 - 2026-07-06

- **Track ID aliases.** Authors can list alternate iRacing TrackIDs on a saved
  map (**Track Scan → Track metadata → Also used for Track IDs**) so layout
  variants share one track file and cloud document — e.g. Echo Park Atlanta
  current (447) and 2008 layout (53).

## 1.55.1 - 2026-07-06

- **Map pit display fix.** Passing the pit entry on the racing line no longer
  pulls your car dot onto the pit route or grays it out. Pit entry blending only
  applies when you are approaching or committed to pit road.

## 1.55.0 - 2026-07-06

- **Car direction after import.** V2 HTML import now trusts the start/finish arrow
  for loop winding instead of forcing counter-clockwise geometry, fixing car dots
  that crawled backwards along the track on some layouts.
- **Persist map orientation on save.** Rotation and mirror settings from
  **Settings → Map** are stamped into saved track JSON so cloud reloads keep the
  author's corrected orientation.
- **Smoother pit entry and rotation changes.** Pit entry eases from the racing
  line onto the pit route; toggling map rotation or mirror clears stale car
  animation so dots no longer teleport.

## 1.54.0 - 2026-07-06

- **Settings wiring audit.** Every visible setting now either applies live or is
  hidden when it does not affect that widget. Row dividers work on leaderboard
  strip, pit board, and weather panel; removed from widgets without row lists
  (radar, delta bar, flags, inputs, and others).
- **Table fade animation.** `fade_ease_tau` now controls row fade-in when cars
  enter the relative/standings tables.
- **Live apply fixes.** Garage profile column-order edits refresh immediately;
  font family and scale changes clear the font cache on every config update.
- **Wiring tests.** Automated checks guard settings-to-widget wiring so orphan
  options are caught in CI.

## 1.53.0 - 2026-07-06

- **Default styling.** Map asphalt width 12 and outline width 6; panel corner
  radius defaults to 0 (square corners) for tables and widgets.
- **Settings Quit.** Quit button fully exits the app (not just minimize Settings).
- **Setting descriptions.** Every settings row has a tooltip and ? help button
  explaining what the option does and what changes when you adjust it.

## 1.52.0 - 2026-07-06

- **Pit edit zoom.** Placing the first pit road point no longer auto-zooms the
  map; the view stays at full-track scale until you wheel-zoom or middle-click pan.

## 1.51.0 - 2026-07-06

- **Cross-device track orientation.** Saved tracks now include `map_rotation` and
  `map_mirror` so every device renders the same orientation as the HTML import.
  Author saves stamp `updated_at`; live sessions refresh stale local caches from
  the cloud instead of keeping an old geometry file.

## 1.50.0 - 2026-07-06

- **HTML import orientation.** V2 loop import now preserves the members-page map
  orientation (no vertical flip). Importing a track resets `map.rotation` to 0
  and `map.mirror` off so the overlay preview matches the HTML. Re-import tracks
  saved with older imports if the layout looked upside-down.

## 1.49.0 - 2026-07-06

- **Pit edit middle-click pan.** In Track Scan pit authoring, middle-click drag
  (mouse wheel click) pans the map without adding pit points. Shift+left-drag
  still works.
- **Laptime log paint fix.** Fixed a crash when the laptime log drew temperature
  rows (`icons.draw_thermo` was missing) and corrected row vertical positioning
  so lap rows render in the right place.

## 1.48.0 - 2026-07-06

- **Loop-only track save.** Track Scan adds **Save loop** to upload racing-line
  geometry without pit lane, so you can author on one device and add pit road +
  merge on another with **Save track**. Loop-only cloud uploads clear stale pit
  fields in Mongo.
- **Session demo preview.** Uploading a track previews it on the demo map for the
  current session only; the shared Community demo track returns on next launch.

## 1.47.0 - 2026-07-06

- **Shared Community demo track.** Authors with MongoDB write access can set a
  shared demo track ID in **Settings → App → Community demo track**. Every user
  running `--demo` loads that map from the cloud library (with local cache).
  Rotate the featured track weekly by changing one admin value. Demo telemetry
  (session info, turn count) follows the loaded map. `--demo-track` is deprecated
  and ignored when a shared ID is configured.

## 1.46.0 - 2026-07-05

- **Weak widget customization.** Tire panel, pit board, weather panel,
  leaderboard strip, ERS/hybrid, delta bar, and sector timing now match the
  richer widgets: customizable titles, corner radius, row height, and
  section toggles in Settings. Pit board banner text, ERS/hybrid labels, and
  the hybrid no-data message are editable; leaderboard position and gap columns
  can be hidden independently.

## 1.45.0 - 2026-07-05

- **Widget UI polish.** Shared panel chrome (dark cells, section headers, metric
  rows, status chips) across weather, pit board, leaderboard strip, ERS/hybrid,
  and tire panels; consistency pass on sector timing, tables, delta bar, fuel
  calc, and the track map placeholder. Edit-mode preview skeletons when panels
  have no live data.
- **Settings UX.** Widget list scrolls when the sidebar overflows; accordion
  accents, row toggle alignment, per-page hints, and search placeholder polish.
  Fixes a crash opening Settings from the CollapsibleSection init order.
- **Map car labels and motion.** Car numbers use bold stroked on-dot text for
  quick reading without extra pill chrome; leader/ahead/behind skip duplicate
  numbers since their marker icons already label those cars. Car dots ease
  smoothly along the track (lap-% interpolation with start/finish wrap); traffic
  marker lines and icons stay pinned to the smoothed dot positions.

## 1.44.0 - 2026-07-05

- **Telemetry expansion (widgets 6–12).** Lap log, fuel calc, sector timing, lap
  compare, delta bar, inputs, and flags gained optional columns, metrics, and
  display modes (sector splits per lap, fuel/incident/tag columns, personal-best
  deltas, live burn and pit-window hints, sector deltas and map highlight,
  handbrake/torque/TC traces, incident-limit and pit-limiter warnings, and more).
- **Five new widgets (off by default).** Tire panel (4-corner wear/temp/pressure),
  pit board (requested services, compound, fuel, fast repairs, optional pressures),
  weather panel (skies, rain, temps with trend, wind), leaderboard strip (top-N
  with gaps), and ERS/hybrid readout for supported series.
- **Richer dash and tables.** Dash slots now include four-tire wear/temps, fuel
  %, burn rate, optimal/best deltas, time remaining, class position, team/limit
  incidents, in-car adjustments, engine/oil/water/voltage, gap ahead/behind,
  and a compact corner-loss strip. Relative and standings tables add class
  position, on-track status, car flags, lap count, gap-to-leader/ahead, and
  closing-rate columns; radar can show car numbers, closing tint, and a clear
  timer.
- **Lower CPU, same animations.** Telemetry reads and widget updates are gated
  on what you actually show — hidden widgets and unused columns do no work.
  Redundant repaints are skipped where safe; dash pedal easing, delta-bar
  deflection, inputs scroll, and radar glow are unchanged.
- **Correctness fixes.** Sector timing advances when only the lap log sectors
  column is on; fuel-per-lap tracking runs for the lap log fuel column even when
  the fuel widget is hidden; lap compare data stays fresh for the dash corner
  strip when the full lap compare panel is off.

## 1.43.1 - 2026-07-02

- **Fix overlay startup crash.** Starting the overlay in edit mode without iRacing
  connected no longer crashes — demo telemetry seeding now uses the synthetic SDK
  instead of the disconnected real one.
- **Table color fallbacks.** Speaking-row colors fall back to defaults when an
  older saved config is missing the new keys.

## 1.43.0 - 2026-07-02

- **Smoother row and marker motion.** Relative/standings rows slide without
  flicker (stable placeholder keys, correct draw order, snap on big jumps).
  Map traffic markers ease toward their targets instead of jumping every tick.
- **Radio speaker visibility.** Green badge, row highlight, and driver name tint
  in the timing tables; map cars keep their number with a ring and mic badge
  while transmitting.
- **Map car colors.** Same-lap traffic uses one configurable color (default
  purple); blue and red are reserved for lapped cars and cars lapping you.
- **Grid session time.** Footer session clock counts down during qualifying/grid
  when iRacing reports SessionTimeRemain as unknown.
- **Performance.** Demo telemetry is cached per tick; the update loop skips work
  when the overlay is hidden; map/radar/table paths reuse cached layout and
  pixmap data where safe.

## 1.42.0 - 2026-07-02

- **Practice relative list.** During practice, the relative table shows only cars
  physically on track — garage and off-track entries are hidden.
- **Qualifying standings.** During qualifying, the standings tower orders drivers
  from live QualifyResultsInfo (with best-lap fallback) instead of race positions.

## 1.41.0 - 2026-07-02

- **Standings pin podium keeps you visible.** When P1–P3 are pinned, your row
  stays in the list — neighbor rows trim first instead of dropping you from view.
- **Fuel calc stats text scale.** Separate header/row text size settings for the
  AVG/MAX/MIN grid, independent of the rest of the widget text scale.
- **Lap log live updates.** Demo lap counter syncs so new laps append each lap;
  live sessions fall back to CarIdxLastLapTime when LapLastLapTime is zero and
  clear stale history on session reset.
- **Fuel calc accuracy.** Demo fuel burn matches FuelUsePerHour and lap time;
  projections prefer the car's estimated lap when the lap log times don't match.

## 1.40.0 - 2026-07-01

- **Standings pin podium.** Optional setting keeps P1–P3 in the first three rows
  while the remaining rows show your usual centered window (same total height).
- **Grouped widget settings.** Each widget's settings page is organized into
  purpose-based sections (Content, Typography, Colors, etc.) instead of one flat list.

## 1.39.0 - 2026-07-01

- **Smaller wind compass near the track.** The wind dial is reduced in size and
  sits just outside the track bounding box (whichever corner overlaps the circuit
  least) instead of in a far widget corner.

## 1.38.0 - 2026-07-01

- **Map traffic marker clipping.** Layout padding now reserves space for outward
  floating icons and car-number pills so they are not cut off at the widget edge.

## 1.37.0 - 2026-07-01

- **Manual pit speeds only.** Pit speed limit and pit lane speed % are no longer
  updated from telemetry when you drive through pit road — set them in Track Scan
  metadata (or load from the track file).

## 1.36.0 - 2026-07-01

- **Edit layout toggle sync.** Settings now reflects `--no-clickthrough` on launch
  (the switch knob matches actual edit mode).
- **Natural demo map traffic.** Removed artificial lap-% pinning; oval pacing and
  staggered pit visits so Chicagoland demo dots flow like live traffic.
- **Clearer traffic markers.** Leader/ahead/behind icons show the car number,
  draw a solid line to the actual car dot, and highlight the target with a ring.

## 1.35.0 - 2026-07-01

- **Demo Chicagoland from MongoDB.** `--demo` now loads Chicagoland Speedway
  (TrackID 123) from the shared track library on every launch, using the local
  cache only as a placeholder while the cloud fetch runs. `--demo-track <ID>`
  still overrides for testing other layouts.
- **Crown leader icon.** The map's overall-leader traffic marker uses a crown
  instead of a trophy.

## 1.34.0 - 2026-07-01

- **Map pace car.** When iRacing reports a pace car on track, the map shows a
  black dot with white **PC** (configurable in Settings).
- **Sector boundaries on the track loop.** Split-time sector starts render as
  perpendicular ticks with S2/S3 labels outside the circuit.
- **Traffic markers.** Floating icons outside the track mark the car ahead, car
  behind, and overall leader (P1). Each slot holds its current target for 3
  seconds before switching to reduce flicker when rivals are side-by-side.

## 1.33.0 - 2026-07-06

- **Configurable row height on every table.** Relative, Standings, Laptime Log,
  Fuel Calc (usage stats grid), and Lap Compare (corner rows) now expose
  **Fixed row height (px)** and **Max row height (panel fraction)** in Settings.
  Set a fixed pixel height to keep rows and text from growing when you resize the
  panel, or leave it at 0 to scale rows to fit (capped by the fraction).

## 1.32.0 - 2026-07-05

- **Pit editor zoom and pan.** Track Scan pit authoring now auto-frames the pit
  road and merge handles, supports scroll-to-zoom (including macOS trackpads),
  Shift-drag pan, and a **Reset view** button. The last pit-road point and first
  merge point stay linked when you drag either handle.
- **Per-track pit lane speed.** Tracks can store `pit_lane_speed_pct` so car dots
  advance along the pit route at the correct rate when the drawn pit polyline is
  longer or shorter than the matching loop arc. Track Scan exposes a speed % slider
  when authoring.
- **Oval pit placement fixes.** On ovals like Chicagoland, cars on pit road are
  mapped along `pit_in_pct`→`pit_out_pct` instead of raw path projection, with
  phase gating so entry/exit blends do not fight lane placement while `OnPitRoad`.
  Turn labels on ovals renumber to 1–4 (counter-clockwise from S/F) at import and
  on the map.
- **Pit edit zoom fix (macOS).** Scroll zoom now reads trackpad `pixelDelta`
  events and uses layout-space coordinates for zoom-to-cursor, so pit editing
  zoom works reliably on macOS and with rotated/mirrored map presets.

## 1.31.0 - 2026-07-04

- **Pit car placement (schematic tracks).** Length-calibrated progress now slows
  dots when the drawn pit polyline is longer than the matching loop arc (short
  ovals like Chicagoland) and speeds correctly when it is shorter. Entry, lane,
  and exit are mapped as separate phases so lap-% no longer races through the
  whole pit chain at once.
- **No false pit on the racing line.** Cars are only drawn on the pit route when
  `OnPitRoad`, approaching pits, or in the exit blend after a stop — not for the
  entire wrapping `pit_in`→`pit_out` lap-% arc on ovals.
- **Smoother pit entry on the map.** The player icon eases from the racing line
  onto the entry blend instead of jumping when `OnPitRoad` flips.
- **Map competitor colors.** All rivals use one default color; cars a lap down
  show blue and cars lapping you show red only when they are close on track or
  a full lap ahead (no brief red/blue flash at the start/finish line).
- **Speaking indicator on the map.** When a driver is on the radio, their dot
  shows the speaker icon instead of car number or position (same telemetry as the
  relative/standings badge).

## 1.30.0 - 2026-07-03

- **Schematic pit car mapping fix.** Cars on the authored pit route (entry, lane,
  exit) now advance along the polylines using length-calibrated lap-% progress,
  so dots no longer zip ahead of their real position on tracks like Indianapolis
  where the pit lane is a shorter parallel offset of the racing line.
- **HTML-only track authoring.** Removed GPS track learning (3-lap scan), GPS pit
  learning (3 pit passes), **Rescan track**, **Rescan pits**, and session-only pit
  blend sliders. Tracks are imported from members HTML in **Track Scan**, pit road
  and merge are drawn on the map, then **Save track** writes `tracks/<TrackID>.json`.
  If no file exists when you join a session, the map prompts you to import HTML.
- **Start/finish editing on map.** Track Scan toggle drags the white start/finish
  line along the racing loop; release saves `start_finish` to the track file.
- **IMS / stripe S/F import.** V2 HTML import aligns the loop to the vertical
  stripe in the `start-finish` layer (exact crossing snap) instead of the
  direction-arrow tip.
- **Demo track loading.** `--demo` prefers the most recently saved track in the
  writable tracks folder (e.g. after saving IMS as TrackID 522); post-save hint
  suggests `--demo-track <ID>` for an explicit reload.
- **Removed `tools/record_track.py`.** One-lap GPS recording is superseded by the
  HTML import workflow; README updated accordingly.

## 1.29.0 - 2026-07-02

- **V2 HTML loop import + manual pit editor.** New `tools/svg_layers_to_track_v2.py`
  imports only the racing loop from members-site HTML (`active-config` layer).
  Pit geometry is drawn by hand on the **live overlay map** in Track Scan (write
  access): click **Pit road** points, then **Merge** points; **Save track** writes
  `pit_path`, `pit_out`, auto-generated `pit_in`, and lap-% extents to
  `tracks/<TrackID>.json`. The v1 `svg_layers_to_track.py` CLI still auto-imports
  pit from HTML when you want a one-shot import.
- **Track Scan v2 panel.** Choose HTML, **Import loop**, toggle draw mode, undo /
  clear pit points, and save — all from Settings → Track Scan without leaving the
  overlay.
- **MongoDB Atlas TLS on macOS.** Cloud track upload/download now passes certifi's
  CA bundle to PyMongo, fixing `CERTIFICATE_VERIFY_FAILED` on python.org macOS
  builds. `tools/check_db.py` surfaces a fix hint when SSL trust fails.

## 1.28.0 - 2026-07-01

- **Unified panel chrome across all widgets.** Every overlay panel (dash, lap log,
  fuel calc, lap compare, sector timing, delta bar, inputs, flags, radar, map,
  relative, standings) now shares the same visual system: gradient card shells,
  consistent border weight, dark translucent cells for numeric readouts, and
  tabular mono fonts for lap times, gaps, and deltas. Shared helpers live in
  `overlay/widgets/chrome.py`, `fonts.py`, and `formats.py`.
- **Table polish (Relative & Standings).** Dark iRating pills with borders,
  optional chart icon, full-width player row highlight, opaque header/footer bands
  with corner-matched rounding, hairline row dividers, softened license pills,
  and signed relative gaps. Footer no longer shows rows bleeding through.
- **Dash polish.** Top/bottom sub-panels, strip pill, position box, and iRating
  pill use the shared chrome; numeric readouts render in tabular mono. iRating
  pill is now dark (matching tables). Stat cells can show row dividers between
  stacked values (e.g. fuel + laps).
- **Widget-specific polish.** Lap log, fuel calc, lap compare, sector timing,
  delta bar, inputs, flags, radar, and map each adopt card chrome, dark cells,
  and/or tabular numerics where appropriate. Map scan/hint overlays and
  schematic pit-lane colors are now config-driven.
- **New config keys.** Shared chrome settings (`header_bg`, `footer_bg`,
  `row_dividers`, `cell_border`, `data_font_bold`, `corner_radius_frac`, etc.)
  are available per widget; `border` and `panel_border` are aliases. Settings
  editor labels added for the new keys.

## 1.27.0 - 2026-06-30

- **SVG track import.** Import tracks from iRacing members-site HTML (SVG layers:
  racing line, pit road, merge) via `tools/svg_layers_to_track.py` or the Track
  Scan / schematic import panel. More reliable than PNG schematic import; oval
  pit geometry fixes (e.g. Chicagoland entry/merge alignment).
- **Dash iRating.** Single **iRating** metric for any dash slot, with optional
  **Show projected iRating change next to iRating** toggle. Renders the
  standings-style pill (value + green up / red down delta). Legacy
  `irating_delta` / `irating_stack` slot keys migrate automatically.
- **Dash car number.** **Car number** is available as a dash metric (icon +
  value).
- **Dash flag bar spacing.** More vertical space between the flag label and
  context subtitle.
- **Session time on grid.** Session/race time no longer shows `168:00:00` while
  on the grid; invalid iRacing placeholders show as em dash until timing is live.
- **Settings dropdowns sorted A-Z.** All combo boxes in the settings editor sort
  options alphabetically by display label (strict, including "None").
- **Fixed row height on tables.** **Fixed row height** on Relative and Standings
  now honors the configured pixel size instead of shrinking rows when the panel
  is short. Slider range is 0-72 (0 = scale-to-fit).

## 1.26.0 - 2026-06-30

- **Schematic track import.** Import iRacing's in-sim map PNG (white racing line,
  red pit road, blue merge) into a schema-2 track JSON via
  `tools/schematic_to_track.py` or the **Track Scan** tab (write access). The map
  renders schematic legend styling and places pitting cars on the authored
  polylines instead of learning the lane from telemetry.
- **Starting grid positions before green.** While on the grid or in formation,
  standings, relative, dash, and map position labels now show qualify starting
  positions from `QualifyResultsInfo` instead of 0 until live
  `CarIdxPosition` values are published.
- **Smarter relative list.** The relative widget only shows cars on track plus
  anyone ahead in race position — garage and off-track cars behind you are hidden.
- **Projected iRating change.** Optional **Show projected iRating change** toggle
  on Standings and Relative estimates +/- iRating from live class positions using
  the SOF formula (race sessions only). Deltas render as green up / red down arrow
  icons beside the iRating column.
- **Table polish.** License column shows Safety Rating (e.g. `3.34`) instead of
  iRating + class pill. A speaker icon appears on the row of whoever is
  transmitting on radio. Lapped-traffic and player rows use a soft left-stripe
  gradient instead of solid fills; empty status badges draw nothing (no black
  box). Standings now tints lapped traffic the same way as Relative.
- **Flag context on dash and flags widget.** Yellow, start, and other flag states
  show a short subtitle when the sim exposes detail (e.g. **1 lap to green**,
  **Get ready — start imminent**).

## 1.25.0 - 2026-06-30

- **Smarter pit scanning on road courses and ovals.** Pit entry/exit blend lengths
  now default from iRacing track type (road vs oval). Entry and exit commitment
  zones are drawn along the racing line from the sim's surface boundaries; the
  main pit lane stays traced from your actual drive (with drift correction), not
  re-wrapped around the loop. Road exits taper onto the track; oval exits hold
  a parallel apron offset until the merge point.
- **More reliable pit scan quality.** Multi-pass pit learning uses least-squares
  alignment on approach and exit anchors, drops outlier passes (e.g. a blown-out
  first lap after a track scan), and shows **PIT n/3** progress on the map while
  gathering passes.
- **Track metadata authoring on the Track Scan tab.** Authors can set pit speed
  limit, official corner count, and drag corner labels on the map; changes save
  to the local track file and upload to the shared library. Fixes ensure a local
  track file is created when needed and cloud tracks match whether TrackID is
  stored as a number or string.
- **Dash primary readouts layout.** Lap count and speed in the lower-left panel
  are right-aligned toward the center ring, with each metric shown as
  icon + label + value (e.g. flag, "LAP", `3/10`).

## 1.24.0 - 2026-06-30

- **New "Track Scan" settings tab (write access only).** The "Rescan track" and
  "Rescan pits only" actions moved off the Map page into their own vertical tab
  under Settings, which only appears for users with write access (a
  `GRIDGLANCE_MONGODB_URI` author credential). Read-only users no longer see any
  scan controls.
- **Live pit blend tuning sliders (session only).** The new tab adds two sliders
  for the pit exit and entry lane lengths so an author can dial in a track on the
  fly. Changes apply instantly to the running overlay but are **not** saved --
  they reset to the `constants.py` defaults on the next launch. Each slider has a
  one-click reset back to its default.
- **Demo mode now shows cars pitting every lap.** Three demo cars make a full
  pit stop each lap, riding the synthesized pit route (entry / exit blends and
  lane), so opponent pit-route placement is visible without a live scan.
- **New "Pit lane opacity" map setting.** Fade the drawn pit lane and its
  entry / exit blend lines back behind the track (0 = hidden, 1 = solid). Shown
  on the Map tab only when the pit lane is enabled.
- **"Car dot size" map setting now works.** The map's `dot_radius_frac` control
  previously had no effect; it now scales every car dot (and the player's glow
  ring) and has a sensible slider range (0.05 = the default size).
- **Accurate corner numbering on road courses.** Auto-detected corners now use
  iRacing's official corner count (`WeekendInfo.TrackNumTurns`) when available,
  so the map matches the sim's numbering on both ovals and road courses instead
  of over- or under-counting. The count is saved with learned tracks and shared
  in the cloud library.
- **Pit entry/exit lanes drawn on the correct side.** The pit blend lines now
  offset toward the side the pit was actually recorded on, rather than always
  toward the track's center. This keeps the lanes parallel to the track on road
  courses where pit road sits on the outside of the loop (previously they could
  flip across the track).
- **Wind compass avoids the track.** The compass now drops into whichever map
  corner the track covers least, so it no longer sits on top of the layout.

## 1.23.4 - 2026-06-30

- Internal cleanup and micro-optimization of the map's per-frame paint and
  per-tick update paths (pit-car styling, palette and config reads are now
  resolved once instead of per car). No user-facing behavior changes.

## 1.23.3 - 2026-06-30

- **Demo mode now shows the full pit lane.** The map demo synthesizes a pit lane
  from the demo track -- the dashed lane, the yellow entry / blue exit blends,
  and the static pit speed badge -- so every recent map feature (plus the new
  "Show pit entry/exit lines" / "Show pit speed limit" toggles and cars riding
  the pit route) is visible without a live iRacing scan.
- Loading a track with no pit data now fully clears any previous pit lane,
  instead of leaving a stale one on the map.

## 1.23.2 - 2026-06-30

- **Better auto corner numbering.** A long sustained bend is now split into
  ~80-degree chunks, so each end of an oval (~180 degrees of turning) reads as
  two numbered turns the way iRacing counts them, instead of collapsing into
  one. Still a geometry heuristic, so it won't always match every road course.
- **More track / pit smoothing.** The learned track loop and the pit lane +
  entry/exit blends now get a second light smoothing pass, ironing out the
  remaining squarish patches and offset steps. Re-scan to apply to pit lines.

## 1.23.1 - 2026-06-30

- **Static pit speed badge.** The pit label now shows only the learned pit speed
  limit (e.g. "PIT 45 MPH") -- no live comparison to your speed and no over-limit
  color flip. Its font is a touch smaller, too.
- **New map setting: "Show pit speed limit."** Toggle the badge independently of
  the entry/exit lines. The pit sub-options (speed badge, entry/exit lines) are
  hidden in settings while the pit lane itself is turned off.

## 1.23.0 - 2026-06-30

- **Map setting: "Show pit entry/exit lines."** New toggle in the map settings.
  When off, the entry/exit blend lines are hidden and a car simply appears in the
  pit lane while it's actually on pit road, snapping back onto the track the
  moment it leaves -- no blend lane to ride.
- **Pit exit line is now blue.** The entry blend stays yellow and the exit blend
  is drawn in blue so the two ends of the pit lane read apart at a glance
  (configurable via the new "Pit exit line" map color).

## 1.22.9 - 2026-06-30

- **Fix: car dragged into the pits past the pit entry on a normal lap.** The dot
  now only rides the pit route after a sustained stint actually on pit road, not
  from merely reading as off the racing line near the entry (which a slightly
  wide line or a rough patch in the learned track could trigger). It holds
  through the exit blend and hands back to the track once you pass the route end.
- **Smoother learned geometry.** The scanned track loop is lightly smoothed so
  sparse/interpolated bins no longer leave squarish corners, and the pit lane and
  entry/exit blends are smoothed (endpoints anchored) to remove the little offset
  'steps' the parallel-lane nudge could leave. Re-scan to apply to pit lines.

## 1.22.8 - 2026-06-30

- **Fix: dot stuck in the pits when passing the pit entry on a normal lap.**
  After a pit stop, driving past the pit entry again while staying out could pin
  the car icon in the pit lane until the start/finish line. The dot now only
  moves onto the pit route after a sustained commitment (actually on pit road or
  clearly off the racing line), so a brief blip while staying out is ignored.

## 1.22.7 - 2026-06-30

- **Fix: car dot blinking between track and pit-exit lane.** Rounding a corner on
  the way out of the pits, the dot could flick onto the track and back as the
  apron skimmed the racing line. The dot now stays on the pit route for the
  route's full extent and only returns to the track once you pass the route's
  end, so it can't flicker mid-corner.
- **Shorter pit entry line.** The yellow entry blend no longer reaches all the
  way back to where you first eased off the racing line; it's capped to the last
  stretch before pit road (`PIT_ENTRY_MAX_PCT`, default 8% of a lap, tunable).
- Pit-exit lane reach default raised to 0.16 of a lap (the value that landed it
  right on the reference oval).

## 1.22.6 - 2026-06-30

- **Pit exit lane reaches further down the track.** With the merge now detected
  accurately, the post-merge extension (`PIT_EXIT_EXTEND_PCT`) is lengthened to
  0.14 of a lap so the yellow exit line runs well down the commitment zone --
  restoring the long reach of the earlier build without its swiggle or flicker.
  Tune the one value up/down if it's slightly long or short on a given track.

## 1.22.5 - 2026-06-29

- **Pit exit lane now reaches the end of the commitment line.** The car
  geometrically rejoins the racing line before iRacing's painted pit-exit line
  actually ends, so the lane stopped short of where you really commit back to the
  track. The exit is now traced a configurable lap fraction past the detected
  merge (`PIT_EXIT_EXTEND_PCT`, default 6% of a lap) so the yellow line continues
  to the true end of the exit, and the amount is speed-independent and easy to
  tune.

## 1.22.4 - 2026-06-29

- **Fix: pit exit merge detected too early on narrow ovals.** Distance from the
  car to the racing line was measured against the *whole* track, so on a tight
  oval the nearest point could be the opposite straight a few metres across the
  infield -- faking a merge while the car was still out on the pit-exit apron,
  which cut the yellow exit line short. Distance is now measured only against the
  racing line near the car's own lap position, so the exit lane is captured all
  the way to where you genuinely rejoin the track. The post-merge extension is
  reduced accordingly (now just a short nudge).

## 1.22.3 - 2026-06-29

- **Pit exit lane now reaches the end of the commitment line.** The car regains
  the racing line a little before iRacing's painted pit-exit line actually ends
  down the straight, so the drawn lane was stopping short. The exit is now traced
  for a short stretch past the detected merge (tunable via `PIT_EXIT_EXTEND`) so
  the yellow line continues down the straight like the real commitment zone.

## 1.22.2 - 2026-06-29

- **Fix: pit exit lane running too far / onto the track.** If a pit pass never
  actually merged back onto the racing line (e.g. you looped around to re-pit),
  its exit trace ran most of a lap and was wrongly used as the "longest" exit,
  dragging the yellow line far past the real merge and adding a squiggle across
  the track. The exit lane is now built only from passes that genuinely
  rejoined the racing line.
- **Fix: player dot flickering between track and pit exit lane.** Rounding a
  corner on the way out of the pits could pop the car icon back and forth as its
  distance to the racing line briefly dipped. The on-route state is now debounced
  (the car must hold near the line for ~0.5s) so the dot stays put.

## 1.22.1 - 2026-06-29

- **Fix: pit entry/exit blends not appearing.** The blend detection compared the
  car's distance from the racing line against a fraction of the whole-track
  size, which was larger than the pit lane's offset -- so the yellow entry/exit
  lines came out empty and only the lane drew. The thresholds are now scaled to
  the pit lane's measured offset from the racing line, so the entry and exit
  blends are captured reliably.
- **Author tools: clearer upload diagnostics.** Track-upload failures are now
  logged (bad credential, blocked Atlas IP, missing `pymongo`) instead of failing
  silently, a new `tools/check_db.py` reports the sharing setup and can test a
  real upload, and `tools/sync_tracks.py` now uses the app's per-user `tracks/`
  folder so it finds maps you scanned in-app.

## 1.22.0 - 2026-06-29

- **Full pit lane, entry to exit.** Pit scanning now captures the whole route --
  the entry blend where you leave the racing line, the pit lane itself, and the
  exit blend back onto the track (e.g. the long merge down an oval's back
  stretch). The entry/exit blends are detected from how far the car strays from
  the racing line, averaged over the three pit passes, and saved/shared with the
  track.
- **Yellow commit lines on the map.** The entry and exit blend sections are
  drawn as dashed yellow "slash" lines (themeable via the new `pit_blend` map
  color), with the pit lane itself still in the pit color between them.
- **Cars follow the pit route.** While pitting, your car is drawn on the real
  pit geometry from live GPS -- peeling off, down the lane, and merging back onto
  the track. Other cars are mapped onto the route by track position once they're
  on pit road and held through the exit blend until they rejoin.

## 1.21.0 - 2026-06-29

- **Local `.env` files are now loaded automatically.** Author/dev database
  credentials placed in a `.env` (in the working directory, repo root, or next
  to the executable) are picked up on launch -- no need to `export` them in your
  shell first. Real environment variables still take precedence.
- **Simpler author setup.** Setting a single `GRIDGLANCE_MONGODB_URI` now drives
  both reads and writes, so one read-write connection string is enough to unlock
  the scan/record controls and download maps. `GRIDGLANCE_MONGODB_READ_URI`
  remains a read-only override and, by design, never enables write/dev mode on
  its own.

## 1.20.0 - 2026-06-29

- **Fix: track map crash while scanning.** The "LAP n/3" scan badge / pit hint
  overlay was drawn from the wrong place and raised an error during map paint;
  the map now renders correctly throughout learning.

## 1.19.1 - 2026-06-29

- **Housekeeping.** Ignore local `.env` files in version control so author
  database credentials (`GRIDGLANCE_MONGODB_URI`) can't be committed by accident.

## 1.19.0 - 2026-06-29

- **Multi-lap scanning for cleaner maps.** Learning a track now takes three full
  laps, averaged together into one smooth line (a "LAP n/3" badge shows
  progress, and the partial lap you start on doesn't count) before the map is
  saved and uploaded. The pit lane unlocks only after the track scan finishes
  and is confirmed over three pit passes (averaged, start/finish-aware) before
  its data is saved/uploaded; passing the pits earlier shows a "Finish track
  scan first" hint.
- **Real pit lane on the map.** The pit lane is now drawn from its actual
  recorded route -- from where you leave the track to where you rejoin -- instead
  of an approximation offset from the racing line. The geometry is captured from
  your GPS over the three pit passes, averaged, and saved/shared with the track.
- **Flexible database credentials.** The shared-maps read connection string now
  comes from the `GRIDGLANCE_MONGODB_READ_URI` environment variable when set,
  falling back to a built-in default -- so a deployment can point at its own
  database without rebuilding. (Also fixes the read URI not being applied.)

## 1.18.0 - 2026-06-29

- **Community track maps.** Track maps are now shared through the cloud, so the
  first time you visit a track GridGlance downloads its map instead of making you
  drive a lap to learn it. Maps still learn locally (and are uploaded by the
  author) so the library keeps growing. Toggle it off on the new App settings
  page to stay fully offline.
- **Auto-sync on launch.** The shared library is the source of truth: on every
  start GridGlance refreshes the maps it has already cached, pulling any that
  changed since last time (it only transfers what actually changed). If the track
  you're on was updated, the live map reloads itself.
- **Bounded cache.** Downloaded maps are cached so revisits are instant and
  offline-friendly, but the cache is capped (least-recently-used maps are
  evicted past the limit) so it can't grow unbounded as you visit more tracks.
  Your own bundled/learned maps are never evicted.
- **Author tools.** Running from source with a read-write database URI unlocks
  uploading: the "Rescan track" / "Rescan pits" controls appear, learned maps are
  pushed automatically, and `tools/sync_tracks.py` bulk-uploads or downloads the
  whole library.

## 1.17.0 - 2026-06-29

- **Auto-switch presets by league.** Bind a preset to one or more league sessions
  (General page: "Add current league") and GridGlance activates it automatically
  when you join that league.
- **A default preset, always.** Mark any preset as your default with the new
  "Default preset" toggle in the preset bar. Exactly one preset is always the
  default -- it acts like a radio button, so you change it by choosing a different
  preset, and your only preset is locked as the default.
- **Smart switch order.** Auto-switching now resolves in priority order: a bound
  league wins over a bound car, which wins over your default. Each rule has its own
  on/off switch in the new "Auto-switch presets" card on the General page.

## 1.16.0 - 2026-06-26

- **Config presets.** Build multiple complete overlay setups and switch between
  them from a new "Preset" bar at the top of Settings. Each preset is fully
  independent: its own on-track and in-garage settings, its own widget layout
  (window positions and sizes), and its own car bindings -- so a "League" set
  and a "Practice" set never step on each other. Create, duplicate, rename and
  delete presets right from the bar.
- **Per-preset layouts.** Window positions and sizes are now saved per preset,
  so each setup remembers exactly where you put its widgets.
- **Auto-switch by car.** Bind cars to a preset (General page: "Add current car")
  and turn on "Auto-switch by car" to have GridGlance activate the matching
  preset automatically when you hop into a car.
- **Safer config writes.** Settings are now written atomically (temp file +
  rename) so a crash mid-save can't corrupt your configuration. Existing
  settings and your saved layout are migrated into a "Default" preset on upgrade.

## 1.15.0 - 2026-06-29

- **Per-widget reset.** Every widget's settings page now has a "Reset to defaults"
  button (tinted in that widget's color) that restores just that widget's settings,
  leaving everything else untouched -- separate from the global Reset that clears
  the whole profile.
- **Tables no longer zoom when resized.** The Relative and Standings tables now use
  a fixed row height, so dragging a table bigger in edit mode just adds empty space
  instead of ballooning the text, row height and columns. Tune it with the new
  "Fixed row height" slider (set it to 0 to go back to the old scale-to-fit
  behavior).
- **Relative and Standings are now themed independently.** The shared "Table" tab
  is gone -- each table has its own full set of settings (colors, column widths,
  fonts, row height and more) on its own page, so changing one never affects the
  other. Your existing table settings are migrated onto both tables on upgrade.
- **Cleaner driver rating pill.** The license pill now reads "1.4k R" (iRating +
  class) and hugs its text instead of padding out the column. A new toggle switches
  iRating between short (1.4k) and full (1432) form, per table.
- **Independent header/footer text size.** Header and footer text can now be scaled
  separately from the row text (and from each other); the band grows to fit so
  larger text no longer clips.
- **Smarter header/footer fit.** Narrowing a table now closes the gaps between the
  header/footer items first instead of running the far-right element off the edge.
- **Seamless updates.** Installing an update no longer walks through the setup
  wizard -- GridGlance closes, updates itself silently and reopens on the new
  version automatically.

## 1.13.0 - 2026-06-29

- **Renamed to GridGlance.** The app, window titles, installer, shortcuts and the
  "Apps & features" entry now use the new name. Your existing settings, layout,
  saved best laps and learned tracks are migrated automatically from the old
  location, so nothing is lost on upgrade.
- **New app icon.** The dashboard badge now shows as a clean transparent circle
  (no square background) across the taskbar, window, tray, installer and the
  installed-programs list.

## 1.12.0 - 2026-06-29

- **Fix: oval turn detection.** Paired oval corners (T1-T2, T3-T4) used to read as
  a single turn because the wheel never unwinds between them; Lap Compare now
  splits them at the speed dip for each apex, so a four-corner oval shows all four.

## 1.11.0 - 2026-06-29

- **New Lap Compare widget.** Benchmarks your current lap against your best lap,
  corner by corner, and tells you *why* you're slow. It records throttle, brake,
  steering, speed, cornering load, gear and rpm against track position, auto-detects
  the turns, and ranks the corners costing you time with plain-language tips --
  apex speed, braking point, getting back to throttle, **coasting** (time on neither
  pedal), **trail-braking to the apex**, **"too cautious here, push harder
  mid-corner" when you're using less grip than your best**, slowest-point-too-early,
  jerky steering, and short-shifting / hitting the limiter. Shows a big live delta,
  a delta-over-distance trace, and the corner list in track order (first corner at
  the top). Tip text wraps so nothing gets cut off.
- **Trustworthy benchmark.** Laps with an off-track or a fresh incident are
  automatically discarded, so a dirty lap never becomes your reference.
- **Best lap persists per car + track.** Your benchmark is saved and restored when
  you load back into the same car at the same track, so you're always comparing
  against your all-time best for that combo.
- **Consistency readout.** A footer shows your lap-time spread over recent clean
  laps (green when tight, red when loose) -- ideal for practice.
- **Settings now live in a per-user folder** (`%LOCALAPPDATA%\GridGlance` on
  Windows), separate from the app, so updates never overwrite your config, layout,
  learned tracks or saved best laps. Existing settings are migrated automatically.
- **Clean installs start with every overlay off** -- turn on just the widgets you
  want from Settings.
- **Lower CPU.** Static widgets (flags, sector timing, lap compare) now only
  repaint when their data actually changes instead of every frame, and the costly
  session-info reads behind the lap-compare and sector widgets are cached.

## 1.9.0 - 2026-06-28

- **New Delta Bar widget.** A big, standalone live time delta: a large signed
  number over a center-anchored bar that deflects left (faster, green) or right
  (slower, red). Pick the reference lap -- session best, your best, or the
  optimal lap -- and the full-scale range from the new Delta Bar settings page.
- **New Flags widget.** A standalone flag banner that mirrors the dash flag
  style (diagonal-slash texture, checkerboard for the finish flag, label plate)
  inside the usual panel. It stays hidden while no flag is flying and only
  appears when a flag comes out (a "TRACK CLEAR" placeholder shows in layout-edit
  mode so you can still position it).
- **New Sector / Lap Timing widget.** Current, last and best lap times plus live
  sector splits derived from your lap-distance crossings, tracking your best per
  sector (cells turn purple when you match it). Uses the session's sector layout
  when available, otherwise a configurable number of equal sectors.

## 1.8.0 - 2026-06-28

- **New Input Telemetry widget.** A scrolling throttle/brake/clutch trace (newest
  on the right) with a vertical title tab, live value bars, and a gear + speed
  medallion -- styled to match the other panels. Toggle each section and channel,
  set the history length, line width and colors from the new Inputs settings page.
- **Steering trace.** Optionally overlay a steering line that swings around the
  center, normalized to the car's lock (`inputs.show_steering`).
- **ABS + trail-braking cues.** The brake line/bar turns a configurable color
  while ABS is active, and an optional brake-threshold line (a percentage) recolors
  the brake trace where it climbs above it.
- **Fix:** the settings window no longer crashes when a numeric value's type
  doesn't match its control (e.g. a hand-edited or older config value).

## 1.7.0 - 2026-06-28

- **Radar side marker shows fore/aft position.** A car alongside is now a marker
  that slides up and down your side: low when they're at your rear bumper, rising
  to the top as they pull up to your front bumper (instead of a static side bar).
- **Front/rear sensing toggles.** You can turn the radar's front and rear
  proximity glow on or off independently (`radar.show_front` / `radar.show_rear`).
- **Optional side proximity color.** A new toggle fades the side marker red→yellow
  by fore/aft overlap (`radar.side_proximity_color`, off by default); the marker
  is solid red otherwise. iRacing exposes no true sideways distance, so this is
  an overlap-based approximation.
- **Demo map corner numbers fixed.** The demo track's corners are now numbered
  from the track geometry (on the real apexes, in driving order) instead of the
  old hardcoded guesses that sat between turns.

## 1.6.0 - 2026-06-28

- **Settings reorganized into Widgets / Settings tabs.** The settings window now
  has two top tabs: **Widgets** (per-widget pages) and **Settings** (global
  options plus the shared table styling), so the sidebar is less crowded.
- **Alphabetical widget tabs.** The widget pages in the sidebar are now ordered
  A-Z (Dash, Fuel Calc, Laptime Log, Map, Radar, Relative, Standings).
- **Removed the standalone Light HUD.** The old simple Fuel/Delta HUD has been
  dropped in favor of the multi-widget overlay; its settings tab and config
  section are gone.

## 1.5.0 - 2026-06-28

- **Pace car hidden.** The pace/safety car no longer shows up as a competitor in
  the Standings or Relative tables.
- **Accurate lapped traffic.** Relative now decides red/blue lapped tints from
  actual lap distance, so a car just behind you that hasn't crossed the line yet
  is no longer mistakenly shown as a lap down (blue).
- **White flag timing.** The white flag now shows only as you approach the line
  to start the final lap and clears the moment you cross onto it.
- **Standings sizing.** With only a few cars, rows no longer stretch and blow up
  the text; row height is capped (configurable via `table.max_row_height_frac`).
- **Tunable shift blink.** The RPM shift-bar blink point is now a fraction of the
  car's redline (`dash.shift_blink_pct`, default 0.99) so you can dial in exactly
  when it flashes if it felt too early.
- **Map readability.** Corner numbers now sit outside the track instead of on the
  racing line, and your own car is far easier to spot (glow + bright ring).

## 1.4.0 - 2026-06-28

- **Corner numbers on the map.** The track map now numbers corners. Tracks with
  corner data use it; learned tracks have their corners auto-detected from the
  shape and numbered in driving order from start/finish. Toggle with the map's
  "Show corners" / "Auto corners" settings.
- **Rotate / flip the map.** Orient the track map with `map.rotation`
  (0/90/180/270 degrees) and `map.mirror` (horizontal flip). The whole map
  rotates together; the wind compass stays north-up.
- **Cleaner timing-table cells.** The car-number column drops its background and
  shows `#3`; the license/safety-rating pill is now filled with the license
  color (e.g. `A 4.99`) with auto-contrasting text. The in-pit `PIT` status
  badge is a wider pill with more padding around the text.
- **Lapped-traffic colors in Relative.** Lapped cars now tint the whole row to
  the right edge: red for a car a lap ahead (about to lap you) and blue for a
  car a lap down (you're lapping it).
- **Dash input ring at rest.** Throttle/brake/clutch arcs in the ring view no
  longer show a stray lit segment when you're off the pedals (small deadzone).
- **More dash flags.** The flag bar now covers the full set of driver-facing
  flags: red (session stopped), blue ("let by"), debris, crossed (halfway) and
  the white flag (final lap). White flashes briefly like the green flag, and all
  flag colors are customizable.
- **Shift-light timing fix.** The RPM bar blinked too early on cars that don't
  report a blink RPM; it now uses iRacing's blink/shift RPM (not the last-LED
  RPM), so it flashes at the right shift point.
- **Overlay only shows in the sim.** The widgets now appear only while iRacing
  is connected and hide automatically when you leave the sim, so nothing floats
  over your desktop. Edit-layout mode still reveals them so you can arrange the
  layout offline.

## 1.3.0 - 2026-06-28

- **Redesigned settings window.** A cleaner, more modern UI that's easier to
  navigate. Widgets now live in a left **sidebar** (each tinted with its theme
  color and a dot showing whether it's enabled) instead of cramped tabs.
- **Enable-gated widgets.** Each widget page opens with a prominent on/off
  switch; a widget's settings stay collapsed until you turn it on, so you only
  see what's relevant.
- **Accordions.** Nested groups (colors, widths, sizes, …) are now collapsible
  cards, with the long secondary ones collapsed by default to cut clutter.
  Search auto-expands any group containing a match.
- **Sliding toggles.** Every on/off option is now an animated switch instead of
  a checkbox.
- **Sliders for numbers.** Numeric settings use a slider paired with a precise
  value box, with sensible ranges inferred per setting.
- **In-app "Edit layout" toggle.** You can now make the overlay widgets
  draggable/resizable and lock them again right from the settings window (and the
  tray menu) — no more relaunching with `--no-clickthrough` to rearrange them.
- **Check for updates from Settings.** The General tab has a "Check for Updates"
  button that asks GitHub for the latest release; if a newer one exists it shows
  the version + notes and, on confirm, downloads the installer (with a progress
  bar) and launches it to update in place.
- **Checkered flag on the dash.** The flag bar now also shows the **checkered**
  flag (`FINISH`) when the session ends, drawn as a black/white weave.
- **Garage vs on-track profiles.** Settings now have two profiles — **On track**
  and **In garage** — selectable at the top of the window. The garage profile
  stores only the values you change from your on-track setup, and the overlay
  switches between them automatically (via iRacing's `IsInGarage`). Use it to
  hide widgets in the garage or show different info there; while editing a
  profile the live overlay previews it.
- **Corner numbers on the map.** The track map now numbers the corners. Tracks
  with corner data use it; learned tracks have their corners auto-detected from
  the shape and numbered in driving order from start/finish. Toggle with the
  map's "Show corners" / "Auto corners" settings.
- **Rotate / flip the map.** The track map can be rotated in 90° steps
  (0/90/180/270) and mirrored horizontally from the map settings, so you can
  orient it however you like. The wind compass stays north-up.

## 1.2.0 - 2026-06-28

- **More flag states on the dash.** The flag bar now distinguishes the
  **meatball** (must-pit-to-repair), **furled** (warning) and **disqualified**
  flags in addition to caution/black/green, each with its own color + label, read
  straight from iRacing's `SessionFlags`.
- **Customizable flag flash.** The flag bar pulses briefly when a flag appears
  and then holds steady; the flash can be toggled and its duration/rate tuned
  from the Dash settings (`dash.flag_pulse` / `flag_pulse_seconds` /
  `flag_blink_hz`).
- **Fuel Calculator feature toggles.** Each section of the Fuel Calculator
  (title, status pill, add-fuel box, gauge, usage table, pit timeline, time/laps
  until empty) can be shown or hidden, and the rest reflow to fill the panel.
- **Clearer settings labels.** Terse toggles now show meaningful descriptions in
  the settings (e.g. "Show pit-window status pill" instead of "Show pill").
- **Dash ring input direction.** The throttle/brake/clutch arcs in the dash ring
  view now sweep left-to-right.

## 1.1.0 - 2026-06-27

- **Fuel Calculator widget.** A new overlay panel showing current fuel + tank
  gauge, litres to add to finish, a pit window, AVG/MAX/MIN usage scenarios
  (usage, laps, pit stops, refuel), and time/laps until empty with a margin
  warning when you'd run dry before the finish. Each section can be toggled on/off
  from the settings and the rest reflow to fill the panel.
- **Laptime Log widget.** A new overlay panel lists your most recent laps with
  lap number, time, a green/red delta (vs. the previous lap or your session
  best), and the track temperature — styled to match the timing tables.
- **Wind compass on the track map.** A small north-up compass in the map's
  corner shows wind direction (arrow) and speed, toggleable via `map.show_wind`.
- **Flag bar on the dash.** A thin bar across the top of the dash shows the
  current flag (`CAUTION`, `BLACK FLAG`, `MEATBALL` for the must-repair flag,
  `WARNING` for the furled black flag, `DISQUALIFIED`, or a transient `GREEN`
  after a caution clears). It is filled with diagonal racing slashes that frame
  the centered label, "waves" by flashing, and stops at the incident container
  so it never overlaps the position box.
- **Dash layout polish.** The air-temp / track-temp / time strip sits a little
  lower and is slightly thinner, keeping its rounded ends within the dash.
- **Independent table fonts.** Header and footer font sizes can now be scaled
  independently of the row font.

## 1.0.0 - 2026-06-26

- **Desktop app for Windows.** Build a standalone executable and installer (no
  terminal needed); a desktop icon launches the settings window first, from
  which you can start/stop the overlay widgets and close settings while they
  keep running. A tray icon provides Open Settings / Start-Stop Overlay / Check
  for Updates / Quit.
- **Automatic releases & in-app updates.** Pushing to `main` builds the
  installer and publishes a GitHub Release; the app checks for newer releases on
  launch (and via the tray) and offers one-click update.
- **Track map learning & persistence.** The track map is learned from telemetry
  (GPS or dead-reckoning), saved per track, and reloaded next time instead of
  rescanning. Settings allow rescanning the whole track or just the pits.
- **Pit lane.** The pit lane is drawn as a thin red dashed line with the learned
  pit speed limit; in-pit cars are grayed and faded, and the player's car shows
  on the pit lane rather than the track.
- **Customizable widgets.** Relative/standings tables, dash/RPM, radar, and
  track map with extensive configuration via the settings window.
