//! Track Scan Settings page + cloud admin cards.

use crate::cloud;
use crate::paths;
use crate::state::StateHandle;
use crate::tracks;
use crate::updater;
use egui::{Color32, RichText, Ui};
use serde_json::{json, Value};
use std::sync::Arc;

use super::theme::{MUTED, TEXT, TITLE};
use super::widgets::{
    button_kind, enable_card, info_card, setting_row, styled_combo, text_field, toggle_switch,
    ButtonKind,
};
use super::SettingsUi;

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

    let (track_id, track_name, can_author, phase, lane, pit_speed, lane_pct, num_turns, aliases) = {
        let st = state.read();
        (
            st.map.cached_track_id.or(st.frame.track_id),
            st.map.cached_track_name.clone(),
            !st.map.cached_path.is_empty() || st.frame.track_id.is_some(),
            st.map.phase_key().to_string(),
            if st.map.lane_is_2() {
                "2".to_string()
            } else {
                "1".to_string()
            },
            st.map.pit_speed_ms as f32,
            st.map.pit_lane_speed_pct as f32,
            st.map.num_turns,
            st.map.alias_ids.clone(),
        )
    };

    enable_card(ui, "Session", accent, |ui| {
        ui.label(
            RichText::new(format!(
                "TrackID: {}  ·  {}",
                track_id
                    .map(|t| t.to_string())
                    .unwrap_or_else(|| "—".into()),
                if track_name.is_empty() {
                    "no name"
                } else {
                    track_name.as_str()
                }
            ))
            .size(12.0)
            .color(TEXT),
        );
        if !can_author {
            ui.label(
                RichText::new("Join a session or import HTML to author.")
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
        if button_kind(ui, "Import loop", ButtonKind::Primary).clicked() {
            match import_html(state, &ui_state.html_path) {
                Ok(msg) => ui_state.flash(msg),
                Err(e) => ui_state.flash(e.to_string()),
            }
        }
    });

    ui.add_space(8.0);
    enable_card(ui, "Edit pit on map", accent, |ui| {
        let mut editing = {
            let st = state.read();
            st.map.interactive && !st.map.corner_edit && !st.map.sf_edit
        };
        setting_row(ui, "Enable pit edit", None, |ui| {
            if toggle_switch(ui, &mut editing, accent, ui.id().with("pit_edit")).changed() {
                if let Some(mut st) = state.try_write() {
                    st.map.interactive = editing;
                    st.map.corner_edit = false;
                    st.map.sf_edit = false;
                    if editing {
                        let _ = st.map.load_pit_from_cache(false);
                    }
                }
            }
        });

        let phases = ["entry", "road", "merge"];
        let mut phase = phase;
        setting_row(ui, "Phase", None, |ui| {
            let opts: Vec<String> = phases.iter().map(|s| (*s).into()).collect();
            if let Some(next) = styled_combo(ui, "pit_phase", &phase, &opts, 120.0) {
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

        let lanes = ["1", "2"];
        let mut lane = lane;
        setting_row(ui, "Lane", None, |ui| {
            let opts: Vec<String> = lanes.iter().map(|s| (*s).into()).collect();
            if let Some(next) = styled_combo(ui, "pit_lane", &lane, &opts, 80.0) {
                lane = next;
                if let Some(mut st) = state.try_write() {
                    st.map.lane = if lane == "2" { "2".into() } else { "1".into() };
                }
            }
        });

        ui.horizontal(|ui| {
            if button_kind(ui, "Load saved pit", ButtonKind::Default).clicked() {
                if let Some(mut st) = state.try_write() {
                    let ok = st.map.load_pit_from_cache(true);
                    ui_state.flash(if ok {
                        "Loaded pit from track file"
                    } else {
                        "No saved pit geometry"
                    });
                }
            }
            if button_kind(ui, "Undo point", ButtonKind::Default).clicked() {
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
        ui.horizontal(|ui| {
            if button_kind(ui, "Clear selected", ButtonKind::Default).clicked() {
                if let Some(mut st) = state.try_write() {
                    let phase = st.map.phase_key().to_string();
                    let lane2 = st.map.lane_is_2();
                    st.map.clear_phase(&phase, Some(lane2));
                }
            }
            if button_kind(ui, "Clear all pit", ButtonKind::Warn).clicked() {
                if let Some(mut st) = state.try_write() {
                    st.map.clear_phase("all", None);
                }
            }
        });
        ui.horizontal(|ui| {
            if button_kind(ui, "Save loop", ButtonKind::GhostAccent).clicked() {
                ui_state.flash(save_loop(state));
            }
            if button_kind(ui, "Save pit", ButtonKind::GhostAccent).clicked() {
                ui_state.flash(save_pit(state));
            }
            if button_kind(ui, "Save track", ButtonKind::Primary).clicked() {
                ui_state.flash(save_track(state));
            }
        });
    });

    ui.add_space(8.0);
    enable_card(ui, "Track metadata", accent, |ui| {
        let mut speed = pit_speed;
        if super::widgets::number_row(
            ui,
            "Pit speed limit (m/s)",
            &mut speed,
            5.0..=40.0,
            0.5,
            accent,
            None,
        ) {
            if let Some(mut st) = state.try_write() {
                st.map.pit_speed_ms = speed as f64;
            }
        }
        let mut pct = lane_pct * 100.0;
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
        let mut turns = num_turns as f32;
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
        let mut alias_str = aliases
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
            if toggle_switch(ui, &mut corner_edit, accent, ui.id().with("corner_edit")).changed() {
                if let Some(mut st) = state.try_write() {
                    st.map.corner_edit = corner_edit;
                    st.map.sf_edit = false;
                    st.map.interactive = corner_edit;
                }
            }
        });
        let mut sf_edit = state.read().map.sf_edit;
        setting_row(ui, "Edit start/finish on map", None, |ui| {
            if toggle_switch(ui, &mut sf_edit, accent, ui.id().with("sf_edit")).changed() {
                if let Some(mut st) = state.try_write() {
                    st.map.sf_edit = sf_edit;
                    st.map.corner_edit = false;
                    st.map.interactive = sf_edit;
                }
            }
        });
    });
}

pub fn paint_cloud_admin(ui: &mut Ui, ui_state: &mut SettingsUi, accent: Color32) {
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
        if button_kind(ui, "Save to cloud", ButtonKind::Primary).clicked() {
            let tid = ui_state.demo_track_id;
            match cloud::fetch_track(&json!(tid)) {
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
                        Ok(_) => {
                            ui_state.demo_track_status = format!("Saved demo track {tid} ({name})");
                            ui_state.flash("Demo track saved");
                        }
                        Err(e) => ui_state.flash(e.to_string()),
                    }
                }
                Ok(None) => ui_state.flash(format!("TrackID {tid} not found in cloud")),
                Err(e) => ui_state.flash(e.to_string()),
            }
        }
    });

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
            if button_kind(ui, "Add / Update", ButtonKind::GhostAccent).clicked() {
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
            if button_kind(ui, "Remove", ButtonKind::Warn).clicked() {
                if let Some(sel) = ui_state.pro_sel.clone() {
                    ui_state
                        .pro_drivers
                        .retain(|d| d.get("name").and_then(|n| n.as_str()) != Some(sel.as_str()));
                    ui_state.pro_sel = None;
                    ui_state.pro_name.clear();
                    ui_state.pro_aliases.clear();
                }
            }
            if button_kind(ui, "Save to cloud", ButtonKind::Primary).clicked() {
                match cloud::save_app_settings(&json!({ "pro_drivers": ui_state.pro_drivers })) {
                    Ok(_) => ui_state.flash("Pro drivers saved"),
                    Err(e) => ui_state.flash(e.to_string()),
                }
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
            if button_kind(ui, "Uninstall…", ButtonKind::Danger).clicked() {
                match updater::launch_uninstaller() {
                    Ok(()) => ui_state.flash("Launching uninstaller"),
                    Err(e) => ui_state.flash(e.to_string()),
                }
            }
        });
    });
}

pub fn ensure_admin_loaded(ui_state: &mut SettingsUi) {
    if ui_state.admin_loaded {
        return;
    }
    ui_state.admin_loaded = true;
    match cloud::fetch_app_settings() {
        Ok(s) => {
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
        Err(e) => {
            ui_state.demo_track_status = format!("Cloud settings: {e}");
        }
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
    if let Some(id) = doc.track_id {
        st.map.cached_track_id = Some(id as i32);
        let mut f = (*st.frame).clone();
        f.track_id = Some(id as i32);
        st.frame = Arc::new(f);
    }
    Ok(format!(
        "Imported {} ({} pts, {} corners){}",
        doc.name,
        st.map.cached_path.len(),
        st.map.cached_corners.len(),
        doc.track_id
            .map(|t| format!(" TrackID {t}"))
            .unwrap_or_default()
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
    if r.ok {
        drop(st);
        if let Some(mut st) = state.try_write() {
            st.map.invalidate_track_cache();
        }
    }
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
    if r.ok {
        drop(st);
        if let Some(mut st) = state.try_write() {
            st.map.invalidate_track_cache();
        }
    }
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
    if r.ok {
        drop(st);
        if let Some(mut st) = state.try_write() {
            st.map.invalidate_track_cache();
        }
    }
    r.msg
}
