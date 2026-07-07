"""Layout tests for pit advisor word-wrap and auto-height."""

import pytest

pytest.importorskip("PyQt6.QtWidgets")

from PyQt6.QtWidgets import QApplication

from overlay.widgets.pit_advisor import (
    PitAdvisorWidget,
    _PREVIEW,
    measure_pit_advisor_layout,
)

_LONG_RATIONALE = (
    "Pit next lap to pass #12 — 6.2s ahead, stop costs ~28s "
    "and you will rejoin ahead of several slower cars on fresh tires"
)
_LONG_SECONDARY = (
    "3 cautions so far · 12 laps since yellow · "
    "62% of cars ahead pitting on this stop cycle"
)


@pytest.fixture(scope="module")
def qapp():
    app = QApplication.instance()
    if app is None:
        app = QApplication([])
    return app


def _sample_data(**overrides):
    base = {
        "rec": "pit_next_lap",
        "label": "PIT NEXT LAP",
        "rationale": _LONG_RATIONALE,
        "secondary": _LONG_SECONDARY,
        "actionable": True,
    }
    base.update(overrides)
    return base


def test_narrow_width_taller_than_wide(qapp):
    narrow = measure_pit_advisor_layout(180, _sample_data())
    wide = measure_pit_advisor_layout(360, _sample_data())
    assert narrow.visible
    assert wide.visible
    assert narrow.total_content_h > wide.total_content_h


def test_long_secondary_increases_height(qapp):
  short = measure_pit_advisor_layout(220, _sample_data(secondary="Best stop: laps 24–26"))
  long_ = measure_pit_advisor_layout(220, _sample_data(secondary=_LONG_SECONDARY))
  assert long_.total_content_h >= short.total_content_h


def test_preview_layout_matches_edit_mode(qapp):
    edit = measure_pit_advisor_layout(220, {"edit": True})
    assert edit.visible
    assert edit.label == _PREVIEW["label"]
    assert edit.rationale == _PREVIEW["rationale"]
    assert edit.total_content_h >= 72


def test_hidden_when_not_actionable(qapp):
    layout = measure_pit_advisor_layout(
        220,
        {"label": "STAY OUT", "rec": "stay_out", "actionable": False},
        {"show_only_when_actionable": True},
    )
    assert not layout.visible
    assert layout.chip is None


def test_widget_syncs_panel_height(qapp):
    from overlay.panel import PanelWindow

    widget = PitAdvisorWidget()
    panel = PanelWindow(
        "pit_advisor_test",
        widget,
        (0, 0, 220, 100),
        {},
        click_through=True,
    )
    panel.show()
    widget.resize(180, 72)
    widget.set_data(_sample_data())
    assert panel.height() >= int(measure_pit_advisor_layout(180, widget.data).total_content_h)


def test_narrow_paint_smoke(qapp):
    w = PitAdvisorWidget()
    w.resize(180, 200)
    w.set_data({
        "edit": True,
        "rationale": _LONG_RATIONALE,
        "secondary": _LONG_SECONDARY,
    })
    w.repaint()
