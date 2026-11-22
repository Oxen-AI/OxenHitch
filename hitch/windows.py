"""Inspect the supported Windows Claude Desktop installation and policy.

| Stage | Mechanism | Tag | Ref |
| --- | --- | --- | --- |
| DISCOVER | Resolve Squirrel root and versioned executables | [mandatory] | R4 |
| IDENTIFY | Match process executable paths exactly | [mandatory] | R4 |
| POLICY | Apply HKLM-over-HKCU policy precedence | [mandatory] | R4 |
| RESTART | Report safe manual quit/reopen steps | [mandatory] | R4 |

EARS guards: R4 WHEN process identity is queried, only validated install paths match.
R4 IF either policy hive cannot be read, then report the policy probe failure.
CLOSED: force-termination and process-name-only matching are excluded.
Preconditions: Windows paths are supplied or discovered from LOCALAPPDATA.
Failure modes: denied process and registry access is surfaced as an error.
"""

from __future__ import annotations

import os
import winreg
from dataclasses import dataclass
from pathlib import Path
from typing import Iterable

import psutil


@dataclass(frozen=True)
class ClaudeInstall:
    """Resolved Claude Desktop installation paths.

    Require: root is a Squirrel installation root.
    Guarantee: executable paths include the stable launcher and versioned apps.
    Maintain: helper processes are never treated as the main application.
    Assert: all returned executable paths are direct claude.exe children.
    """

    root: Path

    @classmethod
    def discover(cls, local_app_data: str | Path | None = None) -> "ClaudeInstall":
        """Find the observed per-user Squirrel install root.

        Require: optional path identifies LOCALAPPDATA.
        Guarantee: discovery performs no writes.
        Maintain: only the supported AnthropicClaude root is considered.
        Assert: executable list stays empty when the root is absent.
        """
        local = local_app_data or os.environ.get("LOCALAPPDATA")
        if not local:
            local = Path(os.environ.get("USERPROFILE", ".")) / "AppData" / "Local"
        base = Path(local)
        return cls(base / "AnthropicClaude")

    @property
    def launch_executable(self) -> Path:
        """Return the stable launcher path used after manual quit.

        Require: root was discovered or explicitly injected.
        Guarantee: no version-specific path is persisted.
        Maintain: no process is launched by property access.
        Assert: returned path ends in claude.exe.
        """
        return self.root / "claude.exe"

    def executables(self) -> list[Path]:
        """List existing stable and versioned main executable candidates.

        Require: root identifies a Claude installation directory.
        Guarantee: helper subdirectories are excluded.
        Maintain: candidates are direct children of root or app-* folders.
        Assert: missing root produces an empty list.
        """
        candidates = [self.launch_executable]
        if self.root.exists():
            candidates.extend(
                app_dir / "claude.exe"
                for app_dir in self.root.glob("app-*")
                if app_dir.is_dir()
            )
        return [path for path in candidates if path.is_file()]

    def running_processes(self, processes: Iterable[object] | None = None) -> list[tuple[int, float]]:
        """Return exact-path Claude Desktop process IDs and creation times.

        Require: process provider returns objects with ``info`` or exe/create_time methods.
        Guarantee: unrelated Claude CLI executables are excluded.
        Maintain: this method never signals or terminates a process.
        Assert: matching uses normalized full executable paths.
        """
        accepted = {os.path.normcase(os.path.abspath(path)) for path in self.executables()}
        observed = psutil.process_iter(["exe", "create_time"]) if processes is None else processes
        matches: list[tuple[int, float]] = []
        for process in observed:
            info = getattr(process, "info", None)
            executable = info.get("exe") if isinstance(info, dict) else process.exe()
            created = info.get("create_time") if isinstance(info, dict) else process.create_time()
            if executable and os.path.normcase(os.path.abspath(executable)) in accepted:
                matches.append((int(process.pid), float(created)))
        return matches


def inference_managed(machine_values: dict[str, object] | None, user_values: dict[str, object] | None) -> bool:
    """Apply Claude's machine-policy precedence to inference control.

    Require: each argument is None when absent or a mapping of registry values.
    Guarantee: machine policy replaces user policy instead of merging with it.
    Maintain: only inferenceProvider/bootstrapUrl mark inference as managed.
    Assert: unrelated application-behavior keys do not mark the install managed.
    """
    effective = machine_values if machine_values is not None else user_values
    return bool(effective and any(effective.get(key) for key in ("inferenceProvider", "bootstrapUrl")))


def read_managed_policy() -> bool:
    """Read HKLM/HKCU Claude policy and report access failures.

    Require: process has registry read access for the policy paths.
    Guarantee: machine hive takes precedence when the key exists.
    Maintain: missing keys are represented as absent; denied reads raise.
    Assert: values are read from the 64-bit policy view where available.
    """
    machine = _read_policy(winreg.HKEY_LOCAL_MACHINE)
    user = _read_policy(winreg.HKEY_CURRENT_USER)
    return inference_managed(machine, user)


def _read_policy(hive: int) -> dict[str, object] | None:
    """Read one Claude policy key without hiding access errors.

    Require: hive is a Windows registry root handle.
    Guarantee: absent key returns None and existing key returns its values.
    Maintain: no registry writes occur.
    Assert: permission failures escape to the caller.
    """
    try:
        with winreg.OpenKey(hive, r"SOFTWARE\Policies\Claude", 0, winreg.KEY_READ | winreg.KEY_WOW64_64KEY) as key:
            values: dict[str, object] = {}
            index = 0
            while True:
                try:
                    name, value, _kind = winreg.EnumValue(key, index)
                except OSError as error:
                    if getattr(error, "winerror", None) == 259:
                        break
                    raise
                values[name] = value
                index += 1
            return values
    except FileNotFoundError:
        return None


def manual_restart_instructions() -> str:
    """Return the safe restart instruction until graceful quit is verified.

    Require: caller needs a Windows restart response.
    Guarantee: tells the user to quit and reopen Claude manually.
    Maintain: the function never launches or terminates a process.
    Assert: returned message includes both manual actions.
    """
    return "Quit Claude yourself, then open it again to load the updated settings."
