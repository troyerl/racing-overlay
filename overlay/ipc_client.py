"""JSON-RPC client for the Rust GridGlance overlay process.

Talks newline-delimited JSON over TCP ``127.0.0.1:19847`` (see
``gridglance_ipc::DEFAULT_IPC_PORT``). Used by the settings editor for live
Apply and Track Scan authoring when the overlay backend is Rust.
"""

from __future__ import annotations

import json
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

    def map_clear_pit(self) -> Any:
        return self.call("map.clear_pit")

    def map_reset_view(self) -> Any:
        return self.call("map.reset_view")

    def map_save_pit(self) -> Any:
        return self.call("map.save_pit")

    def map_save_loop(self) -> Any:
        return self.call("map.save_loop")

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
        # Same cloud sync object ConfigEditor expects on the Python HUD.
        from . import track_store
        self._track_sync = track_store.TrackSync()
        self._track_sync.app_settingsFetched.connect(self._on_app_settings_fetched)

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
        self.ipc.map_set_pit_edit(enabled, phase=phase, lane=lane_s)

    def set_corner_edit_mode(self, enabled: bool) -> None:
        self.ipc.map_set_corner_edit(enabled)

    def set_sf_edit_mode(self, enabled: bool) -> None:
        self.ipc.map_set_sf_edit(enabled)

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
        try:
            r = self.ipc.map_save_pit() or {}
            return bool(r.get("ok", True)), str(r.get("msg") or "saved")
        except OverlayIpcError as exc:
            return False, str(exc)

    def save_loop_v2(self) -> tuple[bool, str]:
        try:
            self.ipc.map_save_loop()
            return True, "saved"
        except OverlayIpcError as exc:
            return False, str(exc)

    def save_manual_track_v2(self) -> tuple[bool, str]:
        return False, "manual track save not available on Rust overlay yet"

    def load_pit_into_editor(self, force: bool = False) -> bool:
        return False

    def clear_pit_edit_phase(self, phase: str, lane: str = "primary") -> None:
        self.ipc.map_clear_pit()

    def parse_loop_v2(self, path: str):
        return False, "loop import not available on Rust overlay yet", None, None

    def apply_loop_v2_import(self, doc, tid, path) -> tuple[bool, str]:
        return False, "loop import not available on Rust overlay yet"

    def effective_track_id(self):
        return None

    # --- profile / identity stubs (settings pages) -----------------------

    def current_car(self):
        return None

    def current_league(self):
        return None

    def _show_profile_loading(self, message: str = "") -> None:
        return None

    def _finish_profile_loading(self) -> None:
        return None

    def _load_demo_track(self) -> None:
        return None


class _RemoteMap:
    def __init__(self, ipc: OverlayIpcClient):
        self._ipc = ipc

    def pop_last_pit_edit_point(self) -> None:
        self._ipc.map_undo_point()

    def clear_pit_edit(self) -> None:
        self._ipc.map_clear_pit()

    def clear_pit(self) -> None:
        self._ipc.map_clear_pit()

    def clear_pit_edit_phase(self, phase: str, lane: str = "primary") -> None:
        self._ipc.map_clear_pit()

    def reset_pit_edit_view(self) -> None:
        self._ipc.map_reset_view()

    def set_pit_edit(self, *args, **kwargs) -> None:
        enabled = bool(args[0]) if args else bool(kwargs.get("enabled", False))
        phase = kwargs.get("phase", "road")
        lane = kwargs.get("lane", "primary")
        self._ipc.map_set_pit_edit(enabled, phase=phase, lane=lane)
