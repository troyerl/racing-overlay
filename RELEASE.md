# Changelog

This file is the source of truth for the app version and release notes. The CI
release workflow reads the **topmost** `## <version>` section below: that version
becomes the git tag / installer version, and the bullet points become the GitHub
Release notes. To cut a release, add a new section to the top and push.

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
