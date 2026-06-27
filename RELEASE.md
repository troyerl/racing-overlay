# Changelog

This file is the source of truth for the app version and release notes. The CI
release workflow reads the **topmost** `## <version>` section below: that version
becomes the git tag / installer version, and the bullet points become the GitHub
Release notes. To cut a release, add a new section to the top and push.

## 1.1.0 - 2026-06-27

- **Flag bar on the dash.** A thin bar across the top of the dash shows the
  current flag (yellow `CAUTION`, `BLACK FLAG`, or a transient `GREEN` after a
  caution clears). It is filled with diagonal racing slashes that frame the
  centered label, "waves" by flashing, and stops at the incident container so it
  never overlaps the position box.
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
