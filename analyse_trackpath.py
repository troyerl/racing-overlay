"""Cross-validate the map calibration: fit the table on one lap, score another.

If the drawn loop really does distribute length differently from the real track,
a table measured on one lap must also fix the next one. If instead the curve were
just dead-reckoning noise, it would not transfer.
"""
import json, sys
import numpy as np
import matplotlib

matplotlib.use("Agg")
import matplotlib.pyplot as plt

csv, track_json = sys.argv[1], sys.argv[2]
N = 720

raw = np.genfromtxt(csv, delimiter=",", names=True)
cols = ("session_time", "lap_dist_pct", "speed_mps", "vx", "vy", "yaw")
T, PCT, SPD, VX, VY, YAW = (raw[c] for c in cols)
wrap = np.where(np.diff(PCT) < -0.5)[0]


def resample_closed(p, n):
    p = np.vstack([p, p[:1]])
    s = np.concatenate([[0.0], np.cumsum(np.hypot(*np.diff(p, axis=0).T))])
    q = np.linspace(0, s[-1], n, endpoint=False)
    return np.column_stack([np.interp(q, s, p[:, 0]), np.interp(q, s, p[:, 1])]), s[-1]


doc = json.load(open(track_json))
loop = np.asarray(doc["points"], dtype=float)
if np.allclose(loop[0], loop[-1]):
    loop = loop[:-1]
loop_u, loop_len = resample_closed(loop, N)
dense, _ = resample_closed(loop, 20000)
M = len(dense)


def procrustes(A, B):
    ca, cb = A.mean(0), B.mean(0)
    A0, B0 = A - ca, B - cb
    U, S, Vt = np.linalg.svd(A0.T @ B0)
    R = Vt.T @ np.diag([1, np.sign(np.linalg.det(Vt.T @ U.T))]) @ U.T
    scale = S.sum() / (A0 ** 2).sum()
    fit = lambda P: (scale * (P - ca) @ R.T) + cb
    return float(np.sqrt(((fit(A) - B) ** 2).sum(1).mean())), fit


def lap_curve(a, b):
    """Arc fraction along the drawn loop at each of N even lap-% steps."""
    t, pct, spd, vx, vy, yaw = (v[a:b] for v in (T, PCT, SPD, VX, VY, YAW))
    wx = vx * np.cos(yaw) - vy * np.sin(yaw)
    wy = vx * np.sin(yaw) + vy * np.cos(yaw)
    dt = np.diff(t)
    px = np.concatenate([[0.0], np.cumsum(0.5 * (wx[:-1] + wx[1:]) * dt)])
    py = np.concatenate([[0.0], np.cumsum(0.5 * (wy[:-1] + wy[1:]) * dt)])
    f = (t - t[0]) / (t[-1] - t[0])
    px, py = px - f * px[-1], py - f * py[-1]
    real = np.column_stack([px, py])

    arc_shape, real_len = resample_closed(real, N)
    best = None
    for mirror in (False, True):
        A = arc_shape * ([-1, 1] if mirror else [1, 1])
        for k in range(N):
            rms, fit = procrustes(np.roll(A, k, axis=0), loop_u)
            if best is None or rms < best[0]:
                best = (rms, fit, mirror)
    rms, fit, mirror = best

    u = np.linspace(pct[0], pct[-1], N, endpoint=False)
    pts = np.column_stack([np.interp(u, pct, real[:, 0]), np.interp(u, pct, real[:, 1])])
    pts = fit(pts * ([-1, 1] if mirror else [1, 1]))

    cur = int(((pts[0] - dense) ** 2).sum(1).argmin())
    idx = []
    for p in pts:
        win = (cur + np.arange(-40, 900)) % M
        cur = int(win[((p - dense[win]) ** 2).sum(1).argmin()])
        idx.append(cur)
    idx = np.asarray(idx)
    steps = np.diff(idx)
    arc = (np.concatenate([[0.0], np.cumsum(np.where(steps < -M // 2, steps + M, steps))])) / M
    return (u - u[0]) % 1.0, arc, real_len, rms


bounds = [0] + list(wrap + 1) + [len(T)]
laps = [
    (bounds[i], bounds[i + 1])
    for i in range(len(bounds) - 1)
    if bounds[i + 1] - bounds[i] > 200
    and PCT[bounds[i + 1] - 1] - PCT[bounds[i]] > 0.97
]
print(f"complete laps: {len(laps)}")

curves = [lap_curve(a, b) for a, b in laps]
for i, (_, _, ln, rms) in enumerate(curves):
    print(f"  lap {i}: length {ln:.1f} m, shape fit rms {rms/loop_len*ln:.1f} m")

fit_pct, fit_arc, fit_len, _ = curves[0]
fig, ax = plt.subplots(figsize=(9, 4.6))
for i, (pct_i, arc_i, len_i, _) in enumerate(curves[1:], start=1):
    before = arc_i - pct_i
    before -= before[0]
    after = arc_i - np.interp(pct_i, fit_pct, fit_arc)
    after -= after[0]
    rms_b = np.sqrt(np.mean(before ** 2)) * len_i
    rms_a = np.sqrt(np.mean(after ** 2)) * len_i
    print(
        f"  lap {i} scored against the lap-0 table: "
        f"rms error {rms_b:5.1f} m -> {rms_a:4.1f} m, "
        f"worst {np.abs(before).max()*len_i:5.1f} m -> {np.abs(after).max()*len_i:4.1f} m"
    )
    ax.plot(pct_i * 100, before * len_i, lw=1.5, label=f"lap {i}: uncalibrated")
    ax.plot(pct_i * 100, after * len_i, lw=1.5, label=f"lap {i}: calibrated")

ax.axhline(0, color="k", lw=0.6)
ax.set_xlabel("lap %")
ax.set_ylabel("dot offset from car (m)")
ax.set_title("Map placement error, before and after calibration (cross-validated)")
ax.legend(fontsize=9)
ax.grid(alpha=0.3)
plt.tight_layout()
plt.savefig("trackpath_calibrated.png", dpi=110)
print("wrote trackpath_calibrated.png")
