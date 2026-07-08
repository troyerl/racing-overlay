"""Map car-dot draw order: speaking drivers render on top."""

from overlay.widgets.track_map import TrackMapWidget


def _car(idx, speaking=False, is_player=False):
    return (idx, 0.5, "12", "#ff0000", is_player, False, False,
            speaking, False, None, False, False)


def test_speaking_cars_sort_last():
    cars = [_car(1), _car(2, speaking=True), _car(0, is_player=True)]
    order = [c[0] for c in sorted(cars, key=TrackMapWidget._car_draw_sort_key)]
    assert order == [1, 0, 2]


def test_speaking_player_sorts_last():
    cars = [_car(1, speaking=True), _car(0, is_player=True, speaking=True)]
    order = [c[0] for c in sorted(cars, key=TrackMapWidget._car_draw_sort_key)]
    assert order == [1, 0]
