//! Local TCP JSON-RPC server for Python settings.

use crate::state::StateHandle;
use anyhow::Result;
use gridglance_ipc::{
    methods, ConfigApplyParams, LayoutSetParams, MapAliasIdsParams, MapBoolParams,
    MapLaneSpeedParams, MapNumTurnsParams, MapPitEditParams, MapSpeedParams, OverlayModeParams,
    PingResult, Request, Response, PROTOCOL_VERSION, DEFAULT_IPC_PORT,
};
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::thread;

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

pub fn spawn_default(state: StateHandle) -> Result<()> {
    spawn(state, DEFAULT_IPC_PORT)
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
        methods::CONFIG_RELOAD => match state.write().config.reload() {
            Ok(()) => Response::ok(id, json!({"generation": state.read().config.generation})),
            Err(e) => Response::err(id, e.to_string()),
        },
        methods::CONFIG_APPLY => {
            let params: ConfigApplyParams = serde_json::from_value(req.params).unwrap_or_default();
            let mut st = state.write();
            if !params.cfg.is_null() {
                st.config.apply_cfg_patch(&params.cfg);
            }
            if let Some(g) = params.generation {
                st.config.generation = g;
            }
            Response::ok(id, json!({"generation": st.config.generation}))
        }
        methods::OVERLAY_START => {
            state.write().running = true;
            Response::ok(id, json!({"running": true}))
        }
        methods::OVERLAY_STOP => {
            state.write().running = false;
            Response::ok(id, json!({"running": false}))
        }
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
            st.map.pit_edit = params.enabled;
            st.map.phase = params.phase;
            st.map.lane = params.lane;
            if params.enabled {
                st.map.interactive = true;
                st.map.corner_edit = false;
                st.map.sf_edit = false;
            }
            Response::ok(id, st.map_state_json())
        }
        methods::MAP_UNDO_POINT => {
            let mut st = state.write();
            st.map.pit_points.pop();
            Response::ok(id, json!({"points": st.map.pit_points.len()}))
        }
        methods::MAP_CLEAR_PIT => {
            let mut st = state.write();
            st.map.pit_points.clear();
            Response::ok(id, json!({"points": 0}))
        }
        methods::MAP_RESET_VIEW => Response::ok(id, json!({"reset": true})),
        methods::MAP_SAVE_PIT => {
            // Persistence of track JSON remains Python-authored for now;
            // acknowledge and return point count.
            let st = state.read();
            Response::ok(
                id,
                json!({"ok": true, "points": st.map.pit_points.len(), "msg": "pit draft held in overlay"}),
            )
        }
        methods::MAP_SAVE_LOOP => Response::ok(id, json!({"ok": true})),
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
