"""Unit tests for pit strategy recommendations (no Qt)."""

from __future__ import annotations

from overlay.pit_strategy import (
    CautionTracker,
    PitRec,
    TIRE_STATUS_BANKRUPT,
    advise_pit_strategy,
    build_caution_outlook,
    build_field_intel,
    build_opponent_profiles,
    build_pit_context,
    build_session_phase,
    build_strategy_extras,
    build_tire_snapshot,
    compute_green_position_tradeoff,
    compute_reentry_verdict,
    record_pit_loss_sample,
    resolve_pit_loss,
    update_caution_tracker,
    _format_intel_secondary,
)
from overlay import telemetry as tele
from overlay import common as oc


def _snapshot(
    *,
    window_open=False,
    window=(24, 26),
    add=18.0,
    laps_margin=5.0,
    lap=23,
    laps_empty=12.0,
):
    return {
        "window_open": window_open,
        "window": window,
        "add": add,
        "laps_margin": laps_margin,
        "lap": lap,
        "laps_remaining": 10,
        "usage_avg": 2.5,
        "laps_empty": laps_empty,
    }


def _ctx(
    *,
    caution=False,
    pits_closed=False,
    position=8,
    gap_ahead=6.0,
    gap_behind=15.0,
    car_ahead="12",
    car_behind="8",
    flag_context=None,
):
    return {
        "position": position,
        "gap_ahead": gap_ahead,
        "gap_behind": gap_behind,
        "car_ahead": car_ahead,
        "car_behind": car_behind,
        "caution": caution,
        "pits_closed": pits_closed,
        "flag_context": flag_context,
    }


_CFG = {
    "pit_loss_seconds": 28.0,
    "low_fuel_laps_threshold": 2.0,
    "undercut_gap_max_s": 12.0,
    "cover_gap_max_s": 8.0,
    "top_positions_stay_out": 5,
    "field_pit_follow_threshold": 0.45,
    "caution_pit_pra_threshold": 0.60,
    "caution_pit_lead_loss_max": 3,
    "show_field_context": False,
    "pit_menu_hard_gate": True,
}

_FULL_PIT_MENU = {"fuel_queued": True, "tires_queued": True}


def _win_strategy(**kwargs):
    base = {"inside_window": True, "post_pit_quiet": False, "on_pit_road": False}
    base.update(kwargs)
    return base


def test_caution_pits_closed_hold():
    advice = advise_pit_strategy(
        _snapshot(),
        _ctx(caution=True, pits_closed=True, flag_context="Caution waving — pits closed"),
        _CFG,
    )
    assert advice.rec == PitRec.HOLD
    assert advice.actionable is True


def test_green_window_open_small_gap_ahead_undercut():
    advice = advise_pit_strategy(
        _snapshot(window_open=True),
        _ctx(gap_ahead=6.0, gap_behind=20.0),
        _CFG,
        field_intel={"reentry": {"v": "CLEAN"}},
        strategy=_win_strategy(),
        pit_menu=_FULL_PIT_MENU,
    )
    assert advice.rec == PitRec.PIT_NEXT_LAP
    assert "#12" in advice.rationale
    assert "6.0s" in advice.rationale


def test_green_comfortable_fuel_stay_out():
    advice = advise_pit_strategy(
        _snapshot(window_open=False, laps_margin=8.0),
        _ctx(),
        _CFG,
    )
    assert advice.rec == PitRec.STAY_OUT
    assert advice.actionable is False
    assert "8" in advice.rationale


def test_fuel_critical_overrides_undercut():
    advice = advise_pit_strategy(
        _snapshot(window_open=True, laps_margin=1.0),
        _ctx(gap_ahead=4.0),
        _CFG,
        strategy=_win_strategy(),
        pit_menu={"fuel_queued": True, "tires_queued": False},
    )
    assert advice.rec == PitRec.PIT_NOW
    assert "not enough fuel" in advice.rationale.lower()


def test_caution_top_position_stay_out():
    advice = advise_pit_strategy(
        _snapshot(window_open=True),
        _ctx(caution=True, position=3),
        _CFG,
        field_intel={"pra": 0.1, "ahead_total": 10},
    )
    assert advice.rec == PitRec.STAY_OUT
    assert "P3" in advice.rationale
    assert "leaders" in advice.rationale.lower()


def test_green_cover_pit_behind():
    advice = advise_pit_strategy(
        _snapshot(window_open=True),
        _ctx(gap_ahead=20.0, gap_behind=4.0),
        _CFG,
        field_intel={"reentry": {"v": "CLEAN"}},
        strategy=_win_strategy(),
        pit_menu=_FULL_PIT_MENU,
    )
    assert advice.rec == PitRec.PIT_NOW
    assert "Cover" in advice.rationale or "behind" in advice.rationale.lower()


def test_build_pit_context_pits_closed_from_flags_fallback():
    """When PitsOpen is unavailable, CAUTION_WAVING implies pits closed."""
    ctx = build_pit_context(
        player=0,
        positions=[1, 2],
        est_time=[0.0, 5.0],
        lap_est=90.0,
        drivers={1: {"CarNumber": "7"}},
        pace_idxs=set(),
        flag="yellow",
        flag_context="Caution",
        session_flags=0x00008000,
        pits_open=None,
    )
    assert ctx["caution"] is True
    assert ctx["pits_closed"] is True


def test_build_pit_context_pits_open_overrides_waving():
    ctx = build_pit_context(
        player=0,
        positions=[1, 2],
        est_time=[0.0, 5.0],
        lap_est=90.0,
        drivers={1: {"CarNumber": "7"}},
        pace_idxs=set(),
        flag="yellow",
        flag_context="Caution waving — pits closed",
        session_flags=0x00008000,
        pits_open=True,
    )
    assert ctx["pits_closed"] is False


def test_build_pit_context_pits_closed_when_not_open():
    ctx = build_pit_context(
        player=0,
        positions=[1, 2],
        est_time=[0.0, 5.0],
        lap_est=90.0,
        drivers={1: {"CarNumber": "7"}},
        pace_idxs=set(),
        flag="yellow",
        flag_context="Full course caution — hold position",
        session_flags=0x00004000,
        pits_open=False,
    )
    assert ctx["pits_closed"] is True


def test_caution_outlook_suppressed_short_green_run():
    cfg = {"show_field_context": True, "green_run_caution_bias_laps": 15}
    hist = {"green_run_laps": 3, "was_yellow": False}
    outlook = {"summary": "Messy field \u2014 yellow could come soon"}
    out = _format_intel_secondary(
        hist, {}, cfg,
        caution_outlook=outlook,
        on_green=True,
        green_run_ok=False,
    )
    assert out is None


def test_stay_out_caution_nudge_requires_long_green():
    snap = _snapshot(window_open=True, laps_margin=8.0, add=18.0)
    ctx = _ctx()
    strat = _win_strategy(
        fuel_window=True, must_stop=False, fuel_critical=False, tire_window=False)
    outlook = {"likelihood": "HIGH"}
    cfg = {**_CFG, "show_field_context": False, "green_run_caution_bias_laps": 15}
    common = dict(
        snapshot=snap,
        ctx=ctx,
        cfg=cfg,
        field_intel={"reentry": {"v": "CLEAN"}},
        strategy=strat,
        caution_outlook=outlook,
        pit_menu=_FULL_PIT_MENU,
    )
    short = advise_pit_strategy(
        **common,
        caution_hist={"green_run_laps": 3},
    )
    assert "caution may save a stop" not in short.rationale.lower()

    long_run = advise_pit_strategy(
        **common,
        caution_hist={"green_run_laps": 20},
    )
    assert "caution may save a stop" in long_run.rationale.lower()


def test_caution_tracker_rising_edge():
    trk = CautionTracker()
    update_caution_tracker(trk, yellow=True, lap=5, session_time=100.0)
    assert trk.caution_count == 1
    assert trk.caution_laps == [5]
    update_caution_tracker(trk, yellow=True, lap=6, session_time=110.0)
    assert trk.caution_count == 1


def test_caution_tracker_fuel_reset_on_green():
    trk = CautionTracker()
    update_caution_tracker(trk, yellow=True, lap=5, session_time=100.0)
    update_caution_tracker(trk, yellow=False, lap=6, session_time=200.0)
    assert trk.as_dict()["fuel_ema_reset"] is True


def test_field_intel_pra_ratio():
    positions = [1, 2, 3, 4, 5]
    car_lap = [10, 10, 10, 10, 10]
    on_pit = [False, True, True, True, False]
    surface = [oc.TRK_ON_TRACK] * 5
    intel = build_field_intel(
        player=4,
        positions=positions,
        car_lap=car_lap,
        on_pit_road=on_pit,
        surface=surface,
        pit_state={},
        lap_pcts=[0.1, 0.2, 0.3, 0.4, 0.5],
        f2_time=[0, 5, 10, 15, 20],
        est_time=[0, 5, 10, 15, 20],
        lap_est=90.0,
        pace_idxs=set(),
        current_lap=10,
        pit_loss=28.0,
        cfg=_CFG,
    )
    assert intel["ahead_total"] == 4
    assert intel["ahead_on_pit"] == 3
    assert abs(intel["pra"] - 0.75) < 0.01


def test_caution_high_pra_pit_now():
    advice = advise_pit_strategy(
        _snapshot(window_open=True),
        _ctx(caution=True, position=8),
        _CFG,
        field_intel={"pra": 0.65, "ahead_on_pit": 6, "ahead_total": 10,
                     "caution_impact": {"lost_lead": 5}},
        strategy=_win_strategy(),
        pit_menu=_FULL_PIT_MENU,
    )
    assert advice.rec == PitRec.PIT_NOW
    assert "already pitting" in advice.rationale.lower()


def test_green_pack_reentry_stay_out():
    advice = advise_pit_strategy(
        _snapshot(window_open=True),
        _ctx(),
        _CFG,
        field_intel={"reentry": {"v": "PACK"}},
        strategy=_win_strategy(),
    )
    assert advice.rec == PitRec.STAY_OUT
    assert "traffic" in advice.rationale.lower()


def test_reentry_lapped_danger():
    rej = compute_reentry_verdict(
        player_lap_pct=0.5,
        lap_pcts=[0.52],
        pit_loss=100.0,
        lap_est=90.0,
        gap_to_leader=5.0,
        on_pit_road=[False],
        surface=[oc.TRK_ON_TRACK],
        pace_idxs=set(),
        player=1,
        reentry_window=0.035,
    )
    assert rej["v"] == "LAPPED_DANGER"


def test_post_pit_quiet_stay_out():
    advice = advise_pit_strategy(
        _snapshot(window_open=True),
        _ctx(),
        _CFG,
        field_intel={"reentry": {"v": "CLEAN"}},
        strategy=_win_strategy(post_pit_quiet=True),
    )
    assert advice.rec == PitRec.STAY_OUT
    assert advice.actionable is False
    assert "just pitted" in advice.rationale.lower()


def test_post_pit_lockout_blocks_caution_pit():
    advice = advise_pit_strategy(
        _snapshot(window_open=True),
        _ctx(caution=True, position=8),
        _CFG,
        field_intel={"pra": 0.8, "ahead_on_pit": 6, "ahead_total": 10,
                     "caution_impact": {"lost_lead": 5}},
        strategy=_win_strategy(post_pit_lockout=True),
    )
    assert advice.rec == PitRec.STAY_OUT
    assert advice.actionable is False


def test_tire_critical_pit_now():
    advice = advise_pit_strategy(
        _snapshot(window_open=False, laps_margin=10.0),
        _ctx(),
        _CFG,
        tire_snapshot={"tire_critical": True, "wear_min_pct": 20.0},
        strategy={"tire_critical": True, "must_stop": True},
        pit_menu={"fuel_queued": False, "tires_queued": True},
    )
    assert advice.rec == PitRec.PIT_NOW
    assert "tire" in advice.rationale.lower()


def test_tire_window_fuel_ok():
    advice = advise_pit_strategy(
        _snapshot(window_open=False, laps_margin=10.0, add=0),
        _ctx(gap_ahead=20.0, gap_behind=20.0),
        _CFG,
        field_intel={"reentry": {"v": "CLEAN"}},
        tire_snapshot={"window_open": True, "wear_min_pct": 30.0},
        strategy=_win_strategy(tire_window=True, inside_window=True, fuel_window=False),
        pit_menu={"fuel_queued": False, "tires_queued": True},
    )
    assert advice.rec == PitRec.PIT_NOW
    assert "tire" in advice.rationale.lower()


def test_build_tire_snapshot_critical_wear():
    telem = {"tire_corners": {"lf": {"wear": 0.20}}}
    snap = build_tire_snapshot(
        telem,
        ll_laps=[],
        cfg={},
        pit_state={},
        player=0,
        car_lap=[10],
        laps_remaining=20.0,
    )
    assert snap["tire_critical"] is True
    assert snap["wear_min_pct"] == 20.0


def test_build_tire_snapshot_inventory_exhausted():
    telem = {"tire_sets_available": 0, "tire_sets_used": 4,
             "tire_corners": {"lf": {"wear": 0.50}}}
    snap = build_tire_snapshot(
        telem,
        ll_laps=[{"secs": 90}, {"secs": 92}, {"secs": 94}],
        cfg={},
        pit_state={0: {"lap": 5, "pit_count": 4}},
        player=0,
        car_lap=[10],
        laps_remaining=20.0,
    )
    assert snap["inventory_blocks_window"] is True
    assert snap["window_open"] is False


def test_field_intel_leader_pitted_recent():
    positions = [1, 2, 3]
    car_lap = [10, 10, 10]
    intel = build_field_intel(
        player=2,
        positions=positions,
        car_lap=car_lap,
        on_pit_road=[False, False, False],
        surface=[oc.TRK_ON_TRACK] * 3,
        pit_state={0: {"lap": 9}},
        lap_pcts=[0.1, 0.5, 0.9],
        f2_time=[0, 5, 10],
        est_time=[0, 5, 10],
        lap_est=90.0,
        pace_idxs=set(),
        current_lap=10,
        pit_loss=28.0,
        cfg=_CFG,
    )
    assert intel["leader_pitted_recent"] is True


def test_field_intel_chaos_counts():
    positions = [1, 2, 3]
    car_lap = [10, 10, 10]
    surface = [oc.TRK_OFF_TRACK, oc.TRK_ON_TRACK, oc.TRK_ON_TRACK]
    intel = build_field_intel(
        player=2,
        positions=positions,
        car_lap=car_lap,
        on_pit_road=[False, False, False],
        surface=surface,
        pit_state={},
        lap_pcts=[0.1, 0.5, 0.9],
        f2_time=[0, 5, 10],
        est_time=[0, 5, 10],
        lap_est=90.0,
        pace_idxs=set(),
        current_lap=10,
        pit_loss=28.0,
        cfg=_CFG,
    )
    assert intel["off_track_count"] >= 1


def test_caution_outlook_calming():
    hist = {"caution_count": 3, "caution_laps": [2, 5, 8],
            "green_run_laps": 12, "avg_laps_between_cautions": 6.0}
    field = {"field_chaos_score": 0.05}
    out = build_caution_outlook(hist, field, current_lap=20, cfg=_CFG)
    assert out["trend"] in ("CALMING", "STEADY", "HEATING_UP")
    assert out["likelihood"] in ("LOW", "MEDIUM", "HIGH")


def test_caution_tire_critical():
    advice = advise_pit_strategy(
        _snapshot(window_open=False, laps_margin=10.0, add=0),
        _ctx(caution=True),
        _CFG,
        tire_snapshot={"tire_critical": True},
        strategy={"tire_critical": True, "must_stop": True},
        pit_menu={"fuel_queued": False, "tires_queued": True},
    )
    assert advice.rec == PitRec.PIT_NOW
    assert "tire" in advice.rationale.lower()


def test_caution_prb_stay_out():
    advice = advise_pit_strategy(
        _snapshot(window_open=True, laps_margin=8.0),
        _ctx(caution=True, position=8),
        _CFG,
        field_intel={"pra": 0.1, "prb": 0.8, "ahead_total": 10},
        strategy=_win_strategy(),
    )
    assert advice.rec == PitRec.STAY_OUT
    assert "behind" in advice.rationale.lower()


def test_green_pos_lost_downgrade():
    advice = advise_pit_strategy(
        _snapshot(window_open=True),
        _ctx(gap_ahead=20.0, gap_behind=20.0),
        _CFG,
        field_intel={
            "reentry": {"v": "CLEAN"},
            "green_positions_lost": 3,
            "ahead_on_pit": 5,
            "pra": 0.6,
        },
        strategy=_win_strategy(),
        pit_menu=_FULL_PIT_MENU,
    )
    assert advice.rec == PitRec.MARGINAL
    assert "lose" in advice.rationale.lower()


def test_session_phase_suppresses_optional():
    advice = advise_pit_strategy(
        _snapshot(window_open=True, laps_margin=8.0),
        _ctx(),
        _CFG,
        strategy={**_win_strategy(), "critical_only": True},
        session_phase={"critical_only": True, "laps_remaining": 2.0},
    )
    assert advice.rec == PitRec.STAY_OUT
    assert "too late" in advice.rationale.lower() or "finish" in advice.rationale.lower()


def test_faster_fresh_ahead_stay_out():
    advice = advise_pit_strategy(
        _snapshot(window_open=True),
        _ctx(gap_ahead=20.0),
        _CFG,
        field_intel={
            "reentry": {"v": "CLEAN"},
            "ahead_faster_count": 2,
            "ahead_fresh_tires_count": 1,
        },
        strategy=_win_strategy(),
    )
    assert advice.rec == PitRec.STAY_OUT
    assert "fresh" in advice.rationale.lower()


def test_pit_menu_hard_gate_blocks_pit_now():
    advice = advise_pit_strategy(
        _snapshot(window_open=False, laps_margin=10.0, add=0),
        _ctx(gap_ahead=20.0, gap_behind=20.0),
        {**_CFG, "pit_menu_hard_gate": True},
        field_intel={"reentry": {"v": "CLEAN"}},
        tire_snapshot={"window_open": True, "wear_min_pct": 30.0},
        strategy=_win_strategy(tire_window=True, inside_window=True, fuel_window=False),
        pit_menu={"fuel_queued": True, "tires_queued": False},
    )
    assert advice.rec == PitRec.MARGINAL
    assert "pit menu" in advice.rationale.lower()
    assert "tires" in advice.rationale.lower()


def test_resolve_pit_loss_uses_ema():
    pit_state = {0: {"pit_loss_ema": 35.0}}
    loss = resolve_pit_loss({"pit_loss_seconds": 28.0, "use_measured_pit_loss": True},
                            pit_state, 0)
    assert loss == 35.0


def test_resolve_pit_loss_falls_back_to_config():
    loss = resolve_pit_loss({"pit_loss_seconds": 28.0}, {}, 0)
    assert loss == 28.0


def test_record_pit_loss_sample():
    pit_state: dict = {}
    record_pit_loss_sample(pit_state, 0, 40.0, cfg={})
    assert pit_state[0]["pit_loss_ema"] == 40.0
    record_pit_loss_sample(pit_state, 0, 30.0, cfg={"pit_loss_ema_alpha": 0.5})
    assert pit_state[0]["pit_loss_ema"] == 35.0


def test_measured_pit_loss_in_tire_snapshot():
    telem = {"tire_corners": {"lf": {"wear": 0.50}}}
    snap = build_tire_snapshot(
        telem,
        ll_laps=[{"secs": 90}, {"secs": 92}],
        cfg={},
        pit_state={0: {"pit_loss_ema": 40.0}},
        player=0,
        car_lap=[10],
        laps_remaining=20.0,
        pit_loss=40.0,
    )
    assert snap["pit_loss_s"] == 40.0
    assert snap["pit_payback_laps"] >= 1


def test_stints_affordable_stretch_last_set():
    telem = {"tire_sets_available": 1, "tire_sets_used": 3,
             "tire_corners": {"lf": {"wear": 0.40}}}
    snap = build_tire_snapshot(
        telem,
        ll_laps=[{"secs": 90}, {"secs": 92}],
        cfg={"tire_sets_reserve": 1},
        pit_state={0: {"lap": 5, "pit_count": 3}},
        player=0,
        car_lap=[15],
        laps_remaining=30.0,
    )
    assert snap["stints_affordable"] == 1
    assert snap["window_open"] is False


def test_build_session_phase_near_finish():
    phase = build_session_phase(
        {"session_laps_remain_ex": 2.0, "session_flags": 0},
        {"final_laps_optional_suppress": 3},
    )
    assert phase["critical_only"] is True


def _bankrupt_pit_state(start_idx: int, count: int, *, tire_stops: int | None = None):
    """Pit state for cars on last set with long stints."""
    state = {}
    for i in range(count):
        idx = start_idx + i
        stops = tire_stops if tire_stops is not None else 3
        state[idx] = {
            "lap": 5,
            "pit_count": stops,
            "tire_stop_count": stops,
            "on": False,
        }
    return state


def test_build_opponent_profiles_bankrupt():
    ahead = list(range(6, 16))
    profiles = build_opponent_profiles(
        ahead_idxs=ahead,
        pit_state=_bankrupt_pit_state(6, 10),
        car_lap=[35] * 20,
        cfg={"race_tire_sets_total": 4, "min_stint_laps": 4},
        laps_remaining=15.0,
    )
    assert len(profiles) == 10
    assert all(p["tire_status"] == TIRE_STATUS_BANKRUPT for p in profiles)


def test_compute_green_position_tradeoff_net_gain():
    profiles = [
        {"tire_status": TIRE_STATUS_BANKRUPT, "stint_laps": 28, "stops_remaining": 0}
        for _ in range(10)
    ]
    tradeoff = compute_green_position_tradeoff(
        profiles=profiles,
        positions_lost_now=2,
        cfg={"strategic_pit_min_net_positions": 3, "min_stint_laps": 4},
    )
    assert tradeoff["positions_gain_ahead"] == 10
    assert tradeoff["positions_lost_now"] == 2
    assert tradeoff["net_gain"] == 8
    assert tradeoff["strategic_pit_viable"] is True


def test_strategic_pit_tire_arbitrage():
    advice = advise_pit_strategy(
        _snapshot(window_open=True, laps_margin=8.0),
        _ctx(position=5, gap_ahead=20.0, gap_behind=20.0),
        {**_CFG, "race_tire_sets_total": 4, "strategic_pit_min_net_positions": 3},
        field_intel={
            "reentry": {"v": "CLEAN"},
            "green_positions_lost": 2,
            "ahead_on_pit": 0,
            "pra": 0.0,
            "position_tradeoff": {
                "positions_gain_ahead": 10,
                "positions_lost_now": 2,
                "net_gain": 8,
                "strategic_pit_viable": True,
            },
            "ahead_bankrupt_count": 10,
        },
        strategy=_win_strategy(tire_window=True, inside_window=True),
        tire_snapshot={
            "window_open": True,
            "sets_remaining": 2,
            "tire_inventory_exhausted": False,
        },
        pit_menu=_FULL_PIT_MENU,
    )
    assert advice.rec == PitRec.PIT_NOW
    assert "out of tires" in advice.rationale.lower()
    assert "recover" in advice.rationale.lower()


def test_green_pos_tradeoff_overrides_downgrade():
    advice = advise_pit_strategy(
        _snapshot(window_open=True),
        _ctx(gap_ahead=20.0, gap_behind=20.0, position=5),
        {**_CFG, "green_pos_tradeoff_override": True, "race_tire_sets_total": 4},
        field_intel={
            "reentry": {"v": "CLEAN"},
            "green_positions_lost": 3,
            "ahead_on_pit": 5,
            "pra": 0.6,
            "position_tradeoff": {
                "positions_gain_ahead": 10,
                "positions_lost_now": 3,
                "net_gain": 7,
                "strategic_pit_viable": True,
            },
            "ahead_bankrupt_count": 10,
        },
        strategy=_win_strategy(tire_window=True, inside_window=True),
        tire_snapshot={
            "window_open": True,
            "sets_remaining": 2,
            "tire_inventory_exhausted": False,
        },
        pit_menu=_FULL_PIT_MENU,
    )
    assert advice.rec == PitRec.PIT_NOW
    assert "out of tires" in advice.rationale.lower()


def test_caution_bankrupt_ahead_pit():
    advice = advise_pit_strategy(
        _snapshot(window_open=True, laps_margin=8.0),
        _ctx(caution=True, position=8),
        {**_CFG, "caution_bankrupt_ahead_min": 3},
        field_intel={
            "pra": 0.4,
            "ahead_total": 10,
            "ahead_bankrupt_count": 5,
        },
        strategy=_win_strategy(),
        pit_menu=_FULL_PIT_MENU,
    )
    assert advice.rec == PitRec.PIT_NOW
    assert "out of tires" in advice.rationale.lower()


def test_field_intel_position_tradeoff_integration():
    positions = [0] * 20
    positions[0] = 15
    for i, pos in enumerate(range(5, 15)):
        positions[i + 1] = pos
    car_lap = [35] * 20
    pit_state = {}
    for i in range(1, 11):
        pit_state[i] = {
            "lap": 5,
            "pit_count": 3,
            "tire_stop_count": 3,
            "on": False,
        }
    field = build_field_intel(
        player=0,
        positions=positions,
        car_lap=car_lap,
        on_pit_road=[False] * 20,
        surface=[oc.TRK_ON_TRACK] * 20,
        pit_state=pit_state,
        lap_pcts=[0.5] * 20,
        f2_time=[0.0] * 20,
        est_time=[90.0] * 20,
        lap_est=90.0,
        pace_idxs=set(),
        current_lap=35,
        pit_loss=28.0,
        cfg={"race_tire_sets_total": 4, "ahead_profile_scan_positions": 15},
        laps_remaining=15.0,
    )
    assert field["ahead_bankrupt_count"] == 10
    assert field["position_tradeoff"]["positions_gain_ahead"] == 10


def test_splash_pit_does_not_increment_tire_stop_count():
    pit_state: dict = {}
    idx = 3
    st = pit_state.setdefault(idx, {"on": False, "lap": None, "time": None})
    st["on"] = True
    st["lap"] = 10
    st["time"] = 100.0
    st["pit_count"] = 1
    duration = 12.0
    splash_max = 20.0
    st["last_pit_duration"] = duration
    is_full_stop = splash_max <= 0 or duration >= splash_max
    if is_full_stop:
        st["tire_stop_count"] = int(st.get("tire_stop_count") or 0) + 1
    assert st.get("tire_stop_count") is None
    duration = 35.0
    st["last_pit_duration"] = duration
    is_full_stop = splash_max <= 0 or duration >= splash_max
    if is_full_stop:
        st["tire_stop_count"] = int(st.get("tire_stop_count") or 0) + 1
    assert st["tire_stop_count"] == 1
