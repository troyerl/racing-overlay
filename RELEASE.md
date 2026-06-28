# Changelog

This file is the source of truth for the app version and release notes. The CI
release workflow reads the **topmost** `## <version>` section below: that version
becomes the git tag / installer version, and the bullet points become the GitHub
Release notes. To cut a release, add a new section to the top and push.

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
