"""
Relative table: cars nearest the player on track, styled after modern overlays.

Rows are painted by BaseTable; the header/footer slots (SOF, position, race
time, temps, etc.) are the shared, fully mappable system in BaseTable, driven by
config.CFG["relative"]["header"/"footer"]. Threat rows (different lap) and the
player row are highlighted by the base class from per-row flags.
"""

from __future__ import annotations

from .table import BaseTable


class RelativeWidget(BaseTable):
    section = "relative"
