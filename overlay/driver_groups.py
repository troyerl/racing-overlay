"""Personal driver groups (league mates) with icon badges for tables."""

from __future__ import annotations

from .track_store import normalize_pro_drivers

# Curated Font Awesome keys available in the settings icon picker.
DRIVER_GROUP_ICONS: tuple[str, ...] = (
    "league", "flag", "trophy", "shield", "crown", "bolt",
)

DRIVER_GROUP_ICON_LABELS: dict[str, str] = {
    "league": "League (users)",
    "flag": "Flag",
    "trophy": "Trophy",
    "shield": "Shield",
    "crown": "Crown",
    "bolt": "Bolt",
}

_DEFAULT_GROUP_COLOR = "#5bb8ff"
_DEFAULT_GROUP_ICON = "league"


def normalize_driver_groups(raw) -> list[dict]:
    """Coerce groups to ``{name, icon, color, members}`` dicts."""
    out: list[dict] = []
    if not isinstance(raw, list):
        return out
    seen_names: set[str] = set()
    for item in raw:
        if not isinstance(item, dict):
            continue
        name = str(item.get("name") or "").strip()
        if not name:
            continue
        key = name.casefold()
        if key in seen_names:
            continue
        seen_names.add(key)
        icon = str(item.get("icon") or _DEFAULT_GROUP_ICON).strip() or _DEFAULT_GROUP_ICON
        if icon not in DRIVER_GROUP_ICONS:
            icon = _DEFAULT_GROUP_ICON
        color = str(item.get("color") or _DEFAULT_GROUP_COLOR).strip()
        if not color.startswith("#"):
            color = _DEFAULT_GROUP_COLOR
        members = normalize_pro_drivers(item.get("members"))
        out.append({
            "name": name,
            "icon": icon,
            "color": color,
            "members": members,
        })
    return out


def driver_group_for_name(user_name: str | None, groups) -> dict | None:
    """First matching group for ``user_name`` (name or alias, casefold)."""
    if not user_name:
        return None
    needle = str(user_name).strip().casefold()
    if not needle:
        return None
    for group in normalize_driver_groups(groups):
        for entry in group.get("members") or []:
            if entry["name"].casefold() == needle:
                return group
            for alias in entry.get("aliases") or []:
                if str(alias).casefold() == needle:
                    return group
    return None
