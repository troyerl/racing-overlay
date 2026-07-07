"""
Local-machine CPU% / memory% / GPU% for optional table header/footer readouts.

Prefers psutil when it's installed, but falls back to OS-native calls so the
``cpu`` / ``mem`` slots work out of the box with no extra dependency:
  * Windows: GetSystemTimes / GlobalMemoryStatusEx via ctypes,
  * Linux:   /proc/stat and /proc/meminfo,
  * macOS:   psutil only (the native path returns None -> "--").

GPU% (``gpu`` slot):
  * Windows: PDH ``\\GPU Engine(*)\\Utilization Percentage`` (max across engines),
  * Linux:   ``nvidia-smi`` when available,
  * macOS:   None -> "--".

WiFi signal (``system_panel`` network fallback):
  * Windows: ``wlanapi.dll`` RSSI on the connected WLAN interface,
  * Linux:   ``/proc/net/wireless`` when present,
  * macOS:   None for v1.

Each function returns a float in 0..100, or None when it can't be determined.
``wifi_signal()`` returns ``{"rssi_dbm": int, "quality_pct": int}`` or None.
"""

from __future__ import annotations

import shutil
import subprocess
import sys
import time

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


def gpu_percent() -> float | None:
    """GPU utilisation (0..100), max across adapters/engines when applicable."""
    if sys.platform.startswith("win"):
        return _win_gpu_percent()
    if sys.platform.startswith("linux"):
        return _linux_gpu_percent()
    return None


def wifi_signal() -> dict | None:
    """Connected WiFi signal: ``rssi_dbm`` and ``quality_pct`` (0..100), or None."""
    if sys.platform.startswith("win"):
        return _win_wifi_signal()
    if sys.platform.startswith("linux"):
        return _linux_wifi_signal()
    return None


def _rssi_to_quality(rssi: int) -> int:
    """Map dBm (-100..-50) to a rough 0..100 quality bar."""
    return max(0, min(100, int((int(rssi) + 100) * 2)))


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


_win_gpu_sampler = None


class _WinGpuSampler:
    """Persistent PDH query for GPU engine utilisation (Windows 10+)."""

    _COUNTER = "\\GPU Engine(*)\\Utilization Percentage"
    _PDH_FMT_DOUBLE = 0x00000200
    _ERROR_SUCCESS = 0

    def __init__(self) -> None:
        import ctypes
        from ctypes import wintypes

        self._pdh = ctypes.windll.pdh
        self._query = wintypes.HANDLE()
        self._counters: list = []
        self._value_type = type("PDH_FMT_COUNTERVALUE", (ctypes.Structure,), {
            "_fields_": [("CStatus", wintypes.DWORD),
                         ("doubleValue", ctypes.c_double)],
        })
        self._ready = self._open()

    def _open(self) -> bool:
        import ctypes
        from ctypes import wintypes

        pdh = self._pdh
        if pdh.PdhOpenQueryW(None, 0, ctypes.byref(self._query)) != self._ERROR_SUCCESS:
            return False
        size = wintypes.DWORD(0)
        pdh.PdhExpandCounterPathW(self._COUNTER, None, ctypes.byref(size), 0)
        if size.value <= 1:
            return False
        buf = ctypes.create_unicode_buffer(size.value)
        if pdh.PdhExpandCounterPathW(self._COUNTER, buf, ctypes.byref(size), 0) != 0:
            return False
        paths = [p for p in buf.value.split("\0") if p]
        for path in paths:
            counter = wintypes.HANDLE()
            if pdh.PdhAddEnglishCounterW(self._query, path, 0,
                                         ctypes.byref(counter)) == 0:
                self._counters.append(counter)
        if not self._counters:
            return False
        pdh.PdhCollectQueryData(self._query)
        time.sleep(0.1)
        pdh.PdhCollectQueryData(self._query)
        return True

    def read(self) -> float | None:
        if not self._ready:
            return None
        import ctypes
        from ctypes import wintypes

        pdh = self._pdh
        if pdh.PdhCollectQueryData(self._query) != self._ERROR_SUCCESS:
            return None
        peak = 0.0
        found = False
        for counter in self._counters:
            val = self._value_type()
            size = wintypes.DWORD(ctypes.sizeof(val))
            if pdh.PdhGetFormattedCounterValue(
                    counter, self._PDH_FMT_DOUBLE, ctypes.byref(size),
                    ctypes.byref(val)) != self._ERROR_SUCCESS:
                continue
            peak = max(peak, float(val.doubleValue))
            found = True
        if not found:
            return None
        return max(0.0, min(100.0, peak))

    def close(self) -> None:
        if self._query.value:
            self._pdh.PdhCloseQuery(self._query)


def _win_gpu_percent() -> float | None:
    global _win_gpu_sampler
    try:
        if _win_gpu_sampler is None:
            _win_gpu_sampler = _WinGpuSampler()
        return _win_gpu_sampler.read()
    except Exception:
        return None


def _linux_gpu_percent() -> float | None:
    if not shutil.which("nvidia-smi"):
        return None
    try:
        out = subprocess.check_output(
            ["nvidia-smi", "--query-gpu=utilization.gpu",
             "--format=csv,noheader,nounits"],
            timeout=1.0,
            text=True,
            stderr=subprocess.DEVNULL,
        )
        vals = [float(line.strip()) for line in out.splitlines() if line.strip()]
        if not vals:
            return None
        return max(0.0, min(100.0, max(vals)))
    except Exception:
        return None


def _win_wifi_signal() -> dict | None:
    try:
        import ctypes
        from ctypes import wintypes

        wlan = ctypes.windll.wlanapi
        WLAN_INTF_OPCODE_RSSI = 0x10009
        WLAN_OPCODE_VALUE_TYPE_UINT32 = 1
        ERROR_SUCCESS = 0

        class GUID(ctypes.Structure):
            _fields_ = [
                ("Data1", wintypes.DWORD),
                ("Data2", wintypes.WORD),
                ("Data3", wintypes.WORD),
                ("Data4", wintypes.BYTE * 8),
            ]

        class WLAN_INTERFACE_INFO(ctypes.Structure):
            _fields_ = [
                ("InterfaceGuid", GUID),
                ("strInterfaceDescription", wintypes.WCHAR * 256),
                ("isState", wintypes.DWORD),
            ]

        class WLAN_INTERFACE_INFO_LIST(ctypes.Structure):
            _fields_ = [
                ("dwNumberOfItems", wintypes.DWORD),
                ("dwIndex", wintypes.DWORD),
                ("InterfaceInfo", WLAN_INTERFACE_INFO * 1),
            ]

        handle = wintypes.HANDLE()
        negotiated = wintypes.DWORD()
        if wlan.WlanOpenHandle(2, None, ctypes.byref(negotiated),
                               ctypes.byref(handle)) != ERROR_SUCCESS:
            return None
        try:
            iface_list = ctypes.POINTER(WLAN_INTERFACE_INFO_LIST)()
            if wlan.WlanEnumInterfaces(handle, None,
                                       ctypes.byref(iface_list)) != ERROR_SUCCESS:
                return None
            try:
                count = iface_list.contents.dwNumberOfItems
                base = ctypes.addressof(iface_list.contents.InterfaceInfo)
                stride = ctypes.sizeof(WLAN_INTERFACE_INFO)
                best: dict | None = None
                for i in range(count):
                    info = WLAN_INTERFACE_INFO.from_address(base + i * stride)
                    if info.isState != 1:  # wlan_interface_state_connected
                        continue
                    data_size = wintypes.DWORD()
                    data_ptr = ctypes.c_void_p()
                    if wlan.WlanQueryInterface(
                            handle,
                            ctypes.byref(info.InterfaceGuid),
                            WLAN_INTF_OPCODE_RSSI,
                            None,
                            ctypes.byref(data_size),
                            ctypes.byref(data_ptr),
                            None) != ERROR_SUCCESS:
                        continue
                    try:
                        rssi = int(ctypes.c_int.from_address(data_ptr.value).value)
                    finally:
                        wlan.WlanFreeMemory(data_ptr)
                    if rssi == 0:
                        continue
                    quality = _rssi_to_quality(rssi)
                    if best is None or quality > best["quality_pct"]:
                        best = {"rssi_dbm": rssi, "quality_pct": quality}
                return best
            finally:
                wlan.WlanFreeMemory(iface_list)
        finally:
            wlan.WlanCloseHandle(handle, None)
    except Exception:
        return None


def _linux_wifi_signal() -> dict | None:
    try:
        with open("/proc/net/wireless", encoding="ascii") as fh:
            lines = fh.readlines()
        best: dict | None = None
        for line in lines[2:]:
            parts = line.split()
            if len(parts) < 4 or not parts[0].endswith(":"):
                continue
            try:
                qual = int(float(parts[2]))
                level = int(float(parts[3]))
            except (TypeError, ValueError):
                continue
            if qual <= 0:
                continue
            rssi = level if level < 0 else -(256 - level)
            quality_pct = max(0, min(100, int(qual / 70 * 100)))
            if best is None or quality_pct > best["quality_pct"]:
                best = {"rssi_dbm": rssi, "quality_pct": quality_pct}
        return best
    except Exception:
        return None
