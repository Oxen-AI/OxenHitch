"""Persist non-secret Hitch state with atomic replacement.

| Stage | Mechanism | Tag | Ref |
| --- | --- | --- | --- |
| LOAD | Decode endpoint settings and the saved loopback port | [mandatory] | R2,L2 |
| SAVE | Serialize to sibling temp then replace | [mandatory] | R2 |

EARS guards: R2 state MUST NOT include credentials.
CLOSED: malformed state is treated as a fresh state to match the existing app.
Preconditions: state path has a writable parent. Failure modes: save failures raise OSError.
"""

from __future__ import annotations

import json
import os
import tempfile
from dataclasses import asdict, dataclass, field
from pathlib import Path
from typing import Any, Optional

from .models import Model


@dataclass
class HitchState:
    """Non-secret settings needed to restore Hitch behavior.

    Require: caller never assigns API keys to this value.
    Guarantee: ``to_dict`` produces JSON-compatible state.
    Maintain: state has no credential field.
    Assert: model catalog uses ``Model`` records.
    """

    profile_id: Optional[str] = None
    previous_applied_id: Optional[str] = None
    previous_mode: Optional[str] = None
    base_url: Optional[str] = None
    models: list[str] = field(default_factory=list)
    catalog: list[Model] = field(default_factory=list)
    last_write_at: Optional[int] = None
    custom_endpoint: bool = False
    relay_port: Optional[int] = None

    def to_dict(self) -> dict[str, Any]:
        """Return snake_case JSON state matching the incumbent disk schema.

        Require: fields satisfy this dataclass contract.
        Guarantee: result includes no credentials.
        Maintain: absent fields serialize as null.
        Assert: catalog records are expanded without mutation.
        """
        return {"profile_id": self.profile_id, "previous_applied_id": self.previous_applied_id, "previous_mode": self.previous_mode, "base_url": self.base_url, "models": self.models, "catalog": [asdict(model) for model in self.catalog], "last_write_at": self.last_write_at, "custom_endpoint": self.custom_endpoint, "relay_port": self.relay_port}

    @classmethod
    def from_dict(cls, data: dict[str, Any]) -> "HitchState":
        """Decode a state object while ignoring unknown fields.

        Require: data was decoded from a JSON object.
        Guarantee: missing or malformed optional collections become empty.
        Maintain: unknown fields never become state attributes.
        Assert: no file is accessed.
        """
        catalog = [Model(**item) for item in data.get("catalog", []) if isinstance(item, dict) and isinstance(item.get("id"), str)]
        port = data.get("relay_port")
        return cls(data.get("profile_id"), data.get("previous_applied_id"), data.get("previous_mode"), data.get("base_url"), list(data.get("models", [])) if isinstance(data.get("models", []), list) else [], catalog, data.get("last_write_at"), data.get("custom_endpoint") is True, port if type(port) is int and 0 < port <= 65535 else None)


class StateStore:
    """Filesystem boundary for a single Hitch state JSON file.

    Require: path identifies a state file.
    Guarantee: saves replace complete files atomically on the same filesystem.
    Maintain: malformed reads never overwrite a file.
    Assert: no credential values are accepted by the schema.
    """

    def __init__(self, path: Path):
        """Bind the store to an explicit state path.

        Require: path is a file path.
        Guarantee: no filesystem I/O occurs during initialization.
        Maintain: path stays immutable by convention.
        Assert: relative paths remain relative to the caller's working directory.
        """
        self.path = Path(path)

    def load(self) -> HitchState:
        """Load a usable state or return a fresh one.

        Require: none.
        Guarantee: missing, unreadable, malformed, or non-object files return fresh state.
        Maintain: load never writes.
        Assert: valid JSON decodes through ``HitchState``.
        """
        try:
            data = json.loads(self.path.read_text(encoding="utf-8"))
            return HitchState.from_dict(data) if isinstance(data, dict) else HitchState()
        except (OSError, json.JSONDecodeError):
            return HitchState()

    def save(self, state: HitchState) -> None:
        """Atomically write state to its configured path.

        Require: state contains no credentials and parent is writable.
        Guarantee: destination is replaced only after a complete sibling temporary write.
        Maintain: parent exists after successful save.
        Assert: a failed write leaves the old destination intact where OS replacement permits.
        """
        self.path.parent.mkdir(parents=True, exist_ok=True)
        descriptor, raw_temp = tempfile.mkstemp(prefix=".hitch-", suffix=".tmp", dir=self.path.parent)
        temp = Path(raw_temp)
        try:
            with os.fdopen(descriptor, "w", encoding="utf-8") as file:
                json.dump(state.to_dict(), file, indent=2)
                file.flush()
                os.fsync(file.fileno())
            os.replace(temp, self.path)
        finally:
            if temp.exists():
                temp.unlink()
