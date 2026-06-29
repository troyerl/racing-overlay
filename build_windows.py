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


def build(icon: str | None) -> str:
    ensure("PyInstaller")
    sep = os.pathsep  # ';' on Windows, ':' elsewhere
    args = [
        sys.executable, "-m", "PyInstaller",
        "--noconfirm", "--clean",
        "--name", APP_NAME,
        "--windowed",                       # no console window
        "--add-data", f"{os.path.join(ROOT, 'assets')}{sep}assets",
        "--add-data", f"{os.path.join(ROOT, 'tracks')}{sep}tracks",
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
    exe = build(icon)
    # CI passes --no-shortcut (it packages an installer instead).
    if "--no-shortcut" not in sys.argv:
        make_desktop_shortcut(exe)
        print("\nDone. Double-click the 'GridGlance' desktop icon to launch.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
