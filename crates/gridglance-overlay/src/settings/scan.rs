//! Track Scan Settings page + cloud admin cards.

use crate::cloud;
use crate::paths;
use crate::state::StateHandle;
use crate::tracks;
use crate::updater;
use egui::{Color32, RichText, Ui};
use serde_json::{json, Value};
use std::sync::Arc;

use super::schema::{PIT_LANE_CHOICES, PIT_PHASE_CHOICES};
use super::theme::{MUTED, TEXT, TITLE};
use super::widgets::{
    button_kind, enable_card, info_card, setting_row, styled_choice_combo, styled_combo,
    text_field, toggle_switch, ButtonKind,
};
use super::SettingsUi;

struct ScanSnap {
    track_id: Option<i32>,
    track_name: String,
    has_loop: bool,
    has_tid: bool,
    has_pit_draft: bool,
    has_any_pit: bool,
    has_cached_pit: bool,
    has_local_file: bool,
    phase: String,
    lane: String,
    pit_speed: f32,
    lane_pct: f32,
    aliases: Vec<i32>,
    active_len: usize,
}

fn track_json_exists(tid: Option<i32>) -> bool {
    let Some(id) = tid else {
        return false;
    };
    let tid = json!(id);
    let dir = paths::tracks_dir();
    let canonical = cloud::resolve_track_id(&dir, &tid).unwrap_or(tid);
    cloud::track_file_path(&dir, &canonical).is_file()
}

pub fn paint_track_scan(
    ui: &mut Ui,
    state: &StateHandle,
    ui_state: &mut SettingsUi,
    accent: Color32,
) {
    ui.label(RichText::new("Track Scan").size(16.0).strong().color(TITLE));
    ui.label(
        RichText::new("Import a members HTML loop, draw pit lanes on the map, save to disk/cloud.")
            .size(11.0)
            .color(MUTED),
    );
    ui.add_space(8.0);

    if !cloud::can_write() {
        info_card(
            ui,
            "Write access required",
            "Set GRIDGLANCE_MONGODB_URI (read-write) to unlock Track Scan.",
        );
        return;
    }

    let snap = {
        let st = state.read();
        let m = &st.map;
        let track_id = m.cached_track_id.or(st.frame.track_id);
        let has_loop = m.cached_path.len() >= 3;
        let has_tid = track_id.is_some();
        let has_pit_draft = m.road_pts.len() >= 2 && m.merge_pts.len() >= 2;
        let has_any_pit = m.entry_pts.len()
            + m.road_pts.len()
            + m.merge_pts.len()
            + m.entry_pts_2.len()
            + m.road_pts_2.len()
            + m.merge_pts_2.len()
            > 0;
        let has_cached_pit = m.cached_pit.path.len() >= 2
            || m.cached_pit.entry.len() >= 2
            || m.cached_pit.exit.len() >= 2
            || m.cached_pit2.path.len() >= 2;
        let lane2 = m.lane_is_2();
        let active_len = match m.phase_key() {
            "entry" => {
                if lane2 {
                    m.entry_pts_2.len()
                } else {
                    m.entry_pts.len()
                }
            }
            "merge" => {
                if lane2 {
                    m.merge_pts_2.len()
                } else {
                    m.merge_pts.len()
                }
            }
            _ => {
                if lane2 {
                    m.road_pts_2.len()
                } else {
                    m.road_pts.len()
                }
            }
        };
        ScanSnap {
            track_id,
            track_name: m.cached_track_name.clone(),
            has_loop,
            has_tid,
            has_pit_draft,
            has_any_pit,
            has_cached_pit,
            has_local_file: track_json_exists(track_id),
            phase: m.phase_key().to_string(),
            lane: if lane2 { "2".into() } else { "1".into() },
            pit_speed: m.pit_speed_ms as f32,
            lane_pct: m.pit_lane_speed_pct as f32,
            aliases: m.alias_ids.clone(),
            active_len,
        }
    };

    enable_card(ui, "Session", accent, |ui| {
        ui.label(
            RichText::new(format!(
                "TrackID: {}  ·  {}",
                snap.track_id
                    .map(|t| t.to_string())
                    .unwrap_or_else(|| "—".into()),
                if snap.track_name.is_empty() {
                    "no name"
                } else {
                    snap.track_name.as_str()
                }
            ))
            .size(12.0)
            .color(TEXT),
        );
        if !snap.has_loop {
            ui.label(
                RichText::new("Import a members HTML loop to author.")
                    .size(11.0)
                    .color(MUTED),
            );
        } else if !snap.has_tid {
            ui.label(
                RichText::new("No TrackID — join a session on this track to save.")
                    .size(11.0)
                    .color(MUTED),
            );
        }
    });

    ui.add_space(8.0);
    enable_card(ui, "Import loop from HTML", accent, |ui| {
        ui.horizontal(|ui| {
            if button_kind(ui, "Choose HTML…", ButtonKind::Default).clicked() {
                if let Some(path) = rfd::FileDialog::new()
                    .add_filter("HTML", &["html", "htm"])
                    .pick_file()
                {
                    ui_state.html_path = path.display().to_string();
                }
            }
            if !ui_state.html_path.is_empty() {
                ui.label(RichText::new(&ui_state.html_path).size(11.0).color(MUTED));
            }
        });
        if !ui_state.html_path.is_empty()
            && button_kind(ui, "Import loop", ButtonKind::Primary).clicked()
        {
            match import_html(state, &ui_state.html_path) {
                Ok(msg) => ui_state.flash(msg),
                Err(e) => ui_state.flash(e.to_string()),
            }
        }
    });

    if snap.has_loop {
        ui.add_space(8.0);
        enable_card(ui, "Edit pit on map", accent, |ui| {
            let mut editing = {
                let st = state.read();
                st.map.interactive && !st.map.corner_edit && !st.map.sf_edit
            };
            setting_row(ui, "Enable pit edit", None, |ui| {
                if toggle_switch(ui, &mut editing, accent, ui.id().with("pit_edit")).changed() {
                    if let Some(mut st) = state.try_write() {
                        st.map.pit_edit = editing;
                        st.map.interactive = editing;
                        st.map.corner_edit = false;
                        st.map.sf_edit = false;
                        if editing {
                            let _ = st.map.load_pit_from_cache(false);
                        } else {
                            st.map.pit_sel = None;
                            st.map.pit_drag = None;
                        }
                    }
                }
            });

            if editing {
                let mut phase = snap.phase.clone();
                setting_row(ui, "Phase", None, |ui| {
                    if let Some(next) =
                        styled_choice_combo(ui, "pit_phase", &phase, PIT_PHASE_CHOICES, 120.0)
                    {
                        phase = next;
                        if let Some(mut st) = state.try_write() {
                            st.map.phase = phase.clone();
                            if phase == "road" {
                                let lane2 = st.map.lane_is_2();
                                st.map.seed_road_from_entry(lane2);
                            }
                        }
                    }
                });

                let mut lane = snap.lane.clone();
                setting_row(ui, "Lane", None, |ui| {
                    if let Some(next) =
                        styled_choice_combo(ui, "pit_lane", &lane, PIT_LANE_CHOICES, 100.0)
                    {
                        lane = next;
                        if let Some(mut st) = state.try_write() {
                            st.map.lane = if lane == "2" { "2".into() } else { "1".into() };
                        }
                    }
                });

                let sel_info = {
                    let st = state.read();
                    st.map.pit_sel.and_then(|(lane_u, phase, idx)| {
                        let key = crate::state::MapAuthoring::pit_sel_phase_key(phase)?;
                        Some((lane_u, key.to_string(), idx))
                    })
                };
                if let Some((lane_u, cur_key, idx)) = sel_info {
                    setting_row(
                        ui,
                        "Selected point",
                        Some("Change type to move this handle between entry / road / exit."),
                        |ui| {
                            ui.label(
                                RichText::new(format!("L{lane_u} · {cur_key}[{idx}]"))
                                    .size(11.0)
                                    .color(MUTED),
                            );
                            let mut typ = cur_key.clone();
                            if let Some(next) = styled_choice_combo(
                                ui,
                                "pit_sel_type",
                                &typ,
                                PIT_PHASE_CHOICES,
                                120.0,
                            ) {
                                typ = next;
                                if typ != cur_key {
                                    if let Some(mut st) = state.try_write() {
                                        match st.map.retype_selected_point(&typ) {
                                            Ok(()) => ui_state.flash(format!("Point → {typ}")),
                                            Err(e) => ui_state.flash(e.to_string()),
                                        }
                                    }
                                }
                            }
                        },
                    );
                } else {
                    ui.label(
                        RichText::new(
                            "Click a pit handle on the map to select it, then change its type here.",
                        )
                        .size(11.0)
                        .color(MUTED),
                    );
                }

                ui.horizontal(|ui| {
                    if snap.has_cached_pit
                        && button_kind(ui, "Load saved pit", ButtonKind::Default).clicked()
                    {
                        if let Some(mut st) = state.try_write() {
                            let ok = st.map.load_pit_from_cache(true);
                            ui_state.flash(if ok {
                                "Loaded pit from track file"
                            } else {
                                "No saved pit geometry"
                            });
                        }
                    }
                    if snap.active_len > 0
                        && button_kind(ui, "Undo point", ButtonKind::Default).clicked()
                    {
                        if let Some(mut st) = state.try_write() {
                            let pts = st.map.active_pts_mut();
                            pts.pop();
                            ui_state.flash("Undid last point");
                        }
                    }
                    if button_kind(ui, "Reset view", ButtonKind::Default).clicked() {
                        if let Some(mut st) = state.try_write() {
                            st.map.reset_pit_edit_view();
                        }
                    }
                });
                ui.label(
                    RichText::new("Ctrl+scroll to zoom · Middle-drag or Shift+drag to pan")
                        .size(11.0)
                        .color(MUTED),
                );
                if snap.active_len > 0 || snap.has_any_pit {
                    ui.horizontal(|ui| {
                        if snap.active_len > 0
                            && button_kind(ui, "Clear selected", ButtonKind::Default).clicked()
                        {
                            if let Some(mut st) = state.try_write() {
                                let phase = st.map.phase_key().to_string();
                                let lane2 = st.map.lane_is_2();
                                st.map.clear_phase(&phase, Some(lane2));
                            }
                        }
                        if snap.has_any_pit
                            && button_kind(ui, "Clear all pit", ButtonKind::Warn).clicked()
                        {
                            if let Some(mut st) = state.try_write() {
                                st.map.clear_phase("all", None);
                            }
                        }
                    });
                }
            }

            if snap.has_tid {
                ui.horizontal(|ui| {
                    if button_kind(ui, "Save loop", ButtonKind::GhostAccent).clicked() {
                        ui_state.flash(save_loop(state));
                    }
                    if snap.has_pit_draft
                        && snap.has_local_file
                        && button_kind(ui, "Save pit", ButtonKind::GhostAccent).clicked()
                    {
                        ui_state.flash(save_pit(state));
                    }
                    if snap.has_pit_draft
                        && button_kind(ui, "Save track", ButtonKind::Primary).clicked()
                    {
                        ui_state.flash(save_track(state));
                    }
                });
            }
        });

        ui.add_space(8.0);
        enable_card(ui, "Track metadata", accent, |ui| {
            let mut speed = snap.pit_speed;
            // Seed from live SDK/YAML when the authoring field is still unset so a
            // stale default (22 m/s ≈ 49 mph) is not what gets saved/shown.
            if speed <= 0.5 {
                if let Some(live) = state.read().frame.pit_speed_limit_mps {
                    if live.is_finite() && live > 0.5 {
                        speed = live;
                    }
                }
            }
            let speed_hint = if speed > 0.5 {
                let st = state.read();
                let shown = st.config.conv_speed(speed);
                let unit = st.config.speed_unit();
                Some(format!("≈ {shown:.0} {unit}"))
            } else {
                None
            };
            if super::widgets::number_row(
                ui,
                "Pit speed limit (m/s)",
                &mut speed,
                0.0..=40.0,
                0.5,
                accent,
                speed_hint.as_deref(),
            ) {
                if let Some(mut st) = state.try_write() {
                    st.map.pit_speed_ms = speed as f64;
                }
            }
            let mut pct = snap.lane_pct * 100.0;
            if super::widgets::number_row(
                ui,
                "Pit lane speed (%)",
                &mut pct,
                50.0..=150.0,
                1.0,
                accent,
                None,
            ) {
                if let Some(mut st) = state.try_write() {
                    st.map.pit_lane_speed_pct = (pct / 100.0) as f64;
                }
            }
            // Read live, not from the frame-start snapshot: the import card runs
            // earlier in this same frame and would otherwise be overwritten with
            // the previous track's corner count.
            let mut turns = state.read().map.num_turns as f32;
            if super::widgets::number_row(
                ui,
                "Number of corners",
                &mut turns,
                0.0..=40.0,
                1.0,
                accent,
                None,
            ) {
                if let Some(mut st) = state.try_write() {
                    st.map.num_turns = turns as i32;
                }
            }
            let mut alias_str = snap
                .aliases
                .iter()
                .map(|a| a.to_string())
                .collect::<Vec<_>>()
                .join(", ");
            setting_row(ui, "Track ID aliases", None, |ui| {
                if text_field(ui, &mut alias_str, "e.g. 123, 456", 220.0).changed() {
                    let ids: Vec<i32> = alias_str
                        .split(|c: char| !c.is_ascii_digit() && c != '-')
                        .filter_map(|s| s.parse().ok())
                        .collect();
                    if let Some(mut st) = state.try_write() {
                        st.map.alias_ids = ids;
                    }
                }
            });

            let mut corner_edit = state.read().map.corner_edit;
            setting_row(ui, "Edit corner labels on map", None, |ui| {
                if toggle_switch(ui, &mut corner_edit, accent, ui.id().with("corner_edit"))
                    .changed()
                {
                    if let Some(mut st) = state.try_write() {
                        st.map.corner_edit = corner_edit;
                        st.map.sf_edit = false;
                        st.map.pit_edit = false;
                        st.map.interactive = corner_edit;
                        st.map.pit_sel = None;
                    }
                }
            });
            // Fine offset for the residual lead/lag left after SVG alignment: the
            // outline's arc length is not distributed quite like iRacing's
            // LapDistPct. Signed percent reads better than the raw 0..1 wrap.
            let mut nudge = {
                let sf = state.read().map.cached_start_finish * 100.0;
                if sf > 50.0 {
                    sf - 100.0
                } else {
                    sf
                }
            };
            if super::widgets::number_row(
                ui,
                "Map position offset (%)",
                &mut nudge,
                -50.0..=50.0,
                0.1,
                accent,
                Some("Positive pulls the dots back along the track. Save track after."),
            ) {
                if let Some(mut st) = state.try_write() {
                    st.map.cached_start_finish = (nudge / 100.0).rem_euclid(1.0);
                }
            }

            let mut sf_edit = state.read().map.sf_edit;
            setting_row(
                ui,
                "Calibrate map position",
                Some("While on track: click where you are on the map. Save track after."),
                |ui| {
                    if toggle_switch(ui, &mut sf_edit, accent, ui.id().with("sf_edit")).changed() {
                        if let Some(mut st) = state.try_write() {
                            st.map.sf_edit = sf_edit;
                            st.map.corner_edit = false;
                            st.map.pit_edit = false;
                            st.map.interactive = sf_edit;
                            st.map.pit_sel = None;
                        }
                    }
                },
            );
        });
    }

    ensure_admin_loaded(ui_state);
    paint_demo_track_admin(ui, ui_state, accent);
}

pub fn paint_demo_track_admin(ui: &mut Ui, ui_state: &mut SettingsUi, accent: Color32) {
    if !cloud::can_write() {
        return;
    }

    ui.add_space(8.0);
    enable_card(ui, "Community demo track", accent, |ui| {
        ui.label(
            RichText::new("Shared demo map used by --demo for everyone.")
                .size(11.0)
                .color(MUTED),
        );
        let mut tid = ui_state.demo_track_id as f32;
        if super::widgets::number_row(ui, "Track ID", &mut tid, 1.0..=99999.0, 1.0, accent, None) {
            ui_state.demo_track_id = tid as i64;
        }
        if !ui_state.demo_track_status.is_empty() {
            ui.label(
                RichText::new(&ui_state.demo_track_status)
                    .size(11.0)
                    .color(MUTED),
            );
        }
        if ui_state.demo_track_id > 0
            && button_kind(ui, "Save to cloud", ButtonKind::Primary).clicked()
        {
            let tid = ui_state.demo_track_id;
            ui_state.flash("Saving demo track…");
            std::thread::spawn(move || {
                let msg = match cloud::fetch_track(&json!(tid)) {
                    Ok(Some(doc)) => {
                        let name = doc
                            .get("name")
                            .and_then(|n| n.as_str())
                            .unwrap_or("")
                            .to_string();
                        match cloud::save_app_settings(&json!({
                            "demo_track_id": tid,
                            "demo_track_name": name,
                        })) {
                            Ok(_) => format!("Saved demo track {tid} ({name})"),
                            Err(e) => e.to_string(),
                        }
                    }
                    Ok(None) => format!("TrackID {tid} not found in cloud"),
                    Err(e) => e.to_string(),
                };
                enqueue_cloud_flash(msg);
            });
        }
    });
}

pub fn paint_pro_drivers_admin(ui: &mut Ui, ui_state: &mut SettingsUi, accent: Color32) {
    if !cloud::can_write() {
        return;
    }

    ui.add_space(8.0);
    enable_card(ui, "Professional drivers", accent, |ui| {
        ui.label(
            RichText::new("Star badge + accent in Relative / Standings.")
                .size(11.0)
                .color(MUTED),
        );
        // List
        let names: Vec<String> = ui_state
            .pro_drivers
            .iter()
            .filter_map(|d| {
                d.get("name")
                    .and_then(|n| n.as_str())
                    .map(|s| s.to_string())
            })
            .collect();
        if !names.is_empty() {
            let selected = ui_state
                .pro_sel
                .clone()
                .filter(|s| names.contains(s))
                .unwrap_or_else(|| names[0].clone());
            if let Some(next) = styled_combo(ui, "pro_list", &selected, &names, 200.0) {
                ui_state.pro_sel = Some(next.clone());
                if let Some(d) = ui_state
                    .pro_drivers
                    .iter()
                    .find(|d| d.get("name").and_then(|n| n.as_str()) == Some(next.as_str()))
                {
                    ui_state.pro_name = next;
                    ui_state.pro_aliases = d
                        .get("aliases")
                        .and_then(|a| a.as_array())
                        .map(|arr| {
                            arr.iter()
                                .filter_map(|v| v.as_str())
                                .collect::<Vec<_>>()
                                .join(", ")
                        })
                        .unwrap_or_default();
                }
            }
        }
        setting_row(ui, "Name", None, |ui| {
            let _ = text_field(ui, &mut ui_state.pro_name, "Driver name", 180.0);
        });
        setting_row(ui, "Aliases", None, |ui| {
            let _ = text_field(ui, &mut ui_state.pro_aliases, "comma-separated", 220.0);
        });
        ui.horizontal(|ui| {
            if !ui_state.pro_name.trim().is_empty()
                && button_kind(ui, "Add / Update", ButtonKind::GhostAccent).clicked()
            {
                let name = ui_state.pro_name.trim().to_string();
                if !name.is_empty() {
                    let aliases: Vec<Value> = ui_state
                        .pro_aliases
                        .split(',')
                        .map(|s| s.trim())
                        .filter(|s| !s.is_empty())
                        .map(|s| json!(s))
                        .collect();
                    let entry = json!({ "name": name, "aliases": aliases });
                    if let Some(pos) = ui_state
                        .pro_drivers
                        .iter()
                        .position(|d| d.get("name").and_then(|n| n.as_str()) == Some(name.as_str()))
                    {
                        ui_state.pro_drivers[pos] = entry;
                    } else {
                        ui_state.pro_drivers.push(entry);
                    }
                    ui_state.pro_sel = Some(name);
                }
            }
            if ui_state.pro_sel.is_some() && button_kind(ui, "Remove", ButtonKind::Warn).clicked() {
                if let Some(sel) = ui_state.pro_sel.clone() {
                    ui_state
                        .pro_drivers
                        .retain(|d| d.get("name").and_then(|n| n.as_str()) != Some(sel.as_str()));
                    ui_state.pro_sel = None;
                    ui_state.pro_name.clear();
                    ui_state.pro_aliases.clear();
                }
            }
            if button_kind(ui, "Import from results...", ButtonKind::Default).clicked() {
                if let Some(path) = rfd::FileDialog::new()
                    .add_filter("JSON", &["json"])
                    .pick_file()
                {
                    match std::fs::read_to_string(&path)
                        .map_err(|e| e.into())
                        .and_then(|t| crate::driver_groups::parse_event_result_names(&t))
                    {
                        Ok(names) => {
                            let (merged, added, skipped) =
                                crate::driver_groups::merge_names_into_members(
                                    &ui_state.pro_drivers,
                                    &names,
                                );
                            ui_state.pro_drivers = merged;
                            ui_state.flash(format!(
                                "Imported +{added}, skipped {skipped} duplicate{}",
                                if skipped == 1 { "" } else { "s" }
                            ));
                        }
                        Err(e) => ui_state.flash(e.to_string()),
                    }
                }
            }
            if !ui_state.pro_drivers.is_empty()
                && button_kind(ui, "Save to cloud", ButtonKind::Primary).clicked()
            {
                let drivers = crate::driver_groups::dedupe_members(&ui_state.pro_drivers);
                ui_state.pro_drivers = drivers.clone();
                ui_state.flash("Saving pro drivers…");
                std::thread::spawn(move || {
                    let msg = match cloud::save_app_settings(&json!({ "pro_drivers": drivers })) {
                        Ok(_) => "Pro drivers saved".into(),
                        Err(e) => e.to_string(),
                    };
                    enqueue_cloud_flash(msg);
                });
            }
        });
    });
}

pub fn paint_about(ui: &mut Ui, ui_state: &mut SettingsUi, accent: Color32) {
    enable_card(ui, "About", accent, |ui| {
        ui.label(
            RichText::new(format!("GridGlance {}", updater::VERSION))
                .size(13.0)
                .color(TITLE),
        );
        ui.horizontal(|ui| {
            if button_kind(ui, "Check for updates", ButtonKind::Default).clicked() {
                match updater::fetch_latest(6) {
                    Ok(Some(info)) => {
                        if updater::is_newer(&info.version, updater::VERSION) {
                            ui_state.flash(format!("Update available: {}", info.version));
                            if let Some(url) = info.url {
                                ui_state.update_url = Some(url);
                            }
                        } else {
                            ui_state.flash("You're up to date");
                        }
                    }
                    Ok(None) => ui_state.flash("Update check disabled (dev build)"),
                    Err(e) => ui_state.flash(e.to_string()),
                }
            }
            if ui_state.update_url.is_some()
                && button_kind(ui, "Download update", ButtonKind::Primary).clicked()
            {
                if let Some(url) = ui_state.update_url.clone() {
                    match updater::download_installer(&url) {
                        Ok(path) => {
                            ui_state.flash(format!("Downloaded {}", path.display()));
                            let _ = std::process::Command::new(&path).spawn();
                        }
                        Err(e) => ui_state.flash(e.to_string()),
                    }
                }
            }
            if updater::uninstaller_available()
                && button_kind(ui, "Uninstall…", ButtonKind::Danger).clicked()
            {
                match updater::launch_uninstaller() {
                    Ok(()) => ui_state.flash("Launching uninstaller"),
                    Err(e) => ui_state.flash(e.to_string()),
                }
            }
        });
    });
}

pub fn ensure_admin_loaded(ui_state: &mut SettingsUi) {
    // Apply any finished background fetch.
    if let Some(pending) = take_admin_result() {
        ui_state.admin_loading = false;
        ui_state.admin_loaded = true;
        match pending {
            Ok(s) => apply_admin_settings(ui_state, &s),
            Err(e) => {
                ui_state.demo_track_status = format!("Cloud settings: {e}");
            }
        }
    }
    // Prefer local cache immediately (no UI freeze).
    if !ui_state.admin_loaded {
        if let Some(cached) = cloud::load_app_settings_cache() {
            apply_admin_settings(ui_state, &cached);
            ui_state.admin_loaded = true;
        }
    }
    // Kick a single background refresh from cloud.
    if ADMIN_FETCH_STARTED
        .compare_exchange(
            false,
            true,
            std::sync::atomic::Ordering::SeqCst,
            std::sync::atomic::Ordering::SeqCst,
        )
        .is_ok()
    {
        ui_state.admin_loading = true;
        std::thread::spawn(|| {
            let result = cloud::fetch_app_settings().map_err(|e| e.to_string());
            store_admin_result(result);
        });
    }
}

fn apply_admin_settings(ui_state: &mut SettingsUi, s: &serde_json::Value) {
    if let Some(id) = s.get("demo_track_id").and_then(|v| {
        v.as_i64()
            .or_else(|| v.as_u64().map(|u| u as i64))
            .or_else(|| v.as_str()?.parse().ok())
    }) {
        ui_state.demo_track_id = id;
    }
    let name = s
        .get("demo_track_name")
        .and_then(|n| n.as_str())
        .unwrap_or("");
    ui_state.demo_track_status = if ui_state.demo_track_id > 0 {
        format!("Current: {} ({name})", ui_state.demo_track_id)
    } else {
        "No demo track set".into()
    };
    if let Some(arr) = s.get("pro_drivers").and_then(|a| a.as_array()) {
        ui_state.pro_drivers = arr.clone();
    }
}

static ADMIN_FETCH_STARTED: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

static ADMIN_RESULT: once_cell::sync::Lazy<
    std::sync::Mutex<Option<Result<serde_json::Value, String>>>,
> = once_cell::sync::Lazy::new(|| std::sync::Mutex::new(None));

fn store_admin_result(r: Result<serde_json::Value, String>) {
    if let Ok(mut g) = ADMIN_RESULT.lock() {
        *g = Some(r);
    }
}

fn take_admin_result() -> Option<Result<serde_json::Value, String>> {
    ADMIN_RESULT.lock().ok().and_then(|mut g| g.take())
}

static CLOUD_FLASH: once_cell::sync::Lazy<std::sync::Mutex<Option<String>>> =
    once_cell::sync::Lazy::new(|| std::sync::Mutex::new(None));

/// Poll background cloud save flashes into the status line.
pub fn poll_cloud_jobs(ui_state: &mut SettingsUi) {
    if let Ok(mut g) = CLOUD_FLASH.lock() {
        if let Some(msg) = g.take() {
            ui_state.flash(msg);
        }
    }
}

fn enqueue_cloud_flash(msg: String) {
    if let Ok(mut g) = CLOUD_FLASH.lock() {
        *g = Some(msg);
    }
}

fn import_html(state: &StateHandle, path: &str) -> anyhow::Result<String> {
    if path.is_empty() {
        anyhow::bail!("Choose an HTML file first");
    }
    let doc = tracks::import_track_source(std::path::Path::new(path), 400, 4, 0.0)?;
    let mut st = state
        .try_write()
        .ok_or_else(|| anyhow::anyhow!("state busy"))?;
    // Reset map orientation to match members preview.
    {
        let cfg = Arc::make_mut(&mut st.config);
        if let Some(map) = cfg.cfg.get_mut("map").and_then(|m| m.as_object_mut()) {
            map.insert("rotation".into(), json!(0));
            map.insert("mirror".into(), json!(false));
        }
    }
    st.map.invalidate_track_cache();
    st.map.cached_path = doc.points.clone();
    st.map.cached_self_crossing = crate::track_path::loop_self_crossing(&st.map.cached_path);
    st.map.cached_track_name = doc.name.clone();
    st.map.cached_start_finish = doc.start_finish;
    st.map.cached_corners = doc
        .corners
        .iter()
        .filter_map(|c| {
            Some(crate::track_path::CornerMark {
                pct: c.get("pct").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32,
                label: c.get("label").and_then(|v| v.as_str())?.to_string(),
                ox: 0.0,
                oy: 0.0,
            })
        })
        .collect();
    if let Some(n) = doc.num_turns {
        st.map.num_turns = n as i32;
    } else {
        st.map.num_turns = st.map.cached_corners.len() as i32;
    }
    // New HTML geometry invalidates lap-% → arc calibration. Restoring the old
    // `pct_map` (or leaving Ready with a stale table) is what kept Iowa ~ahead
    // after import — the table was built for a different winding/outline.
    st.map.cached_pct_map = None;
    let mut cal_note = " (cal cleared — recalibrate)";
    if let Some(id) = doc.track_id {
        st.map.cached_track_id = Some(id as i32);
        let mut f = (*st.frame).clone();
        f.track_id = Some(id as i32);
        st.frame = Arc::new(f);
        if let Some(existing) = crate::track_path::load_for_track_id(id as i32) {
            if loop_points_compatible(&existing.points, &doc.points) {
                if let Some(pm) = existing.pct_map {
                    st.map.cached_pct_map = Some(pm);
                    cal_note = " + cal";
                }
            }
        }
    }
    // Must be Ready — otherwise ensure_path_cached reloads disk and wipes the import
    // (Iowa kept the old full-width yellow pit_in from 559.json).
    st.map.path_status = crate::state::TrackPathStatus::Ready;
    st.map.import_hold = true;

    // Seed pit from HTML `#Pitroad` / `#Mergeline` when stitching succeeded.
    let pit_note = if let Some(pit) = doc.pit.clone() {
        st.map.cached_pit = pit;
        st.map.cached_pit_out_pct = st.map.cached_pit.out_pct;
        st.map.cached_pit2 = crate::track_path::PitLane::default();
        let _ = st.map.load_pit_from_cache(true);
        st.map.pit_latch_seed_pending = true;
        " + pit road"
    } else {
        st.map.cached_pit = crate::track_path::PitLane::default();
        st.map.cached_pit2 = crate::track_path::PitLane::default();
        st.map.cached_pit_out_pct = None;
        st.map.entry_pts.clear();
        st.map.road_pts.clear();
        st.map.merge_pts.clear();
        st.map.entry_pts_2.clear();
        st.map.road_pts_2.clear();
        st.map.merge_pts_2.clear();
        " (no usable pit layer)"
    };

    Ok(format!(
        "Imported {} ({} pts, {} corners){}{}{}",
        doc.name,
        st.map.cached_path.len(),
        st.map.cached_corners.len(),
        doc.track_id
            .map(|t| format!(" TrackID {t}"))
            .unwrap_or_default(),
        pit_note,
        cal_note
    ))
}

fn map_orientation(state: &StateHandle) -> (i32, bool) {
    let st = state.read();
    let rot = st
        .config
        .cfg
        .get("map")
        .and_then(|m| m.get("rotation"))
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);
    let rot = ((rot / 90.0).round() as i32 * 90).rem_euclid(360);
    let mirror = st
        .config
        .cfg
        .get("map")
        .and_then(|m| m.get("mirror"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    (rot, mirror)
}

fn corners_json(state: &StateHandle) -> Vec<Value> {
    state
        .read()
        .map
        .cached_corners
        .iter()
        .map(|c| json!({ "pct": c.pct, "label": c.label }))
        .collect()
}

fn save_loop(state: &StateHandle) -> String {
    let (rot, mirror) = map_orientation(state);
    let st = state.read();
    let tid = st
        .map
        .cached_track_id
        .or(st.frame.track_id)
        .map(|t| json!(t));
    let r = tracks::save_loop_only(
        &paths::tracks_dir(),
        tid.as_ref(),
        &st.map.cached_path,
        Some(&st.map.cached_track_name),
        st.map.cached_start_finish,
        &corners_json(state),
        if st.map.num_turns > 0 {
            Some(st.map.num_turns as i64)
        } else {
            None
        },
        &st.map.alias_ids,
        rot,
        mirror,
        true,
    );
    r.msg
}

fn save_pit(state: &StateHandle) -> String {
    let st = state.read();
    let tid = st
        .map
        .cached_track_id
        .or(st.frame.track_id)
        .map(|t| json!(t));
    let r = tracks::save_pit_patch(
        &paths::tracks_dir(),
        tid.as_ref(),
        &st.map.cached_path,
        &st.map.entry_pts,
        &st.map.road_pts,
        &st.map.merge_pts,
        &st.map.entry_pts_2,
        &st.map.road_pts_2,
        &st.map.merge_pts_2,
        st.map.pit_speed_ms as f32,
        st.map.pit_lane_speed_pct as f32,
        st.map.cached_pit2.lane_speed_pct,
        true,
    );
    r.msg
}

fn save_track(state: &StateHandle) -> String {
    let (rot, mirror) = map_orientation(state);
    let st = state.read();
    let tid = st
        .map
        .cached_track_id
        .or(st.frame.track_id)
        .map(|t| json!(t));
    let r = tracks::save_manual_track(
        &paths::tracks_dir(),
        tid.as_ref(),
        &st.map.cached_path,
        &st.map.entry_pts,
        &st.map.road_pts,
        &st.map.merge_pts,
        &st.map.entry_pts_2,
        &st.map.road_pts_2,
        &st.map.merge_pts_2,
        Some(&st.map.cached_track_name),
        st.map.cached_start_finish,
        &corners_json(state),
        if st.map.num_turns > 0 {
            Some(st.map.num_turns as i64)
        } else {
            None
        },
        &st.map.alias_ids,
        st.map.pit_speed_ms as f32,
        st.map.pit_lane_speed_pct as f32,
        st.map.cached_pit2.lane_speed_pct,
        rot,
        mirror,
        true,
    );
    r.msg
}

fn loop_points_compatible(a: &[(f32, f32)], b: &[(f32, f32)]) -> bool {
    if a.len() != b.len() || a.len() < 3 {
        return false;
    }
    for &i in &[0usize, a.len() / 2, a.len() - 1] {
        if (a[i].0 - b[i].0).abs() + (a[i].1 - b[i].1).abs() > 1e-4 {
            return false;
        }
    }
    true
}
