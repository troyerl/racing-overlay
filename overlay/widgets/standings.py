"""
Standings (Timing Tower): the running order, styled like the Relative table.

Uses the same row layout (status badge, position + class stripe, name, license,
iRating) but the right-hand value is the gap to the leader. The header and
footer use the shared, fully mappable slot system in BaseTable (order pill,
title, count, SOF, temps, session time, ...), driven by
config.CFG["standings"]["header"/"footer"].
"""

from __future__ import annotations

from .table import BaseTable


class StandingsWidget(BaseTable):
    section = "standings"
