"""JSON-RPC client for the Rust GridGlance overlay process.

Talks newline-delimited JSON over TCP ``127.0.0.1:19847`` (see
``gridglance_ipc::DEFAULT_IPC_PORT``). Used by the settings editor for live
Apply and Track Scan authoring when the overlay backend is Rust.
"""

from __future__ import annotations

import json
import os
import socket
import threading
from typing import Any

DEFAULT_HOST = "127.0.0.1"
DEFAULT_PORT = 19847
PROTOCOL_VERSION = 1


class OverlayIpcError(RuntimeError):
    pass


class OverlayIpcClient:
    """Thread-safe thin client. Reconnects on demand."""

    def __init__(self, host: str = DEFAULT_HOST, port: int = DEFAULT_PORT,
                 timeout: float = 5.0):
        self.host = host
        self.port = port
        self.timeout = timeout
        self._sock: socket.socket | None = None
        self._file = None
        self._next_id = 1
        self._lock = threading.Lock()

    def close(self) -> None:
        with self._lock:
            self._close_unlocked()

    def _close_unlocked(self) -> None:
        if self._file is not None:
            try:
                self._file.close()
            except OSError:
                pass
            self._file = None
        if self._sock is not None:
            try:
                self._sock.close()
            except OSError:
                pass
            self._sock = None

    def _ensure(self) -> None:
        if self._sock is not None:
            return
        sock = socket.create_connection((self.host, self.port), timeout=self.timeout)
        sock.settimeout(self.timeout)
        self._sock = sock
        self._file = sock.makefile("rwb")

    def call(self, method: str, params: dict | None = None) -> Any:
        with self._lock:
            try:
                self._ensure()
                req_id = self._next_id
                self._next_id += 1
                payload = {
                    "id": req_id,
                    "method": method,
                    "params": params or {},
                }
                line = (json.dumps(payload) + "\n").encode("utf-8")
                assert self._file is not None
                self._file.write(line)
                self._file.flush()
                raw = self._file.readline()
                if not raw:
                    self._close_unlocked()
                    raise OverlayIpcError("overlay IPC connection closed")
                resp = json.loads(raw.decode("utf-8"))
            except (OSError, TimeoutError, json.JSONDecodeError) as exc:
                self._close_unlocked()
                raise OverlayIpcError(str(exc)) from exc

        if not resp.get("ok", False):
            raise OverlayIpcError(resp.get("error") or "IPC error")
        return resp.get("result")

    def ping(self) -> dict:
        return self.call("ping")

    def available(self) -> bool:
        try:
            self.ping()
            return True
        except OverlayIpcError:
            return False

    def config_reload(self) -> Any:
        return self.call("config.reload")

    def config_apply(self, cfg: dict, generation: int | None = None) -> Any:
        params: dict[str, Any] = {"cfg": cfg}
        if generation is not None:
            params["generation"] = generation
        return self.call("config.apply", params)

    def overlay_start(self) -> Any:
        return self.call("overlay.start")

    def overlay_stop(self) -> Any:
        return self.call("overlay.stop")

    def set_edit_mode(self, enabled: bool) -> Any:
        return self.call("overlay.set_edit_mode", {"edit_mode": enabled})

    def layout_get(self) -> dict:
        return self.call("layout.get") or {}

    def layout_set(self, key: str, x: int, y: int, w: int, h: int) -> Any:
        return self.call("layout.set", {
            "key": key,
            "geom": {"x": x, "y": y, "w": w, "h": h},
        })

    def map_set_pit_edit(self, enabled: bool, phase: str = "road",
                         lane: str = "primary") -> Any:
        return self.call("map.set_pit_edit", {
            "enabled": enabled, "phase": phase, "lane": lane,
        })

    def map_undo_point(self) -> Any:
        return self.call("map.undo_point")

    def map_clear_pit(self, phase: str | None = None,
                      lane: str | None = None) -> Any:
        params = {}
        if phase is not None:
            params["phase"] = phase
        if lane is not None:
            params["lane"] = lane
        return self.call("map.clear_pit", params)

    def map_reset_view(self) -> Any:
        return self.call("map.reset_view")

    def map_save_pit(self) -> Any:
        return self.call("map.save_pit")

    def map_save_loop(self) -> Any:
        return self.call("map.save_loop")

    def map_invalidate_track(self) -> Any:
        return self.call("map.invalidate_track")

    def map_load_pit(self, force: bool = False) -> Any:
        return self.call("map.load_pit", {"force": bool(force)})

    def map_set_loop(self, track_id: int, points: list, *, name: str = "",
                     start_finish: float = 0.0, corners: list | None = None,
                     num_turns: int | None = None) -> Any:
        params: dict[str, Any] = {
            "track_id": int(track_id),
            "name": name or "",
            "points": [[float(p[0]), float(p[1])] for p in points],
            "start_finish": float(start_finish),
            "corners": list(corners or []),
        }
        if num_turns is not None:
            params["num_turns"] = int(num_turns)
        return self.call("map.set_loop", params)

    def map_set_corners(self, corners: list) -> Any:
        return self.call("map.set_corners", {"corners": list(corners or [])})

    def map_set_start_finish(self, pct: float) -> Any:
        return self.call("map.set_start_finish", {"pct": float(pct)})

    def map_set_interactive(self, enabled: bool) -> Any:
        return self.call("map.set_interactive", {"enabled": enabled})

    def map_get_state(self) -> dict:
        return self.call("map.get_state") or {}

    def map_set_corner_edit(self, enabled: bool) -> Any:
        return self.call("map.set_corner_edit", {"enabled": enabled})

    def map_set_sf_edit(self, enabled: bool) -> Any:
        return self.call("map.set_sf_edit", {"enabled": enabled})

    def map_set_pit_speed(self, speed_ms: float) -> Any:
        return self.call("map.set_pit_speed", {"speed_ms": speed_ms})

    def map_set_pit_lane_speed(self, pct: float) -> Any:
        return self.call("map.set_pit_lane_speed", {"pct": pct})

    def map_set_num_turns(self, n: int) -> Any:
        return self.call("map.set_num_turns", {"n": n})

    def map_set_alias_ids(self, ids: list[int]) -> Any:
        return self.call("map.set_alias_ids", {"ids": ids})

    def track_authoring_state(self) -> dict:
        return self.call("track.authoring_state") or {}


class RemoteOverlay:
    """Duck-typed stand-in for ``AdvancedSimHUD`` settings / Track Scan hooks."""

    def __init__(
        self,
        client: OverlayIpcClient | None = None,
        *,
        demo: bool = False,
        edit_mode: bool = False,
        running: bool = False,
    ):
        from . import paths

        self.ipc = client or OverlayIpcClient()
        self.map_widget = _RemoteMap(self.ipc)
        self.demo = bool(demo)
        self.tracks_dir = paths.tracks_dir()
        self._edit_mode = bool(edit_mode)
        self._running = bool(running)
        self._pro_drivers: list = []
        self._shared_demo_track_id: str | None = None
        self._settings_window = None
        self._context_ir = None
        self._last_car_path: str | None = None
        self._last_league_id: int | None = None
        self._profile_loading_depth = 0
        self._profile_loading_dialog = None
        self._v2_authoring_track_id = None
        self._track_turns = None
        # Same cloud sync object ConfigEditor expects on the Python HUD.
        from . import track_store
        self._track_sync = track_store.TrackSync()
        self._track_sync.app_settingsFetched.connect(self._on_app_settings_fetched)

    def bind_context_ir(self, ir) -> None:
        """Attach the hybrid IR/FakeIRSDK used for garage + identity polling."""
        self._context_ir = ir

    def _on_app_settings_fetched(self, settings) -> None:
        """Mirror shared demo-track / pro-driver cache from cloud settings."""
        from . import track_store
        if not settings:
            return
        tid = settings.get("demo_track_id")
        self._shared_demo_track_id = str(tid) if tid is not None else None
        self._pro_drivers = track_store.normalize_pro_drivers(
            settings.get("pro_drivers") or [])

    # --- overlay start / edit (required by ConfigEditor) -----------------

    def edit_mode_enabled(self) -> bool:
        return bool(self._edit_mode)

    def overlay_running(self) -> bool:
        return bool(self._running)

    def start_overlay(self) -> None:
        import time

        last_err: OverlayIpcError | None = None
        for attempt in range(3):
            try:
                # Fresh TCP so a half-open peer cannot eat the request.
                self.ipc.close()
                self.ipc.overlay_start()
                self._running = True
                return
            except OverlayIpcError as exc:
                last_err = exc
                if attempt < 2:
                    time.sleep(0.15 * (attempt + 1))
        self._running = False
        assert last_err is not None
        raise last_err

    def stop_overlay(self) -> None:
        try:
            self.ipc.overlay_stop()
        finally:
            self._running = False

    def toggle_overlay(self) -> bool:
        if self._running:
            self.stop_overlay()
        else:
            self.start_overlay()
        return self._running

    def set_edit_mode(self, enabled: bool) -> None:
        enabled = bool(enabled)
        self.ipc.set_edit_mode(enabled)
        self._edit_mode = enabled

    def apply_config(self, cfg: dict) -> None:
        # Apply live CFG only — do not config.reload() afterward (that reloads
        # from disk and undoes unsaved show flags / live edits).
        self.ipc.config_apply(cfg)

    def apply_active_preset(self) -> None:
        """Push the active preset's full CFG + window layout to the Rust overlay."""
        from . import config

        try:
            self.apply_config(config.CFG)
            for key, geom in (config.active_layout() or {}).items():
                if not geom or len(geom) != 4:
                    continue
                x, y, w, h = (int(geom[0]), int(geom[1]), int(geom[2]), int(geom[3]))
                self.ipc.layout_set(key, x, y, w, h)
        except OverlayIpcError:
            pass

    # --- Track Scan / authoring ------------------------------------------

    def set_pit_edit_mode(self, enabled: bool, phase: str = "road",
                          lane="primary") -> None:
        # Track Scan panel passes lane as 1/2; Rust IPC expects primary/secondary.
        if lane in (1, "1", "primary"):
            lane_s = "primary"
        elif lane in (2, "2", "secondary"):
            lane_s = "secondary"
        else:
            lane_s = str(lane) if lane else "primary"
        if enabled:
            try:
                self.load_pit_into_editor(force=False)
            except OverlayIpcError:
                pass
        self.ipc.map_set_pit_edit(enabled, phase=phase, lane=lane_s)

    def set_corner_edit_mode(self, enabled: bool) -> None:
        was = False
        try:
            was = bool(self.pit_edit_state().get("corner_edit"))
        except OverlayIpcError:
            pass
        self.ipc.map_set_corner_edit(enabled)
        if was and not enabled:
            self._persist_corners_from_rust()

    def set_sf_edit_mode(self, enabled: bool) -> None:
        was = False
        try:
            was = bool(self.pit_edit_state().get("sf_edit"))
        except OverlayIpcError:
            pass
        self.ipc.map_set_sf_edit(enabled)
        if was and not enabled:
            self._persist_start_finish_from_rust()

    def _persist_corners_from_rust(self) -> None:
        from .widgets import track_map as tm

        try:
            state = self.pit_edit_state()
        except OverlayIpcError:
            return
        tid = self.effective_track_id()
        if tid is None:
            return
        corners = state.get("corners") or []
        try:
            tm.update_track_meta(
                self.tracks_dir, tid, corners=corners)
            self.ipc.map_invalidate_track()
        except Exception:
            pass

    def _persist_start_finish_from_rust(self) -> None:
        from .widgets import track_map as tm

        try:
            state = self.pit_edit_state()
        except OverlayIpcError:
            return
        tid = self.effective_track_id()
        if tid is None:
            return
        sf = float(state.get("start_finish") or 0.0)
        try:
            tm.update_track_meta(
                self.tracks_dir, tid, start_finish=round(sf, 5))
            self.ipc.map_invalidate_track()
        except Exception:
            pass

    def set_pit_speed_authoring(self, speed_ms: float) -> bool:
        self.ipc.map_set_pit_speed(speed_ms)
        return True

    def set_pit_lane_speed_authoring(self, pct: float) -> bool:
        self.ipc.map_set_pit_lane_speed(pct)
        return True

    def set_num_turns_authoring(self, n: int) -> bool:
        self.ipc.map_set_num_turns(n)
        return True

    def set_alias_track_ids_authoring(self, ids: list[int]) -> bool:
        self.ipc.map_set_alias_ids(ids)
        return True

    def track_authoring_state(self) -> dict:
        return self.ipc.track_authoring_state()

    def pit_edit_state(self) -> dict:
        return self.ipc.map_get_state()

    def save_pit_v2(self) -> tuple[bool, str]:
        from . import paths
        from . import track_authoring as ta

        try:
            state = self.pit_edit_state()
        except OverlayIpcError as exc:
            return False, str(exc)
        tid = self.effective_track_id()
        loop = ta.xy_list(state.get("loop_points"))
        entry = ta.xy_list(state.get("entry_points"))
        road = ta.xy_list(state.get("road_points"))
        merge = ta.xy_list(state.get("merge_points"))
        entry2 = ta.xy_list(state.get("entry_points_2"))
        road2 = ta.xy_list(state.get("road_points_2"))
        merge2 = ta.xy_list(state.get("merge_points_2"))
        tracks = paths.tracks_dir()

        def _ensure(_canonical) -> bool:
            path = os.path.join(tracks, f"{_canonical}.json")
            if os.path.isfile(path):
                return True
            if len(loop) < 3 or tid is None:
                return False
            doc = ta.build_loop_doc(
                tid,
                loop=loop,
                name=str(state.get("track_name") or tid),
                start_finish=float(state.get("start_finish") or 0.0),
                num_turns=state.get("num_turns") or None,
                alias_track_ids=state.get("alias_track_ids") or None,
                corners=state.get("corners") or [],
            )
            ta.write_track_json(tracks, _canonical, doc)
            return True

        ok, msg, _meta = ta.save_pit_patch(
            tracks,
            tid=tid,
            loop=loop,
            entry=entry,
            road=road,
            merge=merge,
            entry2=entry2,
            road2=road2,
            merge2=merge2,
            pit_speed_ms=float(state.get("pit_speed_ms") or 0.0),
            pit_lane_speed_pct=float(state.get("pit_lane_speed_pct") or 1.0),
            demo=bool(state.get("demo")),
            upload_async=self._track_sync.upload_local_async,
            ensure_file=_ensure,
        )
        if ok:
            try:
                self.ipc.map_invalidate_track()
            except OverlayIpcError:
                pass
        return ok, msg

    def save_loop_v2(self) -> tuple[bool, str]:
        from . import paths
        from . import track_authoring as ta
        from . import track_store

        try:
            state = self.pit_edit_state()
        except OverlayIpcError as exc:
            return False, str(exc)
        tid = self.effective_track_id()
        if tid is None:
            return False, ("No TrackID — join a session on track, or import "
                           "members HTML with id=\"track-map-123\".")
        loop = ta.xy_list(state.get("loop_points"))
        if len(loop) < 3:
            return False, "No track loop loaded."
        tracks = paths.tracks_dir()
        canonical = track_store.resolve_track_id(tracks, tid) or tid
        block = ta.cloud_blocks_track_save(canonical)
        if block:
            return False, block
        doc = ta.build_loop_doc(
            tid,
            loop=loop,
            name=str(state.get("track_name") or tid),
            start_finish=float(state.get("start_finish") or 0.0),
            num_turns=state.get("num_turns") or None,
            alias_track_ids=state.get("alias_track_ids") or None,
            corners=state.get("corners") or [],
            pit_source="loop",
        )
        path = ta.write_track_json(tracks, canonical, doc)
        if track_store.can_write():
            self._track_sync.upload_local_async(tracks, canonical)
        try:
            self.ipc.map_invalidate_track()
        except OverlayIpcError:
            pass
        msg = f"Saved loop to {path} (no pit lane)."
        if track_store.can_write():
            msg += " Uploaded to cloud."
        if state.get("demo"):
            msg += " Demo map updated for this session."
        return True, msg

    def save_manual_track_v2(self) -> tuple[bool, str]:
        from . import paths
        from . import track_authoring as ta

        try:
            state = self.pit_edit_state()
        except OverlayIpcError as exc:
            return False, str(exc)
        tid = self.effective_track_id()
        loop = ta.xy_list(state.get("loop_points"))
        entry = ta.xy_list(state.get("entry_points"))
        road = ta.xy_list(state.get("road_points"))
        merge = ta.xy_list(state.get("merge_points"))
        entry2 = ta.xy_list(state.get("entry_points_2"))
        road2 = ta.xy_list(state.get("road_points_2"))
        merge2 = ta.xy_list(state.get("merge_points_2"))
        tracks = paths.tracks_dir()
        ok, msg, _lane1 = ta.save_manual_track(
            tracks,
            tid=tid,
            loop=loop,
            entry=entry,
            road=road,
            merge=merge,
            entry2=entry2,
            road2=road2,
            merge2=merge2,
            name=str(state.get("track_name") or tid),
            start_finish=float(state.get("start_finish") or 0.0),
            num_turns=state.get("num_turns") or None,
            alias_track_ids=state.get("alias_track_ids") or None,
            corners=state.get("corners") or [],
            pit_speed_ms=float(state.get("pit_speed_ms") or 0.0),
            pit_lane_speed_pct=float(state.get("pit_lane_speed_pct") or 1.0),
            demo=bool(state.get("demo")),
            upload_async=self._track_sync.upload_local_async,
        )
        if ok:
            try:
                self.ipc.map_invalidate_track()
            except OverlayIpcError:
                pass
        return ok, msg

    def load_pit_into_editor(self, force: bool = False) -> bool:
        try:
            r = self.ipc.map_load_pit(force=force) or {}
            return bool(r.get("loaded", True))
        except OverlayIpcError:
            return False

    def clear_pit_edit_phase(self, phase: str, lane: str = "primary") -> None:
        if lane in (2, "2", "secondary"):
            lane_s = "secondary"
        else:
            lane_s = "primary"
        self.ipc.map_clear_pit(phase=phase, lane=lane_s)

    def parse_loop_v2(self, html_path: str):
        """Parse members HTML into a loop doc (same tools as Python HUD)."""
        try:
            from tools.svg_layers_to_track_v2 import (
                import_track_source_v2,
                parse_track_id_from_html,
            )
        except ImportError as exc:
            return False, f"Track import deps missing: {exc}", None, None
        tid = parse_track_id_from_html(html_path=html_path)
        if tid is None:
            tid = self.effective_track_id()
        if tid is None:
            return False, (
                "No TrackID — save members HTML with id=\"track-map-123\", "
                "or join a track in iRacing."), None, None
        try:
            doc = import_track_source_v2(
                html_path,
                num_corners=int(self._track_turns or 4) or 4,
            )
        except Exception as exc:
            return False, str(exc), None, None
        return True, "", doc, tid

    def apply_loop_v2_import(self, doc, tid, html_path) -> tuple[bool, str]:
        from . import track_authoring as ta

        pts = ta.xy_list(doc.get("points"))
        if len(pts) < 3:
            return False, "Import produced too few track points."
        self._v2_authoring_track_id = tid
        stem = os.path.splitext(os.path.basename(html_path or ""))[0]
        name = stem or str(tid)
        corners = [c for c in (doc.get("corners") or []) if isinstance(c, dict)]
        turns = doc.get("num_turns")
        try:
            self.ipc.map_set_loop(
                int(tid),
                pts,
                name=name,
                start_finish=float(doc.get("start_finish") or 0.0),
                corners=corners,
                num_turns=int(turns) if turns else None,
            )
        except OverlayIpcError as exc:
            return False, str(exc)
        n = len(pts)
        return True, (
            f"Loop imported for TrackID {tid} — {n} pts. "
            f"Save loop to upload now, or draw pit road + merge and Save track.")

    def effective_track_id(self):
        if self._v2_authoring_track_id is not None:
            return self._v2_authoring_track_id
        state = {}
        try:
            state = self.pit_edit_state()
        except OverlayIpcError:
            pass
        return state.get("authoring_track_id") or state.get("track_id")

    def maybe_auto_switch_preset(self) -> None:
        """Car/league preset switch for the hybrid poller."""
        from . import config

        if self.demo or not config.auto_switch_enabled():
            return
        car, _name = self.current_car()
        if not car:
            return
        league_id, _ = self.current_league()
        if car == self._last_car_path and league_id == self._last_league_id:
            return
        self._last_car_path = car
        self._last_league_id = league_id
        target = config.preset_for_session(league_id, car)
        if target and target != config.active_preset():
            self._show_profile_loading(f"Loading preset\u2026 {target}")
            try:
                config.set_active_preset(target)
            finally:
                self._finish_profile_loading()

    # --- profile / identity (settings pages) -----------------------------

    def current_car(self) -> tuple[str, str]:
        ir = self._context_ir
        if ir is None:
            return "", ""
        try:
            idx = ir["PlayerCarIdx"]
        except Exception:
            return "", ""
        try:
            idx = int(idx)
        except (TypeError, ValueError):
            return "", ""
        if idx < 0:
            return "", ""
        drivers = None
        try:
            info = ir["DriverInfo"]
            if isinstance(info, dict):
                drivers = info.get("Drivers")
        except Exception:
            drivers = None
        if not isinstance(drivers, list):
            return "", ""
        for drv in drivers:
            if not isinstance(drv, dict):
                continue
            try:
                if int(drv.get("CarIdx", -1)) != idx:
                    continue
            except (TypeError, ValueError):
                continue
            path = str(drv.get("CarPath") or "")
            name = str(drv.get("CarScreenName") or path)
            return path, name
        return "", ""

    def current_league(self) -> tuple[int, str]:
        ir = self._context_ir
        if ir is None:
            return 0, ""
        wk = None
        try:
            wk = ir["WeekendInfo"]
        except Exception:
            pass
        lid = 0
        if isinstance(wk, dict):
            try:
                lid = int(wk.get("LeagueID") or 0)
            except (TypeError, ValueError):
                lid = 0
        return (lid, f"League {lid}") if lid > 0 else (0, "")

    def _show_profile_loading(self, message: str = "") -> None:
        from PyQt6.QtCore import Qt
        from PyQt6.QtWidgets import QApplication, QProgressDialog

        self._profile_loading_depth = max(0, self._profile_loading_depth) + 1
        if self._profile_loading_dialog is not None:
            if message:
                self._profile_loading_dialog.setLabelText(message)
            return
        parent = self._settings_window
        dlg = QProgressDialog(message or "Loading…", None, 0, 0, parent)
        dlg.setWindowModality(Qt.WindowModality.ApplicationModal)
        dlg.setMinimumDuration(0)
        dlg.setCancelButton(None)
        dlg.show()
        self._profile_loading_dialog = dlg
        QApplication.processEvents()

    def _finish_profile_loading(self) -> None:
        self._profile_loading_depth = max(0, self._profile_loading_depth - 1)
        if self._profile_loading_depth > 0:
            return
        dlg = self._profile_loading_dialog
        self._profile_loading_dialog = None
        if dlg is not None:
            dlg.close()
            dlg.deleteLater()

    def _load_demo_track(self) -> None:
        try:
            self.ipc.map_invalidate_track()
        except OverlayIpcError:
            pass


class _RemoteMap:
    def __init__(self, ipc: OverlayIpcClient):
        self._ipc = ipc

    def pop_last_pit_edit_point(self) -> None:
        self._ipc.map_undo_point()

    def clear_pit_edit(self) -> None:
        self._ipc.map_clear_pit(phase="all")

    def clear_pit(self) -> None:
        self._ipc.map_clear_pit(phase="all")

    def clear_pit_edit_phase(self, phase: str, lane: str = "primary") -> None:
        if lane in (2, "2", "secondary"):
            lane_s = "secondary"
        else:
            lane_s = "primary"
        self._ipc.map_clear_pit(phase=phase, lane=lane_s)

    def reset_pit_edit_view(self) -> None:
        self._ipc.map_reset_view()

    def set_pit_edit(self, *args, **kwargs) -> None:
        enabled = bool(args[0]) if args else bool(kwargs.get("enabled", False))
        phase = kwargs.get("phase", "road")
        lane = kwargs.get("lane", "primary")
        self._ipc.map_set_pit_edit(enabled, phase=phase, lane=lane)
