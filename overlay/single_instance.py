"""Hard single-instance guard via Qt local sockets.

A second launch connects to the first instance, asks it to activate (open
Settings), then exits. Always on — correct for taskbar / desktop double-click.
"""

from __future__ import annotations

import logging
from collections.abc import Callable

from PyQt6.QtCore import QByteArray, QObject
from PyQt6.QtNetwork import QLocalServer, QLocalSocket

log = logging.getLogger(__name__)

SERVER_NAME = "GridGlance.SingleInstance"
_ACTIVATE = b"activate\n"


class SingleInstance(QObject):
    """Owns the listen socket for the primary process."""

    def __init__(self, parent: QObject | None = None):
        super().__init__(parent)
        self._server = QLocalServer(self)
        self._on_activate: Callable[[], None] | None = None
        self._server.newConnection.connect(self._on_new_connection)

    def listen(self) -> bool:
        """Become the primary instance. Clears a stale server name after crash."""
        if not self._server.listen(SERVER_NAME):
            QLocalServer.removeServer(SERVER_NAME)
            if not self._server.listen(SERVER_NAME):
                log.warning("Single-instance listen failed: %s",
                            self._server.errorString())
                return False
        return True

    def set_activate_callback(self, cb: Callable[[], None] | None) -> None:
        self._on_activate = cb

    def _on_new_connection(self) -> None:
        while self._server.hasPendingConnections():
            sock = self._server.nextPendingConnection()
            if sock is None:
                continue
            sock.readyRead.connect(
                lambda s=sock: self._read_client(s))
            # Some peers close after write with no further readyRead; also
            # handle data already buffered.
            if sock.bytesAvailable():
                self._read_client(sock)

    def _read_client(self, sock: QLocalSocket) -> None:
        data = bytes(sock.readAll())
        sock.disconnectFromServer()
        sock.deleteLater()
        if _ACTIVATE.strip() in data or b"activate" in data:
            cb = self._on_activate
            if cb is not None:
                try:
                    cb()
                except Exception:  # noqa: BLE001
                    log.exception("single-instance activate callback failed")


def try_activate_existing(timeout_ms: int = 500) -> bool:
    """If another instance is listening, send activate and return True."""
    sock = QLocalSocket()
    sock.connectToServer(SERVER_NAME)
    if not sock.waitForConnected(timeout_ms):
        sock.abort()
        return False
    sock.write(QByteArray(_ACTIVATE))
    sock.flush()
    sock.waitForBytesWritten(timeout_ms)
    sock.disconnectFromServer()
    if sock.state() != QLocalSocket.LocalSocketState.UnconnectedState:
        sock.waitForDisconnected(timeout_ms)
    return True


def acquire(parent: QObject | None = None) -> SingleInstance | None:
    """Return a primary ``SingleInstance``, or None if this process should exit.

    Call right after ``QApplication`` is created. If None, a peer was activated
    and the caller should return 0 from ``main``.
    """
    if try_activate_existing():
        return None
    guard = SingleInstance(parent)
    if not guard.listen():
        # Could not listen and could not activate — allow run to avoid lockout.
        return guard
    return guard
