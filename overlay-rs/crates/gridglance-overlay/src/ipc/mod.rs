//! Local TCP JSON-RPC server for Python settings.

use crate::config::OverlayConfig;
use crate::state::StateHandle;
use anyhow::Result;
use gridglance_ipc::{
    methods, ConfigApplyParams, LayoutSetParams, MapAliasIdsParams, MapBoolParams,
    MapClearPitParams, MapLaneSpeedParams, MapLoadPitParams, MapNumTurnsParams, MapPitEditParams,
    MapSetCornersParams, MapSetLoopParams, MapSetStartFinishParams, MapSpeedParams,
    OverlayModeParams, PingResult, Request, Response, PROTOCOL_VERSION,
};
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

const LOCK_WAIT: Duration = Duration::from_millis(100);

pub fn spawn(state: StateHandle, port: u16) -> Result<()> {
    let listener = TcpListener::bind(("127.0.0.1", port))?;
    listener.set_nonblocking(false)?;
    thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let st = state.clone();
            thread::spawn(move || {
                if let Err(e) = handle_client(st, stream) {
                    eprintln!("ipc client error: {e}");
                }
            });
        }
    });
    eprintln!("GridGlance IPC listening on 127.0.0.1:{port}");
    Ok(())
}

fn handle_client(state: StateHandle, stream: TcpStream) -> Result<()> {
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut writer = stream;
    let mut line = String::new();
    loop {
        line.clear();
        let n = reader.read_line(&mut line)?;
        if n == 0 {
            break;
        }
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let req: Request = match serde_json::from_str(trimmed) {
            Ok(r) => r,
            Err(e) => {
                let resp = Response::err(0, format!("bad request: {e}"));
                writeln!(writer, "{}", serde_json::to_string(&resp)?)?;
                continue;
            }
        };
        let resp = dispatch(&state, req);
        writeln!(writer, "{}", serde_json::to_string(&resp)?)?;
        writer.flush()?;
    }
    Ok(())
}

fn dispatch(state: &StateHandle, req: Request) -> Response {
    let id = req.id;
    match req.method.as_str() {
        methods::PING => {
            let gen = state.read().config.generation;
            Response::ok(
                id,
                serde_json::to_value(PingResult {
                    version: PROTOCOL_VERSION,
                    backend: "rust".into(),
                    generation: gen,
                })
                .unwrap_or(Value::Null),
            )
        }
        methods::CONFIG_RELOAD => {
            // Load from disk outside the lock, then swap Arc under a short write.
            match OverlayConfig::load() {
                Ok(mut next) => match state.try_write_for(LOCK_WAIT) {
                    Some(mut st) => {
                        let gen = st.config.generation.saturating_add(1);
                        next.generation = gen;
                        st.config = Arc::new(next);
                        Response::ok(id, json!({"generation": gen}))
                    }
                    None => Response::err(id, "busy"),
                },
                Err(e) => Response::err(id, e.to_string()),
            }
        }
        methods::CONFIG_APPLY => {
            let params: ConfigApplyParams = serde_json::from_value(req.params).unwrap_or_default();
            match state.try_write_for(LOCK_WAIT) {
                Some(mut st) => {
                    let cfg = Arc::make_mut(&mut st.config);
                    if !params.cfg.is_null() {
                        cfg.apply_cfg_patch(&params.cfg);
                    }
                    if let Some(g) = params.generation {
                        cfg.generation = g;
                    }
                    Response::ok(id, json!({"generation": cfg.generation}))
                }
                None => Response::err(id, "busy"),
            }
        }
        methods::OVERLAY_START => match state.try_write_for(LOCK_WAIT) {
            Some(mut st) => {
                st.running = true;
                Response::ok(id, json!({"running": true}))
            }
            None => Response::err(id, "busy"),
        },
        methods::OVERLAY_STOP => match state.try_write_for(LOCK_WAIT) {
            Some(mut st) => {
                st.running = false;
                Response::ok(id, json!({"running": false}))
            }
            None => Response::err(id, "busy"),
        },
        methods::OVERLAY_SET_EDIT_MODE => {
            let params: OverlayModeParams =
                serde_json::from_value(req.params).unwrap_or_default();
            let mut st = state.write();
            if let Some(e) = params.edit_mode {
                st.edit_mode = e;
                st.click_through = !e;
            }
            if let Some(c) = params.click_through {
                st.click_through = c;
                st.edit_mode = !c;
            }
            if let Some(r) = params.running {
                st.running = r;
            }
            Response::ok(
                id,
                json!({
                    "edit_mode": st.edit_mode,
                    "click_through": st.click_through,
                    "running": st.running
                }),
            )
        }
        methods::LAYOUT_GET => {
            let st = state.read();
            let mut map = serde_json::Map::new();
            for (k, g) in &st.layout {
                map.insert(k.clone(), json!({"x": g.x, "y": g.y, "w": g.w, "h": g.h}));
            }
            Response::ok(id, Value::Object(map))
        }
        methods::LAYOUT_SET => {
            let params: LayoutSetParams = match serde_json::from_value(req.params) {
                Ok(p) => p,
                Err(e) => return Response::err(id, e.to_string()),
            };
            let mut st = state.write();
            st.layout.insert(
                params.key,
                crate::state::PanelLayout {
                    x: params.geom.x,
                    y: params.geom.y,
                    w: params.geom.w,
                    h: params.geom.h,
                },
            );
            st.save_layout();
            Response::ok(id, json!({"saved": true}))
        }
        methods::MAP_SET_PIT_EDIT => {
            let params: MapPitEditParams =
                serde_json::from_value(req.params).unwrap_or_default();
            let mut st = state.write();
            let was = st.map.pit_edit;
            st.map.pit_edit = params.enabled;
            st.map.phase = params.phase;
            st.map.lane = params.lane;
            if params.enabled {
                st.map.interactive = true;
                st.map.corner_edit = false;
                st.map.sf_edit = false;
                if !was {
                    st.map.reset_pit_edit_view();
                }
                let lane2 = st.map.lane_is_2();
                if st.map.phase_key() == "road" {
                    st.map.seed_road_from_entry(lane2);
                }
            } else {
                st.map.pit_drag = None;
            }
            Response::ok(id, st.map_state_json())
        }
        methods::MAP_UNDO_POINT => {
            let mut st = state.write();
            let pts = st.map.active_pts_mut();
            pts.pop();
            let n = pts.len();
            Response::ok(id, json!({"points": n}))
        }
        methods::MAP_CLEAR_PIT => {
            let params: MapClearPitParams =
                serde_json::from_value(req.params).unwrap_or_default();
            let mut st = state.write();
            let phase = params
                .phase
                .unwrap_or_else(|| st.map.phase_key().to_string());
            let lane2 = params.lane.as_deref().map(|l| matches!(l, "2" | "secondary"));
            st.map.clear_phase(&phase, lane2);
            Response::ok(id, st.map_state_json())
        }
        methods::MAP_RESET_VIEW => {
            let mut st = state.write();
            st.map.reset_pit_edit_view();
            Response::ok(id, json!({"reset": true}))
        }
        methods::MAP_SAVE_PIT => {
            let st = state.read();
            let n = st.map.entry_pts.len()
                + st.map.road_pts.len()
                + st.map.merge_pts.len()
                + st.map.entry_pts_2.len()
                + st.map.road_pts_2.len()
                + st.map.merge_pts_2.len();
            Response::ok(
                id,
                json!({"ok": true, "points": n, "msg": "pit draft held in overlay"}),
            )
        }
        methods::MAP_SAVE_LOOP => Response::ok(id, json!({"ok": true})),
        methods::MAP_INVALIDATE_TRACK => {
            state.write().map.invalidate_track_cache();
            Response::ok(id, json!({"ok": true}))
        }
        methods::MAP_LOAD_PIT => {
            let params: MapLoadPitParams =
                serde_json::from_value(req.params).unwrap_or_default();
            let mut st = state.write();
            // Ensure cache is warm from disk when possible.
            if st.map.cached_path.is_empty() {
                if let Some(id) = st.frame.track_id {
                    if let Some(tp) = crate::track_path::load_for_track_id(id) {
                        st.map.cached_track_id = Some(id);
                        st.map.cached_path = tp.points;
                        st.map.cached_track_name = tp.name;
                        st.map.cached_start_finish = tp.start_finish;
                        st.map.cached_pit_out_pct = tp.pit_out_pct;
                        st.map.cached_pit = tp.pit;
                        st.map.cached_pit2 = tp.pit2;
                        st.map.cached_drs_zones = tp.drs_zones;
                        st.map.cached_p2p_zones = tp.p2p_zones;
                        st.map.cached_corners = tp.corners;
                        if st.map.cached_pit.lane_speed_pct > 0.0 {
                            st.map.pit_lane_speed_pct =
                                st.map.cached_pit.lane_speed_pct as f64;
                        }
                    }
                }
            }
            let ok = st.map.load_pit_from_cache(params.force);
            let mut out = st.map_state_json();
            if let Some(obj) = out.as_object_mut() {
                obj.insert("loaded".into(), json!(ok));
            }
            Response::ok(id, out)
        }
        methods::MAP_SET_LOOP => {
            let params: MapSetLoopParams = match serde_json::from_value(req.params) {
                Ok(p) => p,
                Err(e) => return Response::err(id, e.to_string()),
            };
            let mut pts: Vec<(f32, f32)> = params
                .points
                .iter()
                .map(|p| (p[0] as f32, p[1] as f32))
                .filter(|(x, y)| x.is_finite() && y.is_finite())
                .collect();
            if pts.len() < 3 {
                return Response::err(id, "loop needs at least 3 points");
            }
            let corners = parse_corner_values(&params.corners);
            let mut st = state.write();
            st.frame = {
                let mut f = (*st.frame).clone();
                f.track_id = Some(params.track_id);
                if !params.name.is_empty() {
                    f.track_name = Some(params.name.clone());
                }
                Arc::new(f)
            };
            st.map.cached_track_id = Some(params.track_id);
            st.map.cached_path = std::mem::take(&mut pts);
            st.map.cached_track_name = params.name;
            st.map.cached_start_finish = params.start_finish as f32;
            st.map.cached_corners = corners;
            st.map.cached_pit = crate::track_path::PitLane::default();
            st.map.cached_pit2 = crate::track_path::PitLane::default();
            st.map.cached_pit_out_pct = None;
            st.map.clear_phase("all", None);
            if let Some(n) = params.num_turns {
                st.map.num_turns = n;
            }
            Response::ok(id, st.map_state_json())
        }
        methods::MAP_SET_CORNERS => {
            let params: MapSetCornersParams =
                serde_json::from_value(req.params).unwrap_or_default();
            let mut st = state.write();
            st.map.cached_corners = parse_corner_values(&params.corners);
            Response::ok(id, st.map_state_json())
        }
        methods::MAP_SET_START_FINISH => {
            let params: MapSetStartFinishParams = match serde_json::from_value(req.params) {
                Ok(p) => p,
                Err(e) => return Response::err(id, e.to_string()),
            };
            let mut st = state.write();
            st.map.cached_start_finish = (params.pct as f32).rem_euclid(1.0);
            Response::ok(id, st.map_state_json())
        }
        methods::MAP_SET_INTERACTIVE => {
            let params: MapBoolParams = serde_json::from_value(req.params).unwrap_or_default();
            state.write().map.interactive = params.enabled;
            Response::ok(id, json!({"interactive": params.enabled}))
        }
        methods::MAP_GET_STATE | methods::TRACK_AUTHORING_STATE => {
            Response::ok(id, state.read().map_state_json())
        }
        methods::MAP_SET_CORNER_EDIT => {
            let params: MapBoolParams = serde_json::from_value(req.params).unwrap_or_default();
            let mut st = state.write();
            st.map.corner_edit = params.enabled;
            if params.enabled {
                st.map.pit_edit = false;
                st.map.sf_edit = false;
                st.map.interactive = true;
            }
            Response::ok(id, st.map_state_json())
        }
        methods::MAP_SET_SF_EDIT => {
            let params: MapBoolParams = serde_json::from_value(req.params).unwrap_or_default();
            let mut st = state.write();
            st.map.sf_edit = params.enabled;
            if params.enabled {
                st.map.pit_edit = false;
                st.map.corner_edit = false;
                st.map.interactive = true;
            }
            Response::ok(id, st.map_state_json())
        }
        methods::MAP_SET_PIT_SPEED => {
            let params: MapSpeedParams = match serde_json::from_value(req.params) {
                Ok(p) => p,
                Err(e) => return Response::err(id, e.to_string()),
            };
            state.write().map.pit_speed_ms = params.speed_ms;
            Response::ok(id, json!({"speed_ms": params.speed_ms}))
        }
        methods::MAP_SET_PIT_LANE_SPEED => {
            let params: MapLaneSpeedParams = match serde_json::from_value(req.params) {
                Ok(p) => p,
                Err(e) => return Response::err(id, e.to_string()),
            };
            state.write().map.pit_lane_speed_pct = params.pct;
            Response::ok(id, json!({"pct": params.pct}))
        }
        methods::MAP_SET_NUM_TURNS => {
            let params: MapNumTurnsParams = match serde_json::from_value(req.params) {
                Ok(p) => p,
                Err(e) => return Response::err(id, e.to_string()),
            };
            state.write().map.num_turns = params.n;
            Response::ok(id, json!({"n": params.n}))
        }
        methods::MAP_SET_ALIAS_IDS => {
            let params: MapAliasIdsParams =
                serde_json::from_value(req.params).unwrap_or_default();
            state.write().map.alias_ids = params.ids.clone();
            Response::ok(id, json!({"ids": params.ids}))
        }
        other => Response::err(id, format!("unknown method: {other}")),
    }
}

fn parse_corner_values(vals: &[Value]) -> Vec<crate::track_path::CornerMark> {
    let mut out = Vec::new();
    for (i, c) in vals.iter().enumerate() {
        let pct = c.get("pct").and_then(|x| x.as_f64()).unwrap_or(0.0) as f32;
        let label = if let Some(s) = c.get("label").and_then(|x| x.as_str()) {
            s.to_string()
        } else if let Some(n) = c.get("label").and_then(|x| x.as_i64()) {
            n.to_string()
        } else {
            (i + 1).to_string()
        };
        let ox = c.get("ox").and_then(|x| x.as_f64()).unwrap_or(0.0) as f32;
        let oy = c.get("oy").and_then(|x| x.as_f64()).unwrap_or(0.0) as f32;
        if pct.is_finite() {
            out.push(crate::track_path::CornerMark {
                pct: pct.rem_euclid(1.0),
                label,
                ox,
                oy,
            });
        }
    }
    out
}
