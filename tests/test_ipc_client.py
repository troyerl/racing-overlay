"""Tests for overlay IPC client (mocked socket)."""

from __future__ import annotations

import json
import socket
import threading
from typing import Callable

import pytest
from PyQt6.QtCore import QCoreApplication

from overlay.ipc_client import (
    DEFAULT_PORT,
    OverlayIpcClient,
    OverlayIpcError,
    RemoteOverlay,
)


@pytest.fixture(scope="module")
def qapp():
    """QObject/TrackSync needs a Qt app; Core is enough (no GUI)."""
    app = QCoreApplication.instance()
    if app is None:
        app = QCoreApplication([])
    return app


def _serve_once(handler: Callable[[dict], dict], port: int) -> threading.Thread:
    def run() -> None:
        srv = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        srv.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
        srv.bind(("127.0.0.1", port))
        srv.listen(1)
        srv.settimeout(2.0)
        try:
            conn, _ = srv.accept()
            with conn:
                f = conn.makefile("rwb")
                line = f.readline()
                req = json.loads(line.decode())
                resp = handler(req)
                f.write((json.dumps(resp) + "\n").encode())
                f.flush()
        finally:
            srv.close()

    t = threading.Thread(target=run, daemon=True)
    t.start()
    return t


def test_ping_roundtrip():
    port = 19890

    def handler(req):
        assert req["method"] == "ping"
        return {
            "id": req["id"],
            "ok": True,
            "result": {"version": 1, "backend": "rust", "generation": 0},
        }

    _serve_once(handler, port)
    client = OverlayIpcClient(port=port, timeout=2.0)
    result = client.ping()
    assert result["backend"] == "rust"
    client.close()


def test_error_raises():
    port = 19891

    def handler(req):
        return {"id": req["id"], "ok": False, "error": "nope"}

    _serve_once(handler, port)
    client = OverlayIpcClient(port=port, timeout=2.0)
    with pytest.raises(OverlayIpcError, match="nope"):
        client.call("ping")
    client.close()


def test_remote_overlay_map_api(qapp):
    port = 19892
    seen = {}

    def handler(req):
        seen["method"] = req["method"]
        seen["params"] = req.get("params")
        return {"id": req["id"], "ok": True, "result": {"pit_edit": True}}

    _serve_once(handler, port)
    remote = RemoteOverlay(OverlayIpcClient(port=port, timeout=2.0))
    remote.set_pit_edit_mode(True, phase="road", lane="primary")
    assert seen["method"] == "map.set_pit_edit"
    assert seen["params"]["enabled"] is True


def test_remote_overlay_hud_api(qapp):
    """ConfigEditor requires these methods without hasattr guards."""
    port = 19893
    calls: list[str] = []

    def handler(req):
        calls.append(req["method"])
        return {"id": req["id"], "ok": True, "result": {"running": True}}

    _serve_once(handler, port)
    remote = RemoteOverlay(
        OverlayIpcClient(port=port, timeout=2.0),
        edit_mode=True,
        running=False,
    )
    assert remote.edit_mode_enabled() is True
    assert remote.overlay_running() is False
    assert remote.toggle_overlay() is True
    assert remote.overlay_running() is True
    assert "overlay.start" in calls


def test_remote_overlay_track_sync(qapp):
    """ConfigEditor demo-track admin connects app_settingsFetched on this."""
    remote = RemoteOverlay(OverlayIpcClient(port=19999, timeout=0.1))
    sync = remote._track_sync
    assert sync is not None
    assert hasattr(sync, "app_settingsFetched")
    assert callable(sync.fetch_app_settings_async)


def test_timeout_closes_and_reconnects():
    """A hung peer raises OverlayIpcError; the next call can reconnect."""
    hang_port = 19894
    ok_port = 19895

    def hang() -> None:
        srv = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        srv.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
        srv.bind(("127.0.0.1", hang_port))
        srv.listen(1)
        srv.settimeout(3.0)
        try:
            conn, _ = srv.accept()
            with conn:
                # Read the request but never reply — client should time out.
                conn.makefile("rb").readline()
                threading.Event().wait(2.0)
        finally:
            srv.close()

    t = threading.Thread(target=hang, daemon=True)
    t.start()

    client = OverlayIpcClient(port=hang_port, timeout=0.3)
    with pytest.raises(OverlayIpcError):
        client.ping()
    assert client._sock is None

    def handler(req):
        return {
            "id": req["id"],
            "ok": True,
            "result": {"version": 1, "backend": "rust", "generation": 1},
        }

    _serve_once(handler, ok_port)
    client.port = ok_port
    client.timeout = 2.0
    result = client.ping()
    assert result["generation"] == 1
    client.close()


def test_apply_config_does_not_reload(qapp):
    """Live apply must not config.reload() (that undoes unsaved show flags)."""
    port = 19896
    methods: list[str] = []

    def handler(req):
        methods.append(req["method"])
        return {"id": req["id"], "ok": True, "result": {"generation": 1}}

    _serve_once(handler, port)
    remote = RemoteOverlay(OverlayIpcClient(port=port, timeout=2.0))
    remote.apply_config({"map": {"show": True}, "dash": {"show": False}})
    assert methods == ["config.apply"]
    assert "config.reload" not in methods
