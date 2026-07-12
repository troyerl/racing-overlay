#!/usr/bin/env python3
"""Build a standalone GridGlance app and put a shortcut on the Desktop.

Run this ONCE (on the machine you want to use the overlay on):

    python build_windows.py

It produces ``dist/GridGlance/GridGlance.exe`` (a self-contained folder
you can move anywhere) and, on Windows, drops a "GridGlance" shortcut on your
Desktop. After that just double-click the desktop icon -- no terminal needed.

Notes
-----
* PyInstaller cannot cross-compile, so run this on Windows to get a Windows exe.
* Missing build tools (PyInstaller / Pillow) are installed automatically.
"""

from __future__ import annotations

import os
import shutil
import subprocess
import sys

ROOT = os.path.dirname(os.path.abspath(__file__))
APP_NAME = "GridGlance"
ENTRY = os.path.join(ROOT, "run.py")
PNG = os.path.join(ROOT, "assets", "app.png")
ICO = os.path.join(ROOT, "assets", "app.ico")


def _pip_install(*pkgs: str) -> None:
    subprocess.check_call([sys.executable, "-m", "pip", "install", *pkgs])


def ensure(pkg: str, import_name: str | None = None) -> None:
    try:
        __import__(import_name or pkg)
    except ImportError:
        print(f"Installing {pkg}\u2026")
        _pip_install(pkg)


def make_icon() -> str | None:
    """Convert assets/app.png into a multi-resolution .ico (square, centered)."""
    if not os.path.exists(PNG):
        print("No assets/app.png found; building without a custom icon.")
        return None
    ensure("Pillow", "PIL")
    from PIL import Image

    img = Image.open(PNG).convert("RGBA")
    side = min(img.size)
    left = (img.width - side) // 2
    top = (img.height - side) // 2
    img = img.crop((left, top, left + side, top + side))
    img.save(ICO, sizes=[(s, s) for s in (16, 24, 32, 48, 64, 128, 256)])
    print(f"Wrote {ICO}")
    return ICO


def build_rust_overlay() -> str | None:
    """Build ``gridglance-overlay`` and return the binary path, or None."""
    cargo = shutil.which("cargo")
    rs = os.path.join(ROOT, "overlay-rs")
    if cargo is None or not os.path.isdir(rs):
        print("Skipping Rust overlay build (cargo / overlay-rs not found).")
        return None
    env = os.environ.copy()
    env["CARGO_TARGET_DIR"] = os.path.join(rs, "target")
    print("Building Rust overlay (release)…")
    subprocess.check_call(
        [cargo, "build", "--release", "-p", "gridglance-overlay"],
        cwd=rs,
        env=env,
    )
    name = "gridglance-overlay.exe" if os.name == "nt" else "gridglance-overlay"
    path = os.path.join(rs, "target", "release", name)
    if not os.path.isfile(path):
        print(f"Rust build finished but binary missing: {path}")
        return None
    return path


def copy_rust_overlay(exe: str, rust_bin: str | None) -> None:
    if not rust_bin:
        return
    dest_dir = os.path.dirname(exe)
    dest = os.path.join(
        dest_dir,
        "gridglance-overlay.exe" if os.name == "nt" else "gridglance-overlay",
    )
    shutil.copy2(rust_bin, dest)
    print(f"Bundled Rust overlay: {dest}")


def build(icon: str | None) -> str:
    ensure("PyInstaller")
    sep = os.pathsep  # ';' on Windows, ':' elsewhere
    args = [
        sys.executable, "-m", "PyInstaller",
        "--noconfirm", "--clean",
        "--name", APP_NAME,
        "--windowed",                       # no console window
        "--add-data", f"{os.path.join(ROOT, 'assets')}{sep}assets",
        # Ship ONLY the demo track -- real maps come from the cloud, never the
        # bundle. (Bundling the whole tracks/ folder would ship any maps a dev
        # happened to scan locally before building.)
        "--add-data", f"{os.path.join(ROOT, 'tracks', '_demo.json')}{sep}tracks",
        # pymongo + dnspython (mongodb+srv) are imported lazily, so PyInstaller's
        # static analysis can miss them -- bundle them explicitly.
        "--hidden-import", "pymongo",
        "--hidden-import", "dns",
        # Track Scan v2 HTML import (lazy imports; PyInstaller misses these).
        "--hidden-import", "tools.svg_layers_to_track_v2",
        "--hidden-import", "tools.svg_layers_to_track",
        "--hidden-import", "tools.schematic_to_track",
        "--hidden-import", "bs4",
        "--hidden-import", "svgpathtools",
    ]
    if icon:
        args += ["--icon", icon]
    args.append(ENTRY)
    print("Running:", " ".join(args))
    subprocess.check_call(args, cwd=ROOT)

    exe = os.path.join(ROOT, "dist", APP_NAME,
                       APP_NAME + (".exe" if os.name == "nt" else ""))
    print(f"\nBuilt: {exe}")
    return exe


def make_desktop_shortcut(exe: str) -> None:
    if os.name != "nt":
        print("Not on Windows -- skipping desktop shortcut "
              "(run this script on Windows to create it).")
        return
    work_dir = os.path.dirname(exe)
    ps = (
        "$d=[Environment]::GetFolderPath('Desktop');"
        f"$lnk=Join-Path $d '{APP_NAME}.lnk';"
        "$s=(New-Object -ComObject WScript.Shell).CreateShortcut($lnk);"
        f"$s.TargetPath='{exe}';"
        f"$s.WorkingDirectory='{work_dir}';"
        f"$s.IconLocation='{exe},0';"
        "$s.Save()"
    )
    try:
        subprocess.check_call(["powershell", "-NoProfile", "-Command", ps])
        print("Created desktop shortcut: GridGlance")
    except Exception as exc:  # noqa: BLE001
        print(f"Could not create desktop shortcut automatically: {exc}")


def main() -> int:
    icon = make_icon()
    rust_bin = build_rust_overlay()
    exe = build(icon)
    copy_rust_overlay(exe, rust_bin)
    # CI passes --no-shortcut (it packages an installer instead).
    if "--no-shortcut" not in sys.argv:
        make_desktop_shortcut(exe)
        print("\nDone. Double-click the 'GridGlance' desktop icon to launch.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
