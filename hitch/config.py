"""Manage Claude Desktop configLibrary profiles and sandbox paths.

| Stage | Mechanism | Tag | Ref |
| --- | --- | --- | --- |
| DISCOVER | Find Windows Claude data or sandbox override | [mandatory] | R1 |
| SNAPSHOT | Read current applied profile and deployment mode | [mandatory] | R1 |
| RESTORE_TOKEN | Read an unsealed profile token only for migration; Claude may seal saved credentials | [mandatory] | L2,L5 |
| REPAIR_LABELS | Replace prior alias-only display names in owned profiles | [contract] | L2 |
| APPLY | Preserve unknown JSON while applying gateway settings | [mandatory] | R1 |
| RESTORE | Remove Hitch profile and restore prior selection | [mandatory] | R1 |

EARS guards: R1 WHERE ``HITCH_CLAUDE_DIR`` exists, all configuration paths derive from it.
R1 IF a JSON object cannot parse, mutation fails before changing that file.
L5 WHEN Hitch replaces a relay token, it MUST preserve all other Claude profile fields.
CLOSED: Windows registry policy and process discovery are intentionally owned by later modules.
Preconditions: IDs are UUID-shaped strings and callers own sandbox/profile paths.
Failure modes: malformed strict files and filesystem failures raise ``ValueError``/``OSError``.
"""

from __future__ import annotations

import json
import os
import tempfile
import uuid
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any, Optional

from .models import ModelEntry

PROFILE_NAME = "Oxen"


@dataclass(frozen=True)
class ClaudePaths:
    """Resolved Claude third-party configuration locations.

    Require: ``data_dir`` names the Claude-3p directory.
    Guarantee: derived locations remain below ``data_dir``.
    Maintain: a sandbox override never resolves real Claude configuration.
    Assert: no filesystem mutation happens while resolving paths.
    """

    data_dir: Path

    @classmethod
    def at(cls, data_dir: Path) -> "ClaudePaths":
        """Create explicit paths for fixture or sandbox use.

        Require: data_dir is a configuration root.
        Guarantee: no environment value is consulted.
        Maintain: all derived locations are descendants.
        Assert: no I/O occurs.
        """
        return cls(Path(data_dir))

    @classmethod
    def detect(cls, env: Optional[dict[str, str]] = None) -> "ClaudePaths":
        """Resolve the Windows default or ``HITCH_CLAUDE_DIR`` override.

        Require: env values, when supplied, are string paths.
        Guarantee: sandbox override takes priority.
        Maintain: default is LOCALAPPDATA/Claude-3p.
        Assert: no directory is created.
        """
        values = os.environ if env is None else env
        if values.get("HITCH_CLAUDE_DIR"):
            return cls.at(Path(values["HITCH_CLAUDE_DIR"]))
        user = Path(values.get("USERPROFILE") or ".")
        local = Path(values.get("LOCALAPPDATA")) if values.get("LOCALAPPDATA") else user / "AppData" / "Local"
        return cls.at(local / "Claude-3p")

    @property
    def library_dir(self) -> Path:
        """Return Claude's profile library directory.

        Require: data_dir is configured.
        Guarantee: path is below data_dir.
        Maintain: no directory is created.
        Assert: returned path ends in configLibrary.
        """
        return self.data_dir / "configLibrary"

    @property
    def meta_file(self) -> Path:
        """Return Claude's config library metadata file.

        Require: none.
        Guarantee: path is below the library directory.
        Maintain: no file is created.
        Assert: name is _meta.json.
        """
        return self.library_dir / "_meta.json"

    def profile_file(self, profile_id: str) -> Path:
        """Return a profile JSON location.

        Require: profile_id is a safe UUID-like identifier.
        Guarantee: generated filename ends in .json within library_dir.
        Maintain: no I/O occurs.
        Assert: caller owns id validation when accepting external input.
        """
        return self.library_dir / f"{profile_id}.json"

    @property
    def desktop_config(self) -> Path:
        """Return Claude's deployment-mode configuration file.

        Require: none.
        Guarantee: path is directly below data_dir.
        Maintain: no file is created.
        Assert: name follows the documented Claude convention.
        """
        return self.data_dir / "claude_desktop_config.json"


@dataclass(frozen=True)
class GatewayProfile:
    """Gateway values applied to one Claude profile.

    Require: base_url is normalized by the gateway module.
    Guarantee: None api_key preserves a nonblank existing key.
    Maintain: model entries are already Claude-compatible.
    Assert: applying is handled by ``apply_profile``.
    """

    base_url: str
    api_key: Optional[str]
    models: list[ModelEntry] = field(default_factory=list)
    discover_models: bool = True


@dataclass(frozen=True)
class ProfileView:
    """Minimal observable profile view for service status.

    Require: values are decoded JSON fields.
    Guarantee: key presence is boolean without exposing the key.
    Maintain: models contain only names.
    Assert: no credential text is retained here.
    """

    provider: Optional[str] = None
    base_url: Optional[str] = None
    has_key: bool = False
    models: list[str] = field(default_factory=list)


@dataclass(frozen=True)
class Snapshot:
    """Current Claude profile-library selection.

    Require: values originate from profile files.
    Guarantee: unreadable files produce empty/default fields.
    Maintain: snapshot does not write.
    Assert: deployment mode is 1p, 3p, or None.
    """

    applied_id: Optional[str] = None
    entry_ids: list[str] = field(default_factory=list)
    mode: Optional[str] = None
    applied: Optional[ProfileView] = None


def new_profile_id() -> str:
    """Create an accepted random profile identifier.

    Require: none.
    Guarantee: returns lowercase UUID text.
    Maintain: IDs are generated independently.
    Assert: output satisfies ``_is_profile_id``.
    """
    return str(uuid.uuid4())


def snapshot(paths: ClaudePaths) -> Snapshot:
    """Read Claude's applied profile without raising for missing files.

    Require: paths are an approved sandbox or user root.
    Guarantee: absent/unreadable files produce an empty snapshot.
    Maintain: filesystem remains unchanged.
    Assert: applied profile is read only for a UUID-shaped applied id.
    """
    meta = _read_object(paths.meta_file) or {}
    applied_id = meta.get("appliedId") if _is_profile_id(meta.get("appliedId")) else None
    desktop = _read_object(paths.desktop_config) or {}
    applied = _view(_read_object(paths.profile_file(applied_id))) if applied_id and _read_object(paths.profile_file(applied_id)) is not None else None
    return Snapshot(applied_id, _entry_ids(meta), desktop.get("deploymentMode") if desktop.get("deploymentMode") in ("1p", "3p") else None, applied)


def profile_models(paths: ClaudePaths, profile_id: str) -> list[Any]:
    """Return the configured model entries for one saved profile.

    Require: profile_id is a Claude profile identifier.
    Guarantee: absent or malformed model lists return an empty list.
    Maintain: stored entry order and JSON-compatible values are preserved.
    Assert: profile files are never changed.
    """
    profile = _read_object(paths.profile_file(profile_id)) or {}
    entries = profile.get("inferenceModels")
    return list(entries) if isinstance(entries, list) else []


def profile_key(paths: ClaudePaths, profile_id: str) -> Optional[str]:
    """Read the existing Claude gateway credential for native code only.

    Args:
        paths: Claude configuration root.
        profile_id: Saved Hitch profile ID.
    Returns:
        Nonblank credential or None for an absent key.
    """
    if not _is_profile_id(profile_id):
        return None
    value = (_read_object(paths.profile_file(profile_id)) or {}).get("inferenceGatewayApiKey")
    return value if isinstance(value, str) and value else None


def set_profile_key(paths: ClaudePaths, profile_id: str, key: str) -> None:
    """Replace only Hitch's credential in an existing Claude profile.

    Require: profile_id is UUID-shaped and key is the plaintext relay token.
    Guarantee: unrelated Claude profile fields survive the atomic write.
    Maintain: credentials are never returned in status views.
    Assert: malformed or absent profiles fail before replacing the file.
    """
    if not _is_profile_id(profile_id):
        raise ValueError("Claude profile IDs must be UUID-shaped.")
    file = paths.profile_file(profile_id)
    profile = _read_object_strict(file)
    profile["inferenceGatewayApiKey"] = key
    _write_object(file, profile)


def repair_profile_labels(paths: ClaudePaths, profile_id: str, labels: dict[str, str]) -> bool:
    """Correct older alias-only labels without editing the profile's routing fields.

    Args:
        paths: Claude configuration root.
        profile_id: Hitch-owned profile ID.
        labels: Wire alias to original display name.
    Returns:
        Whether any labels were repaired.
    """
    if not _is_profile_id(profile_id):
        return False
    file = paths.profile_file(profile_id)
    profile = _read_object(file) or {}
    entries = profile.get("inferenceModels")
    if not isinstance(entries, list):
        return False
    changed = False
    for entry in entries:
        if not isinstance(entry, dict):
            continue
        alias = entry.get("name")
        if isinstance(alias, str) and alias in labels and entry.get("labelOverride") == alias and labels[alias] != alias:
            entry["labelOverride"] = labels[alias]
            changed = True
    if changed:
        _write_object(file, profile)
    return changed


def discovery_enabled(paths: ClaudePaths, profile_id: str) -> bool:
    """Return whether Claude model discovery is enabled for a profile.

    Require: profile_id is a Claude profile identifier.
    Guarantee: absent or malformed values return False.
    Maintain: profile JSON remains unchanged.
    Assert: only the exact boolean True enables discovery.
    """
    profile = _read_object(paths.profile_file(profile_id)) or {}
    return profile.get("modelDiscoveryEnabled") is True


def apply_profile(paths: ClaudePaths, profile_id: str, profile: GatewayProfile, *, active: bool = True) -> None:
    """Write a gateway profile, select it, and enable third-party deployment.

    Require: profile_id is UUID-shaped; api_key is supplied or already present.
    Guarantee: unknown profile/meta/desktop fields survive.
    Maintain: strict desktop parsing completes before any write.
    Assert: all writes use sibling temporary files then replacement.
    """
    if not _is_profile_id(profile_id):
        raise ValueError("Claude profile IDs must be UUID-shaped.")
    file = paths.profile_file(profile_id)
    body = _read_object(file) or {}
    if profile.api_key is None and not (isinstance(body.get("inferenceGatewayApiKey"), str) and body["inferenceGatewayApiKey"]):
        raise ValueError("An API key is needed to connect.")
    desktop = _read_object_strict(paths.desktop_config)
    meta = _read_object(paths.meta_file) or {}
    imported = body.get("claudeAiImport")
    body["claudeAiImport"] = {**imported, "enabled": True} if isinstance(imported, dict) else {"enabled": True}
    body.update({"inferenceProvider": "gateway", "inferenceGatewayBaseUrl": profile.base_url, "inferenceCredentialKind": "static"})
    if profile.api_key is not None:
        body["inferenceGatewayApiKey"] = profile.api_key
    if profile.models:
        if profile.discover_models:
            body["modelDiscoveryEnabled"] = True
        else:
            body.pop("modelDiscoveryEnabled", None)
        body["inferenceModels"] = [model.to_config() for model in profile.models]
    else:
        if profile.discover_models:
            body["modelDiscoveryEnabled"] = True
        else:
            body.pop("modelDiscoveryEnabled", None)
        body.pop("inferenceModels", None)
    entries = _entries(meta)
    if not any(_entry_id(entry) == profile_id for entry in entries):
        entries.append({"id": profile_id, "name": PROFILE_NAME})
    meta.update({"appliedId": profile_id, "entries": entries})
    desktop["deploymentMode"] = "3p" if active else "1p"
    _write_object(file, body)
    _write_object(paths.meta_file, meta)
    _write_object(paths.desktop_config, desktop)


def set_mode(paths: ClaudePaths, mode: Optional[str]) -> None:
    """Set or clear Claude deployment mode while preserving all other fields.

    Require: mode is None, 1p, or 3p.
    Guarantee: only deploymentMode changes.
    Maintain: malformed desktop files are preserved.
    Assert: mutation is atomic.
    """
    if mode not in (None, "1p", "3p"):
        raise ValueError("Deployment mode must be 1p, 3p, or None.")
    desktop = _read_object_strict(paths.desktop_config)
    if mode is None:
        desktop.pop("deploymentMode", None)
    else:
        desktop["deploymentMode"] = mode
    _write_object(paths.desktop_config, desktop)


def remove_profile(paths: ClaudePaths, profile_id: str, restore_applied: Optional[str], restore_mode: Optional[str]) -> None:
    """Remove Hitch's profile and restore the prior valid selection.

    Require: desktop config is parseable; restore mode follows ``set_mode`` contract.
    Guarantee: Hitch profile is removed; a valid prior profile is selected if available.
    Maintain: other profile entries and unknown metadata survive.
    Assert: malformed desktop config prevents deletion.
    """
    _read_object_strict(paths.desktop_config)
    file = paths.profile_file(profile_id)
    if file.exists():
        file.unlink()
    meta = _read_object(paths.meta_file)
    if meta is not None:
        entries = [entry for entry in _entries(meta) if _entry_id(entry) != profile_id]
        remaining = [item for item in (_entry_id(entry) for entry in entries) if item]
        if not remaining:
            if paths.meta_file.exists():
                paths.meta_file.unlink()
            try:
                paths.library_dir.rmdir()
            except OSError:
                pass
        else:
            if meta.get("appliedId") == profile_id:
                meta["appliedId"] = restore_applied if restore_applied in remaining else remaining[0]
            meta["entries"] = entries
            _write_object(paths.meta_file, meta)
    set_mode(paths, restore_mode)


def _read_object(path: Path) -> Optional[dict[str, Any]]:
    """Read a JSON object or return None for absent/unparseable values.

    Require: path is a configuration file location.
    Guarantee: errors are represented as None for snapshot semantics.
    Maintain: no file is changed.
    Assert: scalar and array JSON are rejected.
    """
    try:
        value = json.loads(path.read_text(encoding="utf-8"))
        return value if isinstance(value, dict) else None
    except (OSError, json.JSONDecodeError):
        return None


def _read_object_strict(path: Path) -> dict[str, Any]:
    """Read a mutation target without permitting destructive parse fallback.

    Require: path is the desktop configuration file.
    Guarantee: missing becomes an empty object; malformed content raises.
    Maintain: the source file is not changed.
    Assert: non-object JSON is rejected.
    """
    if not path.exists():
        return {}
    value = _read_object(path)
    if value is None:
        raise ValueError(f"Claude's settings file could not be read, so nothing was changed: {path}")
    return value


def _write_object(path: Path, value: dict[str, Any]) -> None:
    """Atomically serialize a JSON object at one configuration path.

    Require: parent location is writable.
    Guarantee: replace happens after a flushed temporary sibling write.
    Maintain: JSON is human-readable and unknown values are retained by callers.
    Assert: temporary files are cleaned after success or failure.
    """
    path.parent.mkdir(parents=True, exist_ok=True)
    descriptor, raw_temp = tempfile.mkstemp(prefix=".hitch-", suffix=".tmp", dir=path.parent)
    temp = Path(raw_temp)
    try:
        with os.fdopen(descriptor, "w", encoding="utf-8") as file:
            json.dump(value, file, indent=2)
            file.flush()
            os.fsync(file.fileno())
        os.replace(temp, path)
    finally:
        if temp.exists():
            temp.unlink()


def _entries(meta: dict[str, Any]) -> list[Any]:
    """Extract metadata entries without trusting malformed values.

    Require: meta is an object.
    Guarantee: result is a new list.
    Maintain: unknown metadata remains in the caller's object.
    Assert: non-list entries become empty.
    """
    return list(meta.get("entries", [])) if isinstance(meta.get("entries"), list) else []


def _entry_id(entry: Any) -> Optional[str]:
    """Extract one profile entry identifier.

    Require: entry may be any decoded JSON value.
    Guarantee: only string ids are returned.
    Maintain: entry is not mutated.
    Assert: invalid entries are ignored.
    """
    return entry.get("id") if isinstance(entry, dict) and isinstance(entry.get("id"), str) else None


def _entry_ids(meta: dict[str, Any]) -> list[str]:
    """Return valid entry IDs in stored order.

    Require: meta is decoded JSON.
    Guarantee: output excludes malformed entries.
    Maintain: order is preserved.
    Assert: no profile file is read.
    """
    return [item for item in (_entry_id(entry) for entry in _entries(meta)) if item]


def _view(profile: dict[str, Any]) -> ProfileView:
    """Build a credential-safe status view from a profile object.

    Require: profile is a decoded JSON object.
    Guarantee: API key text is never returned.
    Maintain: model order is preserved.
    Assert: object model entries expose only their name.
    """
    raw_models = profile.get("inferenceModels") if isinstance(profile.get("inferenceModels"), list) else []
    names = [entry if isinstance(entry, str) else entry.get("name") for entry in raw_models if isinstance(entry, str) or isinstance(entry, dict) and isinstance(entry.get("name"), str)]
    key = profile.get("inferenceGatewayApiKey")
    return ProfileView(profile.get("inferenceProvider") if isinstance(profile.get("inferenceProvider"), str) else None, profile.get("inferenceGatewayBaseUrl") if isinstance(profile.get("inferenceGatewayBaseUrl"), str) else None, isinstance(key, str) and bool(key), names)


def _is_profile_id(value: Any) -> bool:
    """Check Claude's permissive UUID-shaped profile-id rule.

    Require: value may be any JSON value.
    Guarantee: only lowercase hexadecimal/hyphen 36-character ids pass.
    Maintain: no normalization occurs.
    Assert: malformed ids cannot select profile files through snapshots.
    """
    return isinstance(value, str) and len(value) == 36 and all(char in "0123456789abcdef-" for char in value)
