# Changelog

This file is the source of truth for the app version and release notes. The CI
release workflow reads the **topmost** `## <version>` section below: that version
becomes the git tag / installer version, and the bullet points become the GitHub
Release notes. To cut a release, add a new section to the top and push.

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
