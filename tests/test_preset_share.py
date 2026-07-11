"""Preset export / import for sharing profiles."""

from __future__ import annotations

import pytest

from overlay import config


@pytest.fixture
def scratch_preset():
    name = "__share_test_src__"
    if name in config.presets():
        config.delete_preset(name)
    assert config.create_preset(name, copy_from=config.active_preset(),
                                activate=False)
    yield name
    for n in list(config.presets()):
        if n.startswith("__share_test"):
            if len(config.presets()) > 1:
                config.delete_preset(n)


def test_export_import_roundtrip(scratch_preset):
    payload = config.export_preset(scratch_preset)
    assert payload["kind"] == "gridglance.preset"
    assert payload["name"] == scratch_preset
    assert "default" not in payload["preset"]

    dest = "__share_test_imported__"
    if dest in config.presets():
        config.delete_preset(dest)
    got = config.import_preset(payload, name=dest, activate=False)
    assert got == dest
    assert dest in config.presets()
    assert config._PRESETS[dest].get("default") is False


def test_import_rejects_collision(scratch_preset):
    payload = config.export_preset(scratch_preset)
    with pytest.raises(ValueError, match="already exists"):
        config.import_preset(payload, name=scratch_preset, overwrite=False,
                             activate=False)
