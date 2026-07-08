"""Single-instance guard: second launch activates the first and exits early."""

from __future__ import annotations

import pytest
from PyQt6.QtCore import QCoreApplication
from PyQt6.QtWidgets import QApplication

from overlay import single_instance


@pytest.fixture
def qapp():
    app = QApplication.instance()
    if app is None:
        app = QApplication([])
    return app


def test_acquire_primary_then_secondary_activates(qapp):
    activated: list[int] = []

    primary = single_instance.acquire(qapp)
    assert primary is not None
    primary.set_activate_callback(lambda: activated.append(1))

    # A second acquire should detect the primary and return None.
    secondary = single_instance.acquire(qapp)
    assert secondary is None

    # Give Qt a moment to deliver the local-socket message.
    for _ in range(50):
        QCoreApplication.processEvents()
        if activated:
            break
    assert activated == [1]

    # Tear down so other tests are not poisoned.
    primary._server.close()
    single_instance.QLocalServer.removeServer(single_instance.SERVER_NAME)


def test_try_activate_existing_false_when_nobody_listening(qapp):
    single_instance.QLocalServer.removeServer(single_instance.SERVER_NAME)
    assert single_instance.try_activate_existing(timeout_ms=100) is False
