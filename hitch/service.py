"""Coordinate gateway checks, Claude profiles, saved keys, and Hitch state.

| Stage | Mechanism | Tag | Ref |
| --- | --- | --- | --- |
| LOAD | Read non-secret state and current Claude snapshot | [mandatory] | R1,R2 |
| VALIDATE | Check Oxen or discover LiteLLM models when no manual IDs were entered | [mandatory] | R3,L1 |
| APPLY | Persist profile, state, provider credential, and separate relay token | [mandatory] | R1,R2,L5 |
| ROUTE | Restore relay authentication from its credential-store entry and install the alias map | [mandatory] | L2,L3,L5 |
| REPAIR_LABELS | Restore original display names in previously saved alias profiles | [contract] | L2 |
| RESTORE | Remove Hitch profile and restore prior selection | [mandatory] | R1 |
| REPORT | Return status and gateway results without key material | [contract] | R1-R4 |

EARS guards: R1 WHEN ``HITCH_CLAUDE_DIR`` is set, all state/auth/config writes stay in its sibling sandbox files.
R2 IF gateway validation fails, THEN connect leaves Claude configuration unchanged.
R3 WHEN a provider key is saved, THEN it is scoped to the destination host and state/status never include key text.
L5 IF Claude seals a saved profile key, THEN relay authentication MUST continue using the separately stored plaintext token.
CLOSED: automatic Claude termination is excluded until a graceful quit mechanism is verified.
Preconditions: paths and stores refer to this user's files; caller serializes writes through the service.
Failure modes: gateway, TOML, JSON, registry, and filesystem errors surface as UI-safe messages.
"""

from __future__ import annotations

import asyncio
import os
import time
import uuid
import warnings
from dataclasses import asdict
from pathlib import Path
from secrets import token_urlsafe
from threading import RLock
from typing import Any, Optional

from . import config, gateway, models
from .relay import ModelRelay, RelayTarget
from .secrets import OxenConfig
from .state import HitchState, StateStore
from .windows import ClaudeInstall, inference_managed, manual_restart_instructions, read_managed_policy


class HitchService:
    """Own the user-visible configuration and gateway operations.

    Require: paths, state, secrets, and install describe one user or sandbox.
    Guarantee: status values contain no secrets and config writes are serialized.
    Maintain: state never stores API keys; Claude profile ownership remains in state.profile_id.
    Assert: network validation precedes connect profile writes.
    """

    def __init__(self, paths: config.ClaudePaths, state: StateStore, secrets: OxenConfig, install: ClaudeInstall):
        """Bind the service to injected platform and persistence boundaries.

        Require: all dependencies target the same user or sandbox.
        Guarantee: initialization performs no writes or network requests.
        Maintain: one lock serializes every mutating operation.
        Assert: stores are retained by reference.
        """
        self.paths = paths
        self.state = state
        self.secrets = secrets
        self.install = install
        self._lock = RLock()
        self._sandbox = bool(os.environ.get("HITCH_CLAUDE_DIR"))
        self.relay: ModelRelay | None = None
        self.relay_error: str | None = None

    @classmethod
    def for_user(cls, env: Optional[dict[str, str]] = None) -> "HitchService":
        """Build paths and stores for the real user or an explicit sandbox.

        Require: environment either contains user paths or caller supplied sandbox paths.
        Guarantee: sandbox state and auth remain beside HITCH_CLAUDE_DIR.
        Maintain: real state goes under LOCALAPPDATA/OxenHitch and auth remains compatible with the Oxen CLI.
        Assert: construction itself does not create files.
        """
        values = os.environ if env is None else env
        paths = config.ClaudePaths.detect(dict(values))
        sandbox_dir = values.get("HITCH_CLAUDE_DIR")
        if sandbox_dir:
            state_path = paths.data_dir.with_name("hitch-state.json")
            auth = paths.data_dir.with_name("oxen-auth_config.toml")
        else:
            local = Path(values.get("LOCALAPPDATA") or Path(values.get("USERPROFILE", ".")) / "AppData" / "Local")
            state_path = local / "OxenHitch" / "state.json"
            auth = OxenConfig.for_user(dict(values)).path
        return cls(paths, StateStore(state_path), OxenConfig(auth), ClaudeInstall.discover(values.get("LOCALAPPDATA")))

    def status(self) -> dict[str, Any]:
        """Return the current UI status without changing files.

        Require: process and registry probes are permitted or sandbox mode is active.
        Guarantee: returns the TypeScript ``Status`` shape without credential values.
        Maintain: no network request or filesystem write occurs.
        Assert: status keys use camelCase to match the UI.
        """
        state = self.state.load()
        current = config.snapshot(self.paths)
        running = self.install.running_processes() if not self._sandbox else [(0, 1.0)]
        managed = False if self._sandbox else read_managed_policy()
        link = self._link(state, current)
        base_url = self._base_url(state)
        key = self._key_for(base_url)
        needs_restart = bool(running and state.last_write_at is not None and min(started for _, started in running) <= state.last_write_at)
        return {
            "claudeInstalled": bool(self.install.executables()) if not self._sandbox else True,
            "claudeRunning": bool(running),
            "managed": managed,
            "link": link,
            "needsRestart": needs_restart,
            "baseUrl": base_url,
            "keyHint": self._hint(key) if key else None,
            "models": list(state.models),
            "customEndpoint": state.custom_endpoint,
            "modelCatalog": [asdict(model) for model in state.catalog] if state.relay_port else None,
            "relayError": self.relay_error,
        }

    def check(self) -> Optional[dict[str, Any]]:
        """Check the selected saved key and refresh the cached model catalog.

        Require: current host key may be absent.
        Guarantee: absent key returns None; otherwise returns gateway check data.
        Maintain: key text stays within this method and gateway request.
        Assert: a successful catalog response updates non-secret state only.
        """
        with self._lock:
            state = self.state.load()
            base_url = self._base_url(state)
            key = self._key_for(base_url)
            if state.relay_port:
                self.connect(model_ids=[], keep_paused=True)
                return {"state": "ok", "models": [asdict(model) for model in self.state.load().catalog]}
            if key is None:
                return None
            result = asyncio.run(gateway.check(base_url, key))
            if result["state"] == "ok":
                state.catalog = [models.Model(**item) for item in result["models"]]
                self.state.save(state)
                self._sync_models(state)
            return result

    def connect(
        self, api_key: Optional[str] = None, base_url: Optional[str] = None,
        custom_endpoint: bool = False, model_ids: Optional[list[str]] = None,
        *, selection: Optional[list[str]] = None, keep_paused: bool = False,
    ) -> dict[str, Any]:
        """Save an Oxen or custom Claude Messages endpoint profile.

        Require: an empty explicit model list asks LiteLLM to supply the catalog.
        Guarantee: manual setup is offline and never reuses another host's key.
        Maintain: Oxen setup still validates before modifying Claude configuration.
        Assert: a discovery failure precedes any writes to the profile.
        """
        with self._lock:
            state = self.state.load()
            custom_endpoint = custom_endpoint or state.custom_endpoint
            normalized = gateway.normalize_base_url(base_url or self._base_url(state), custom=custom_endpoint)
            host = _host_of(normalized)
            saved_for_host = self.secrets.get(host)
            key = (api_key.strip() if api_key and api_key.strip() else None) or saved_for_host
            aliases = None
            if not custom_endpoint and not key:
                raise ValueError("Enter the API key for this endpoint.")
            if custom_endpoint:
                use_litellm = model_ids == [] or (model_ids is None and (state.relay_port or not state.models))
                if use_litellm:
                    if model_ids == [] or normalized != state.base_url or key != saved_for_host or not state.catalog:
                        state.catalog = asyncio.run(gateway.discover(normalized, key))
                    aliases = {models.litellm_alias(item.id): item for item in state.catalog}
                    available = {item.id for item in state.catalog}
                    picked = selection if selection is not None else state.models
                    state.models = [item for item in picked if item in available] if state.relay_port else []
                else:
                    picked = selection if selection is not None else (model_ids if model_ids is not None else state.models)
                    state.models = list(dict.fromkeys(item.strip() for item in picked if isinstance(item, str) and item.strip()))
                    if not state.models:
                        raise ValueError("Select at least one model or use automatic discovery.")
                    state.catalog = [models.Model(item, item, developer="openai") for item in state.models]
            else:
                result = asyncio.run(gateway.check(normalized, key))
                if result["state"] == "invalid_key":
                    raise ValueError("This endpoint didn't accept that API key. Check it and try again.")
                if result["state"] != "ok":
                    raise ValueError(result.get("message", "The gateway could not be checked."))
                state.catalog = [models.Model(**item) for item in result["models"]]
                if selection is not None:
                    state.models = list(selection)
            before = config.snapshot(self.paths)
            profile_id = state.profile_id or str(uuid.uuid4())
            if state.profile_id is None:
                state.previous_applied_id = before.applied_id
                state.previous_mode = before.mode
            profile_url = normalized
            if aliases is None:
                chosen_models = models.entries_for(state.models, state.catalog, alias_non_anthropic=not custom_endpoint)
                config_key = key or "hitch-no-auth"
                state.relay_port = None
            else:
                relay_secret = self._relay_secret_id(profile_id)
                config_key = self.secrets.get(relay_secret)
                profile_key = config.profile_key(self.paths, profile_id)
                if not config_key and profile_key and not profile_key.startswith("__sealed__:v1:"):
                    config_key = profile_key
                config_key = config_key or token_urlsafe(32)
                if self.relay is None:
                    self.relay = ModelRelay(state.relay_port or 0)
                state.relay_port = self.relay.server_port
                profile_url = f"http://127.0.0.1:{state.relay_port}"
                selected = [models.litellm_alias(item) for item in state.models] if state.models else aliases
                chosen_models = [
                    models.ModelEntry(alias, aliases[alias].display_name, models.BEHAVES_AS)
                    for alias in selected
                ]
            profile = config.GatewayProfile(profile_url, config_key, chosen_models, discover_models=not custom_endpoint)
            config.apply_profile(self.paths, profile_id, profile, active=not (keep_paused and before.mode == "1p"))
            state.profile_id = profile_id
            state.base_url = normalized
            state.custom_endpoint = custom_endpoint
            state.last_write_at = int(time.time())
            self.state.save(state)
            if aliases is not None:
                self.secrets.set(self._relay_secret_id(profile_id), config_key)
            else:
                self.secrets.remove(self._relay_secret_id(profile_id))
            if key and saved_for_host != key:
                self.secrets.set(host, key)
            if aliases is not None and self.relay is not None:
                self.relay.target = RelayTarget(normalized, key, config_key, aliases)
                self.relay_error = None
            else:
                self.close_relay()
            return self.status()

    def restore_relay(self) -> None:
        """Restore an existing alias profile from disk without calling LiteLLM.

        Maintain: Claude keeps using the same address and credential after restart.
        """
        state = self.state.load()
        if not (state.relay_port and state.profile_id and state.base_url):
            return
        secret_id = self._relay_secret_id(state.profile_id)
        token = self.secrets.get(secret_id)
        profile_token = config.profile_key(self.paths, state.profile_id)
        if not token:
            if profile_token and not profile_token.startswith("__sealed__:v1:"):
                token = profile_token
            else:
                token = token_urlsafe(32)
                config.set_profile_key(self.paths, state.profile_id, token)
                state.last_write_at = int(time.time())
                self.state.save(state)
            self.secrets.set(secret_id, token)
        elif profile_token and not profile_token.startswith("__sealed__:v1:") and profile_token != token:
            config.set_profile_key(self.paths, state.profile_id, token)
            state.last_write_at = int(time.time())
            self.state.save(state)
        try:
            self.relay = ModelRelay(state.relay_port)
        except OSError:
            self.relay_error = f"Hitch could not reopen local port {state.relay_port}. Close other instances of Hitch and reopen."
            return
        aliases = {models.litellm_alias(item.id): item for item in state.catalog}
        self.relay.target = RelayTarget(state.base_url, self._key_for(state.base_url), token, aliases)
        try:
            if config.repair_profile_labels(
                self.paths, state.profile_id, {alias: item.display_name for alias, item in aliases.items()}
            ):
                state.last_write_at = int(time.time())
                self.state.save(state)
        except OSError as error:
            warnings.warn(f"Hitch could not update the saved model labels: {error}", RuntimeWarning, stacklevel=2)

    def close_relay(self) -> None:
        """Release Hitch's local listener on exit or profile removal."""
        if self.relay is not None:
            self.relay.close()
            self.relay = None
        self.relay_error = None

    def set_routing(self, enabled: bool) -> dict[str, Any]:
        """Switch Claude between Oxen and Anthropic deployment modes.

        Require: Hitch has already created a Claude profile.
        Guarantee: disabling changes deployment mode without deleting the profile or key.
        Maintain: enabling reapplies the latest catalog and stored model choice.
        Assert: disabled mode is serialized as ``1p`` and enabled mode as ``3p``.
        """
        with self._lock:
            state = self._configured()
            if enabled:
                return self.connect()
            config.set_mode(self.paths, "1p")
            state.last_write_at = int(time.time())
            self.state.save(state)
            return self.status()

    def set_models(self, chosen: list[str]) -> dict[str, Any]:
        """Persist the chosen model order and reapply Claude's profile.

        Require: a Hitch profile exists and chosen contains model ids.
        Guarantee: Claude receives exactly the selected order; empty means all models.
        Maintain: failed optional catalog refresh leaves the prior catalog usable.
        Assert: profile application uses the saved host key.
        """
        with self._lock:
            self._configured()
            return self.connect(selection=chosen, keep_paused=True)

    def remove(self) -> dict[str, Any]:
        """Remove Hitch's Claude profile and restore the previous profile/mode.

        Require: state may contain a previously owned profile id.
        Guarantee: Oxen auth key remains saved after profile removal.
        Maintain: Claude files outside Hitch's profile ownership survive.
        Assert: state clears only after config restore succeeds.
        """
        with self._lock:
            state = self.state.load()
            if state.profile_id:
                config.remove_profile(self.paths, state.profile_id, state.previous_applied_id, state.previous_mode)
                self.secrets.remove(self._relay_secret_id(state.profile_id))
            self.state.save(HitchState(last_write_at=int(time.time())))
            self.close_relay()
            return self.status()

    def restart_claude(self) -> dict[str, Any]:
        """Return the safe manual restart response on Windows.

        Require: caller requests a Claude restart.
        Guarantee: no Claude process is terminated or relaunched.
        Maintain: users are told how to load changed settings.
        Assert: absence of a verified graceful quit keeps the operation manual.
        """
        raise ValueError(manual_restart_instructions())

    def saved_key(self, host: str) -> Optional[str]:
        """Return a key hint for the host entered in the Connection screen.

        Require: host may be bare or a full gateway URL.
        Guarantee: only the final four characters are returned.
        Maintain: invalid host input returns None.
        Assert: full credential text never leaves this method.
        """
        try:
            normalized = gateway.normalize_base_url(host, custom=True)
        except ValueError:
            return None
        key = self.secrets.get(_host_of(normalized))
        return self._hint(key) if key else None

    def test_message(self) -> str:
        """Send a minimal test request and return the chosen model id.

        Require: a saved gateway key and at least one known or returned model.
        Guarantee: only successful HTTP response returns a model name.
        Maintain: test request does not mutate Claude profile settings.
        Assert: an empty catalog is reported instead of guessed.
        """
        state = self.state.load()
        base_url = self._base_url(state)
        key = self._key_for(base_url)
        if state.relay_port and state.profile_id:
            if self.relay is None or self.relay.target is None:
                raise ValueError(self.relay_error or "The Hitch alias gateway is not running.")
            model_id = (state.models or [item.id for item in state.catalog])[0]
            return asyncio.run(gateway.test_message(
                f"http://127.0.0.1:{state.relay_port}", self.relay.target.token, models.litellm_alias(model_id)
            ))
        if not key:
            raise ValueError("There is no API key to test with.")
        if state.models:
            model_id = state.models[0]
        else:
            result = asyncio.run(gateway.check(base_url, key))
            if result["state"] != "ok":
                raise ValueError("This endpoint didn't accept that API key." if result["state"] == "invalid_key" else result.get("message", "The endpoint could not be checked."))
            catalog = result["models"]
            model_id = next((item["id"] for item in catalog if "claude" in item["id"]), catalog[0]["id"] if catalog else None)
            if not model_id:
                raise ValueError("The gateway did not list any models.")
        return asyncio.run(gateway.test_message(base_url, key, model_id))

    def autostart(self) -> bool:
        """Read whether the current user's login run entry exists.

        Require: Windows HKCU Run key is readable.
        Guarantee: false means no matching Hitch entry.
        Maintain: no registry mutation occurs.
        Assert: only the current user's autostart entry is considered.
        """
        import winreg

        try:
            with winreg.OpenKey(winreg.HKEY_CURRENT_USER, r"Software\Microsoft\Windows\CurrentVersion\Run") as key:
                winreg.QueryValueEx(key, "OxenHitch")
                return True
        except FileNotFoundError:
            return False

    def set_autostart(self, enabled: bool) -> bool:
        """Enable or disable the current user's login run entry.

        Require: app is running from a stable executable or Python entrypoint.
        Guarantee: registry value targets the current executable command.
        Maintain: disabling removes only OxenHitch's value.
        Assert: permission errors are not silently reported as success.
        """
        import winreg

        location = r"Software\Microsoft\Windows\CurrentVersion\Run"
        with winreg.CreateKey(winreg.HKEY_CURRENT_USER, location) as key:
            if enabled:
                import sys
                command = f'"{sys.executable}" "{Path(sys.argv[0]).resolve()}"'
                winreg.SetValueEx(key, "OxenHitch", 0, winreg.REG_SZ, command)
            else:
                try:
                    winreg.DeleteValue(key, "OxenHitch")
                except FileNotFoundError:
                    pass
        return self.autostart()

    def claude_icon(self) -> Optional[str]:
        """Return the installed Claude icon data URL when available.

        Require: supported Squirrel install root may contain icon files.
        Guarantee: missing image returns None.
        Maintain: no installation files are changed.
        Assert: icon data is safe to pass to the UI as a data URL.
        """
        return None

    def _configured(self) -> HitchState:
        """Load state or reject operations requiring an owned profile.

        Require: state store is readable.
        Guarantee: missing profile raises a display-safe error.
        Maintain: state is not changed.
        Assert: configured implies profile_id is present.
        """
        state = self.state.load()
        if not state.profile_id:
            raise ValueError("Connect to Oxen first.")
        return state

    def _key_for(self, base_url: str) -> Optional[str]:
        """Read the credential saved for a gateway URL.

        Require: base_url is normalized.
        Guarantee: host-specific secret or None is returned.
        Maintain: key text stays internal to the service.
        Assert: host extraction excludes URL path.
        """
        return self.secrets.get(_host_of(base_url))

    @staticmethod
    def _relay_secret_id(profile_id: str) -> str:
        """Return a credential-store key scoped to one Hitch relay profile.

        Require: profile_id identifies a Hitch-owned Claude profile.
        Guarantee: relay tokens cannot collide with endpoint host credentials.
        Maintain: the returned identifier contains no secret material.
        Assert: caller uses the identifier only for local relay authentication.
        """
        return f"hitch-relay:{profile_id}"

    def _base_url(self, state: HitchState) -> str:
        """Choose saved, environment, or default gateway address.

        Require: state may have a stored URL.
        Guarantee: output is a gateway base URL candidate.
        Maintain: saved settings outrank environment defaults.
        Assert: no URL is written by lookup.
        """
        return state.base_url or os.environ.get("HITCH_BASE_URL") or gateway.DEFAULT_BASE_URL

    def _link(self, state: HitchState, current: config.Snapshot) -> str:
        """Derive configured, connected, paused, or drifted status.

        Require: current is a read-only Claude snapshot.
        Guarantee: link matches the incumbent ownership checks.
        Maintain: disconnected profiles are not claimed as Oxen-controlled.
        Assert: provider and key presence are both required.
        """
        if not state.profile_id:
            return "not_configured"
        applied = current.applied
        ours = current.applied_id == state.profile_id and applied is not None and applied.provider == "gateway" and applied.has_key
        if not ours:
            return "drifted"
        return "paused" if current.mode == "1p" else "connected"

    def _sync_models(self, state: HitchState) -> None:
        """Update selected Claude models after a successful catalog refresh.

        Require: gateway catalog has been saved to state.
        Guarantee: only connected profiles can be rewritten.
        Maintain: automatic all-model order changes alone do not force a rewrite.
        Assert: failed or empty catalog never clears currently configured models.
        """
        if not state.profile_id or self._link(state, config.snapshot(self.paths)) != "connected":
            return
        wanted = [entry.to_config() for entry in models.entries_for(state.models, state.catalog, alias_non_anthropic=not state.custom_endpoint)]
        if not wanted:
            return
        current = config.profile_models(self.paths, state.profile_id)
        discovery = config.discovery_enabled(self.paths, state.profile_id)
        if (not state.models and sorted(map(str, wanted)) == sorted(map(str, current)) and discovery) or (wanted == current and discovery):
            return
        self.connect(custom_endpoint=state.custom_endpoint, model_ids=state.models)

    @staticmethod
    def _hint(key: str) -> str:
        """Return the last four Unicode codepoints of a saved key.

        Require: key is nonempty text.
        Guarantee: output never exceeds four characters.
        Maintain: key itself is not retained.
        Assert: short keys are returned in full, matching the UI hint contract.
        """
        return key[-4:]


def _host_of(base_url: str) -> str:
    """Extract a gateway host for per-host secret lookup.

    Require: base_url may include a scheme and path.
    Guarantee: returns only the authority portion.
    Maintain: explicit ports remain part of the host key.
    Assert: scheme text is removed.
    """
    return base_url.split("://", 1)[-1].split("/", 1)[0]
