"""Read and atomically update provider and local relay credentials in Oxen auth TOML.

| Stage | Mechanism | Tag | Ref |
| --- | --- | --- | --- |
| LOAD | Parse host tokens while preserving comments | [mandatory] | R2 |
| SELECT | Locate a provider token by host or relay token by profile key | [contract] | R2,L5 |
| SAVE | Replace or remove one credential entry through an atomic sibling file | [mandatory] | R2,L5 |

EARS guards: R2 credentials must never enter Hitch state or status values; L5 relay credentials use the reserved hitch-relay:<profile_id> key.
CLOSED: unrelated host entries and comments are preserved.
Preconditions: caller supplies an isolated sandbox path or the user's Oxen auth path.
Failure modes: malformed existing TOML raises on mutation and returns no key on lookup.
"""
from __future__ import annotations

import os
import tempfile
from pathlib import Path
from typing import Optional

import tomlkit


class OxenConfig:
    """Credential store compatible with Oxen CLI's auth_config.toml.

    Require: path is user-owned.
    Guarantee: a write preserves unrelated TOML content and host entries.
    Maintain: a host has at most one mutated token entry.
    Assert: replacement occurs only after the temp file is flushed.
    """

    def __init__(self, path: Path):
        """Bind the store to an explicit auth file.

        Require: path identifies a TOML file.
        Guarantee: construction performs no I/O.
        Maintain: path is the only mutation target.
        Assert: caller controls the root directory.
        """
        self.path = Path(path)

    @classmethod
    def for_user(cls, env: Optional[dict[str, str]] = None) -> "OxenConfig":
        """Return the standard Oxen CLI credential location.

        Require: optional environment values are paths.
        Guarantee: HOME takes precedence over USERPROFILE.
        Maintain: no file is created.
        Assert: fallback is relative only when no home is available.
        """
        values = os.environ if env is None else env
        home = Path(values.get("HOME") or values.get("USERPROFILE") or ".")
        return cls(home / ".config" / "oxen" / "auth_config.toml")

    def get(self, host: str) -> Optional[str]:
        """Return a nonblank token for one host when parseable.

        Require: host is a normalized host label.
        Guarantee: absent, unreadable, malformed, and blank tokens return None.
        Maintain: lookup never writes.
        Assert: token text is returned only to the caller.
        """
        try:
            document = tomlkit.parse(self.path.read_text(encoding="utf-8"))
        except (OSError, tomlkit.exceptions.ParseError):
            return None
        for entry in document.get("host_configs", []):
            if entry.get("host") == host and isinstance(entry.get("auth_token"), str) and entry["auth_token"]:
                return str(entry["auth_token"])
        return None

    def set(self, host: str, key: str) -> None:
        """Persist one host token without disturbing other TOML content.

        Require: host and key are nonempty caller-validated strings.
        Guarantee: existing host token is replaced or a host table is appended.
        Maintain: malformed existing TOML is never overwritten.
        Assert: destination replacement follows a complete temporary write.
        """
        if self.path.exists():
            try:
                document = tomlkit.parse(self.path.read_text(encoding="utf-8"))
            except tomlkit.exceptions.ParseError as error:
                raise ValueError(f"Your Oxen settings file could not be read, so the key was not saved: {self.path}") from error
        else:
            document = tomlkit.document()
            document.add("default_host", "hub.oxen.ai")
        hosts = document.get("host_configs")
        if hosts is None:
            hosts = tomlkit.aot()
            document.add("host_configs", hosts)
        if not isinstance(hosts, tomlkit.container.AoT):
            raise ValueError("Your Oxen settings file has an unexpected layout.")
        for entry in hosts:
            if entry.get("host") == host:
                entry["auth_token"] = key
                self._write(tomlkit.dumps(document))
                return
        entry = tomlkit.table()
        entry.add("host", host)
        entry.add("auth_token", key)
        hosts.append(entry)
        self._write(tomlkit.dumps(document))

    def remove(self, host: str) -> None:
        """Remove one provider or Hitch-internal credential entry.

        Require: host is the exact stored key.
        Guarantee: unrelated hosts and TOML comments remain intact.
        Maintain: a missing credential file remains absent.
        Assert: malformed TOML is never overwritten.
        """
        if not self.path.exists():
            return
        try:
            document = tomlkit.parse(self.path.read_text(encoding="utf-8"))
        except tomlkit.exceptions.ParseError as error:
            raise ValueError(f"Your Oxen settings file could not be read, so the key was not removed: {self.path}") from error
        hosts = document.get("host_configs", [])
        if not isinstance(hosts, tomlkit.container.AoT):
            return
        kept = [entry for entry in hosts if entry.get("host") != host]
        if len(kept) == len(hosts):
            return
        replacement = tomlkit.aot()
        for entry in kept:
            replacement.append(entry)
        document["host_configs"] = replacement
        self._write(tomlkit.dumps(document))

    def _write(self, text: str) -> None:
        """Atomically replace the credential file after a durable temp write.

        Require: parent can be created and written.
        Guarantee: only a complete text serialization replaces destination.
        Maintain: temporary siblings are removed on failure.
        Assert: no alternative destination is used.
        """
        self.path.parent.mkdir(parents=True, exist_ok=True)
        descriptor, temporary = tempfile.mkstemp(prefix=".hitch-", suffix=".tmp", dir=self.path.parent)
        temp = Path(temporary)
        try:
            with os.fdopen(descriptor, "w", encoding="utf-8") as file:
                file.write(text)
                file.flush()
                os.fsync(file.fileno())
            os.replace(temp, self.path)
        finally:
            if temp.exists():
                temp.unlink()
