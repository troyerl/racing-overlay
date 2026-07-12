"""Tests for overlay IPC client (mocked socket)."""

from __future__ import annotations

import json
import socket
import threading
from typing import Callable

import pytest

from overlay.ipc_client import (
    DEFAULT_PORT,
    OverlayIpcClient,
    OverlayIpcError,
    RemoteOverlay,
)


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


def test_remote_overlay_map_api():
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
