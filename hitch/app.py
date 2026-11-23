"""Launch the React frontend in pywebview with a Windows notification-area icon.

| Stage | Mechanism | Tag | Ref |
| --- | --- | --- | --- |
| ASSET | Resolve bundled frontend and icons | [mandatory] | R5 |
| URI | Add desktop marker as a file-URL fragment | [mandatory] | R6 |
| API_READY | Wait for delayed JS bridge injection | [mandatory] | R6 |
| WINDOW | Create pywebview with the Python bridge | [mandatory] | R5 |
| RELAY | Resume the saved LiteLLM alias endpoint | [mandatory] | L2 |
| TRAY | Run pystray in its Windows-compatible thread | [mandatory] | R5 |
| SHUTDOWN | Stop tray after the window closes | [mandatory] | R5 |

EARS guards: R5 WHEN packaged, resolve assets from PyInstaller's bundle root.
R5 WHEN the window closes, THEN stop the tray loop and exit.
R6 WHEN loading a file URL, THEN put the Python desktop marker in the fragment so it does not alter the local file path.
R6 WHEN loading the Python frontend, THEN identify desktop mode before React chooses an API.
CLOSED: no Tauri or Cargo command is invoked by this entrypoint.
Preconditions: frontend ``dist/index.html`` and tray icon asset exist.
Failure modes: missing assets or unsupported WebView2 runtime fail at startup.
"""

from __future__ import annotations

import sys
from pathlib import Path
from threading import Thread
from typing import Any

import pystray
import webview
from PIL import Image

from .bridge import HitchBridge
from .service import HitchService


def resource_path(relative: str) -> Path:
    """Resolve a bundled asset or source-tree path.

    Require: relative is a project-relative resource path.
    Guarantee: returns a path beneath PyInstaller root or source project.
    Maintain: path resolution does not create or modify files.
    Assert: frontend and tray assets share one bundle root.
    """
    root = Path(getattr(sys, "_MEIPASS", Path(__file__).resolve().parents[1]))
    return root / relative


def show_window(window: Any) -> None:
    """Show and focus the primary Hitch window.

    Require: window supports pywebview show and restore methods.
    Guarantee: window becomes visible to the user.
    Maintain: service state is unchanged.
    Assert: tray action does not create duplicate windows.
    """
    window.show()
    window.restore()


def frontend_url(frontend: Path) -> str:
    """Build the packaged frontend URL with a file-safe desktop marker.

    Require: frontend is an existing local HTML file.
    Guarantee: returns a file URI whose local path remains frontend and whose fragment marks Python desktop mode.
    Maintain: no query text is appended to the Windows file path.
    Assert: the URL fragment is exactly ``#desktop=python``.
    """
    if not frontend.is_file():
        raise FileNotFoundError(frontend)
    return f"{frontend.resolve().as_uri()}#desktop=python"


def run() -> None:
    """Start the Python desktop shell and its tray icon.

    Require: ``dist/index.html`` and ``src-tauri/icons/tray.png`` exist.
    Guarantee: Python owns the bridge, window, and tray lifecycle.
    Maintain: closing the window stops the tray thread.
    Assert: one pywebview window is created for the process lifetime.
    """
    frontend = resource_path("dist/index.html")
    tray_asset = resource_path("src-tauri/icons/tray.png")
    if not frontend.is_file() or not tray_asset.is_file():
        raise FileNotFoundError("Build the React frontend and include the Hitch tray icon before launching.")

    service = HitchService.for_user()
    bridge = HitchBridge(service)
    window = webview.create_window(
        "Oxen Hitch",
        frontend_url(frontend),
        js_api=bridge,
        width=340,
        height=600,
        min_size=(340, 120),
        resizable=False,
    )
    bridge.bind_window(window)
    service.restore_relay()

    image = Image.open(tray_asset).convert("RGBA")
    tray = pystray.Icon(
        "OxenHitch",
        image,
        "Oxen Hitch",
        menu=pystray.Menu(
            pystray.MenuItem("Open Oxen Hitch", lambda _icon, _item: show_window(window), default=True),
            pystray.MenuItem("Quit", lambda _icon, _item: bridge.quit()),
        ),
    )
    tray_thread = Thread(target=tray.run, name="OxenHitchTray", daemon=True)
    tray_thread.start()
    window.events.closing += bridge.allow_close
    window.events.closed += tray.stop
    try:
        webview.start(debug=False)
    finally:
        service.close_relay()
        tray.stop()
        tray_thread.join(timeout=2)


if __name__ == "__main__":
    run()
