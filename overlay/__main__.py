"""Enable ``python3 -m overlay`` as an entry point for the multi-widget HUD."""

from .app import main

if __name__ == "__main__":
    raise SystemExit(main())
