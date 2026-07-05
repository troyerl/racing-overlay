"""PitSvFlags decode tests."""

from overlay import pit_service as ps


def test_decode_flags_fuel_and_tires():
    flags = ps.FUEL_FILL | ps.LF_TIRE | ps.RR_TIRE
    svcs = ps.decode_flags(flags)
    by_key = {s["key"]: s["checked"] for s in svcs}
    assert by_key["fuel"] is True
    assert by_key["lf_tire"] is True
    assert by_key["rr_tire"] is True
    assert by_key["rf_tire"] is False


def test_any_requested():
    assert ps.any_requested(0) is False
    assert ps.any_requested(ps.FAST_REPAIR) is True
