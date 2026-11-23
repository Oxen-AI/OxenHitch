"""Windows Python application entrypoint.

| Stage | Mechanism | Tag | Ref |
| --- | --- | --- | --- |
| IMPORT | Load the Python desktop shell | [mandatory] | R5 |
| RUN | Start the app when executed by Python or PyInstaller | [mandatory] | R5 |

EARS guards: R5 WHEN invoked as the application script, THEN call the Python shell.
CLOSED: this entrypoint never invokes Cargo or Tauri.
Preconditions: Windows Python dependencies and built UI assets are installed.
Failure modes: startup errors propagate to the launcher.
"""

from hitch.app import run


if __name__ == "__main__":
    run()
