"""
Local-machine CPU% / memory% for the optional table header/footer readouts.

Prefers psutil when it's installed, but falls back to OS-native calls so the
``cpu`` / ``mem`` slots work out of the box with no extra dependency:
  * Windows: GetSystemTimes / GlobalMemoryStatusEx via ctypes,
  * Linux:   /proc/stat and /proc/meminfo,
  * macOS:   psutil only (the native path returns None -> "--").

Each function returns a float in 0..100, or None when it can't be determined.
"""

from __future__ import annotations

import sys

try:  # psutil is optional; the ctypes/proc fallbacks cover the common cases.
    import psutil
except Exception:  # pragma: no cover - optional dependency
    psutil = None

# Previous (busy, total) CPU tick counters for the no-psutil delta method.
_prev_cpu: tuple[int, int] | None = None


def cpu_percent() -> float | None:
    """System-wide CPU utilisation since the previous call (0..100)."""
    global _prev_cpu
    if psutil is not None:
        try:
            return float(psutil.cpu_percent(interval=None))
        except Exception:
            pass
    sample = _cpu_times()
    if sample is None:
        return None
    if _prev_cpu is None:
        _prev_cpu = sample
        return 0.0
    busy = sample[0] - _prev_cpu[0]
    total = sample[1] - _prev_cpu[1]
    _prev_cpu = sample
    if total <= 0:
        return None
    return max(0.0, min(100.0, busy / total * 100.0))


def mem_percent() -> float | None:
    """Physical memory in use (0..100)."""
    if psutil is not None:
        try:
            return float(psutil.virtual_memory().percent)
        except Exception:
            pass
    if sys.platform.startswith("win"):
        return _win_mem()
    return _proc_mem()


# --- platform-specific helpers ---------------------------------------------

def _cpu_times() -> tuple[int, int] | None:
    if sys.platform.startswith("win"):
        return _win_cpu_times()
    if sys.platform.startswith("linux"):
        return _linux_cpu_times()
    return None  # macOS without psutil: no cheap stdlib path


def _win_cpu_times() -> tuple[int, int] | None:
    try:
        import ctypes
        from ctypes import wintypes

        idle, kernel, user = (wintypes.FILETIME() for _ in range(3))
        ok = ctypes.windll.kernel32.GetSystemTimes(
            ctypes.byref(idle), ctypes.byref(kernel), ctypes.byref(user))
        if not ok:
            return None

        def _ticks(ft) -> int:
            return (ft.dwHighDateTime << 32) | ft.dwLowDateTime

        idle_t = _ticks(idle)
        total = _ticks(kernel) + _ticks(user)  # kernel time already includes idle
        return (total - idle_t, total)
    except Exception:
        return None


def _linux_cpu_times() -> tuple[int, int] | None:
    try:
        with open("/proc/stat", encoding="ascii") as fh:
            parts = fh.readline().split()
        if not parts or parts[0] != "cpu":
            return None
        vals = [int(x) for x in parts[1:]]
        idle = vals[3] + (vals[4] if len(vals) > 4 else 0)  # idle + iowait
        return (sum(vals) - idle, sum(vals))
    except Exception:
        return None


def _win_mem() -> float | None:
    try:
        import ctypes
        from ctypes import wintypes

        class MEMORYSTATUSEX(ctypes.Structure):
            _fields_ = [
                ("dwLength", wintypes.DWORD),
                ("dwMemoryLoad", wintypes.DWORD),
                ("ullTotalPhys", ctypes.c_ulonglong),
                ("ullAvailPhys", ctypes.c_ulonglong),
                ("ullTotalPageFile", ctypes.c_ulonglong),
                ("ullAvailPageFile", ctypes.c_ulonglong),
                ("ullTotalVirtual", ctypes.c_ulonglong),
                ("ullAvailVirtual", ctypes.c_ulonglong),
                ("ullAvailExtendedVirtual", ctypes.c_ulonglong),
            ]

        stat = MEMORYSTATUSEX()
        stat.dwLength = ctypes.sizeof(MEMORYSTATUSEX)
        if not ctypes.windll.kernel32.GlobalMemoryStatusEx(ctypes.byref(stat)):
            return None
        return float(stat.dwMemoryLoad)
    except Exception:
        return None


def _proc_mem() -> float | None:
    try:
        info: dict[str, float] = {}
        with open("/proc/meminfo", encoding="ascii") as fh:
            for line in fh:
                key, _, rest = line.partition(":")
                if rest:
                    info[key] = float(rest.strip().split()[0])
        total = info.get("MemTotal")
        avail = info.get("MemAvailable")
        if total and avail is not None:
            return max(0.0, min(100.0, (1.0 - avail / total) * 100.0))
    except Exception:
        pass
    return None
