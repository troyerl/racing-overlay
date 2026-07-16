"""Locate and launch the Rust ``gridglance-overlay`` binary."""

from __future__ import annotations

import os
import shutil
import subprocess
import sys
from pathlib import Path

_REPO_ROOT = Path(__file__).resolve().parent.parent


def overlay_binary_candidates() -> list[Path]:
    env = os.environ.get("GRIDGLANCE_OVERLAY_BIN")
    out: list[Path] = []
    if env:
        out.append(Path(env))
    # Dev build
    target = _REPO_ROOT / "overlay-rs" / "target"
    for profile in ("release", "debug"):
        out.append(target / profile / "gridglance-overlay")
        out.append(target / profile / "gridglance-overlay.exe")
    # Next to frozen exe / repo
    if getattr(sys, "frozen", False):
        out.append(Path(sys.executable).parent / "gridglance-overlay.exe")
        out.append(Path(sys.executable).parent / "gridglance-overlay")
    which = shutil.which("gridglance-overlay")
    if which:
        out.append(Path(which))
    return out


def find_overlay_binary() -> Path | None:
    for p in overlay_binary_candidates():
        if p.is_file() and os.access(p, os.X_OK):
            return p
    # Windows: isfile is enough (X_OK can be odd)
    for p in overlay_binary_candidates():
        if p.is_file():
            return p
    return None


def start_rust_overlay(
    *,
    demo: bool = False,
    click_through: bool = True,
    ipc_port: int = 19847,
    stopped: bool = False,
    settings: bool = False,
) -> subprocess.Popen:
    binary = find_overlay_binary()
    if binary is None:
        raise FileNotFoundError(
            "gridglance-overlay not found. Build with: "
            "cd overlay-rs && cargo build --release"
        )
    cmd = [str(binary), "--ipc-port", str(ipc_port)]
    if demo:
        cmd.append("--demo")
    if not click_through:
        cmd.append("--no-clickthrough")
    if stopped:
        cmd.append("--stopped")
    if settings:
        cmd.append("--settings")
    return subprocess.Popen(
        cmd,
        stdout=subprocess.DEVNULL,
        stderr=None,
        cwd=str(_REPO_ROOT),
    )
