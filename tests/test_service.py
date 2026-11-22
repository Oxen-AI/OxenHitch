"""Verify service sequencing and secret-free UI status contracts.

| Stage | Mechanism | Tag | Ref |
| --- | --- | --- | --- |
| FIXTURE | Inject isolated paths, state, auth, and install | [contract] | R1-R4 |
| VALIDATE | Mock gateway outcomes before profile mutation | [mandatory] | R3 |
| VERIFY | Check status, pause, and restore behavior | [mandatory] | R1,R2 |
| ROUTE | Verify catalog discovery and alias forwarding at actual HTTP boundaries | [mandatory] | L1-L3 |

EARS guards: R3 WHEN gateway validation fails, THEN no profile or state is written.
CLOSED: no real Claude profile, process, registry, or gateway is accessed.
Preconditions: pytest supplies isolated paths and gateway calls are monkeypatched.
Failure modes: assertions identify service/config ordering mismatches.
"""

from __future__ import annotations

import json
import socket
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from threading import Thread

import httpx
import pytest

from hitch import gateway
from hitch.config import ClaudePaths
from hitch.models import Model, litellm_alias
from hitch.secrets import OxenConfig
from hitch.service import HitchService
from hitch.state import HitchState, StateStore
from hitch.windows import ClaudeInstall


def _service(root: Path) -> HitchService:
    """Build a service whose every file lives beneath one fixture directory.

    Require: root is provided by pytest temporary storage.
    Guarantee: no default user paths are used.
    Maintain: profile, state, and auth roots are isolated.
    Assert: service construction performs no write.
    """
    data = root / "Claude-3p"
    service = HitchService(ClaudePaths.at(data), StateStore(root / "state.json"), OxenConfig(root / "auth.toml"), ClaudeInstall(root / "AnthropicClaude"))
    service._sandbox = True
    return service


def test_failed_gateway_validation_writes_no_profile_or_state(tmp_path: Path, monkeypatch: pytest.MonkeyPatch) -> None:
    """Reject an invalid key before Claude configuration changes.

    Require: fake gateway returns invalid_key.
    Guarantee: profile library and state remain absent.
    Maintain: no auth token is persisted.
    Assert: user sees the existing plain-language key error.
    """
    async def failed_check(_url: str, _key: str) -> dict[str, object]:
        """Return the controlled invalid-key outcome.

        Require: gateway arguments are non-secret test values.
        Guarantee: no network request occurs.
        Maintain: result matches the gateway bridge shape.
        Assert: error state is explicit.
        """
        return {"state": "invalid_key"}

    monkeypatch.setattr(gateway, "check", failed_check)
    service = _service(tmp_path)
    with pytest.raises(ValueError, match="didn't accept"):
        service.connect("bad-key")
    assert not service.paths.library_dir.exists()
    assert not service.state.path.exists()
    assert not service.secrets.path.exists()


def test_connect_pause_and_remove_restore_prior_profile_and_mode(tmp_path: Path, monkeypatch: pytest.MonkeyPatch) -> None:
    """Validate profile ownership, pause, and prior state restoration.

    Require: gateway mock returns a valid mixed catalog.
    Guarantee: saved key is per-host and status contains only its hint.
    Maintain: prior Claude profile and mode survive removal.
    Assert: no plaintext key appears in Hitch's state file.
    """
    async def successful_check(_url: str, _key: str) -> dict[str, object]:
        """Return a deterministic catalog without using the network.

        Require: arguments are test-only.
        Guarantee: catalog includes Anthropic and another provider.
        Maintain: model order comes from the mock payload.
        Assert: outcome is successful.
        """
        return {"state": "ok", "models": [{"id": "claude-sonnet-5", "name": None, "uuid": None, "developer": "anthropic"}, {"id": "gpt-test", "name": "GPT Test", "uuid": None, "developer": "openai"}]}

    monkeypatch.setattr(gateway, "check", successful_check)
    service = _service(tmp_path)
    other = "c2898676-28c3-4ef2-90bb-6a10501a33c1"
    service.paths.library_dir.mkdir(parents=True)
    service.paths.meta_file.write_text(json.dumps({"appliedId": other, "entries": [{"id": other, "name": "Default"}]}), encoding="utf-8")
    service.paths.desktop_config.write_text(json.dumps({"deploymentMode": "1p", "kept": True}), encoding="utf-8")
    status = service.connect("test-secret-1234")
    assert status["link"] == "connected"
    assert status["keyHint"] == "1234"
    assert "test-secret" not in service.state.path.read_text(encoding="utf-8")
    paused = service.set_routing(False)
    assert paused["link"] == "paused"
    restored = service.remove()
    assert restored["link"] == "not_configured"
    assert json.loads(service.paths.meta_file.read_text(encoding="utf-8"))["appliedId"] == other
    assert json.loads(service.paths.desktop_config.read_text(encoding="utf-8")) == {"deploymentMode": "1p", "kept": True}
    assert service.secrets.get("hub.oxen.ai") == "test-secret-1234"


def test_custom_endpoint_save_is_offline_and_keeps_model_ids(tmp_path: Path, monkeypatch: pytest.MonkeyPatch) -> None:
    """Save custom Claude Messages settings without contacting the endpoint.

    Require: endpoint and model IDs are provided by the user.
    Guarantee: exact IDs are configured and no auth key or network check is required.
    Maintain: profile still uses Claude's gateway provider contract.
    Assert: state marks the endpoint custom and model discovery is disabled.
    """
    async def forbidden_check(_url: str, _key: str) -> dict[str, object]:
        raise AssertionError("custom Save must not contact the endpoint")

    monkeypatch.setattr(gateway, "check", forbidden_check)
    service = _service(tmp_path)
    status = service.connect(base_url="https://api.example.com/v1", custom_endpoint=True, model_ids=["gpt-test", "claude-sonnet-5"])
    profile_id = service.state.load().profile_id
    profile = json.loads(service.paths.profile_file(profile_id).read_text(encoding="utf-8"))
    assert status["customEndpoint"] is True
    assert status["keyHint"] is None
    assert profile["inferenceGatewayBaseUrl"] == "https://api.example.com"
    assert profile.get("modelDiscoveryEnabled") is not True
    assert [item["name"] if isinstance(item, dict) else item for item in profile["inferenceModels"]] == ["gpt-test", "claude-sonnet-5"]
    assert "claude-via-" not in json.dumps(profile["inferenceModels"])


def test_litellm_discovers_and_routes_stable_aliases(tmp_path: Path) -> None:
    """Verify discovery, restart, and authenticated alias requests against a fake LiteLLM.

    Require: fake upstream exposes one embedding group and three distinct chat IDs.
    Guarantee: only the chat aliases are saved, and their requests reach the real IDs.
    """
    model_ids = ["team/model-a", "vendor/モデル:4b", "party-c"]
    names = ["Team Model A", "モデル 4B", "party-c"]
    calls = []

    class LiteLLM(BaseHTTPRequestHandler):
        """Emulate the two catalog responses and echo model routing over HTTP."""

        def do_GET(self):
            """Return advertised model names and capability metadata."""
            if self.path == "/model/info":
                data = [{"model_name": "embed", "model_info": {"mode": "embedding"}}]
            else:
                data = [
                    {"id": model_ids[0], "display_name": names[0]},
                    {"id": model_ids[1], "display_name": names[1]},
                    {"id": model_ids[2]},
                    {"id": "embed"},
                ]
            self.send_response(200)
            self.end_headers()
            self.wfile.write(json.dumps({"data": data}).encode())

        def do_POST(self):
            """Capture the original ID and emit an Anthropic reply."""
            body = json.loads(self.rfile.read(int(self.headers["Content-Length"])))
            calls.append((self.path, body["model"], self.headers.get("Authorization")))
            self.send_response(200)
            if body.get("stream"):
                self.send_header("Content-Type", "text/event-stream")
                self.end_headers()
                self.wfile.write(b'data: {"delta":{"type":"text_delta","text":"hi"}}\n\n')
            else:
                self.end_headers()
                self.wfile.write(b'{"input_tokens":1}')

    upstream = ThreadingHTTPServer(("127.0.0.1", 0), LiteLLM)
    thread = Thread(target=upstream.serve_forever, daemon=True)
    thread.start()
    service = _service(tmp_path)
    try:
        url = f"http://127.0.0.1:{upstream.server_port}"
        status = service.connect(base_url=url + "/v1", custom_endpoint=True, model_ids=[])
        profile_id = service.state.load().profile_id
        file = service.paths.profile_file(profile_id)
        profile = json.loads(file.read_text(encoding="utf-8"))
        relay_token = service.secrets.get(service._relay_secret_id(profile_id))
        assert relay_token == profile["inferenceGatewayApiKey"]
        aliases = [litellm_alias(item) for item in model_ids]
        assert [entry["name"] for entry in profile["inferenceModels"]] == aliases
        assert [entry["labelOverride"] for entry in profile["inferenceModels"]] == names
        assert [entry["id"] for entry in status["modelCatalog"]] == model_ids
        assert status["keyHint"] is None
        relay_url = profile["inferenceGatewayBaseUrl"]
        headers = {"Authorization": "Bearer " + profile["inferenceGatewayApiKey"]}
        # Simulate the previous installed version's profile before reopening Hitch.
        for entry in profile["inferenceModels"]:
            entry["labelOverride"] = entry["name"]
        profile["inferenceGatewayApiKey"] = "__sealed__:v1:simulated-claude-encryption"
        file.write_text(json.dumps(profile), encoding="utf-8")
        service.close_relay()
        service = _service(tmp_path)
        service.restore_relay()
        repaired = json.loads(file.read_text(encoding="utf-8"))
        assert [entry["name"] for entry in repaired["inferenceModels"]] == aliases
        assert [entry["labelOverride"] for entry in repaired["inferenceModels"]] == names
        assert repaired["inferenceGatewayApiKey"] == "__sealed__:v1:simulated-claude-encryption"
        with httpx.Client(base_url=relay_url, timeout=2, trust_env=False) as client:
            assert client.get("/v1/models").status_code == 401
            assert client.get("/v1/models", headers={"Authorization": "Bearer " + repaired["inferenceGatewayApiKey"]}).status_code == 401
            headers = {"Authorization": "Bearer " + relay_token}
            listed = client.get("/v1/models", headers=headers).json()["data"]
            assert [item["id"] for item in listed] == aliases
            assert [item["display_name"] for item in listed] == names
            assert client.post("/v1/messages", headers=headers, json={"model": "unknown"}).status_code == 400
            stream = client.post("/v1/messages", headers=headers, json={"model": aliases[0], "stream": True})
            assert stream.headers["Content-Type"] == "text/event-stream"
            assert '"text":"hi"' in stream.text
            for alias in aliases[1:]:
                response = client.post("/v1/messages/count_tokens?beta=true", headers=headers, json={"model": alias})
                assert response.json() == {"input_tokens": 1}
        assert calls == [("/v1/messages", model_ids[0], None), *[
            ("/v1/messages/count_tokens?beta=true", item, None) for item in model_ids[1:]
        ]]
    finally:
        service.close_relay()
        upstream.shutdown()
        upstream.server_close()
        thread.join()


def test_restore_relay_replaces_unrecoverable_sealed_credential(tmp_path: Path) -> None:
    """Create a new usable token when an old Claude profile has only ciphertext.

    Require: saved relay state references a profile whose only credential is sealed.
    Guarantee: a fresh plaintext relay token is stored separately and written to Claude.
    Maintain: the ciphertext never authenticates against the restored relay.
    Assert: the saved token retrieves the catalog while the sealed token gets 401.
    """
    service = _service(tmp_path)
    profile_id = "ad4e7c71-b683-4a6b-823a-7f9d1d17f7ab"
    service.paths.library_dir.mkdir(parents=True)
    service.paths.profile_file(profile_id).write_text(json.dumps({
        "inferenceGatewayApiKey": "__sealed__:v1:unavailable-plaintext",
        "inferenceGatewayBaseUrl": "http://127.0.0.1:4000",
        "inferenceModels": [],
    }), encoding="utf-8")
    with socket.socket() as probe:
        probe.bind(("127.0.0.1", 0))
        port = probe.getsockname()[1]
    service.state.save(HitchState(
        profile_id=profile_id,
        base_url="http://127.0.0.1:4000",
        catalog=[Model("test-model", "Test model")],
        relay_port=port,
        custom_endpoint=True,
    ))
    try:
        service.restore_relay()
        token = service.secrets.get(service._relay_secret_id(profile_id))
        profile = json.loads(service.paths.profile_file(profile_id).read_text(encoding="utf-8"))
        assert token
        assert profile["inferenceGatewayApiKey"] == token
        with httpx.Client(base_url=f"http://127.0.0.1:{port}", timeout=2, trust_env=False) as client:
            assert client.get("/v1/models", headers={"x-api-key": token}).status_code == 200
            assert client.get("/v1/models", headers={"x-api-key": "__sealed__:v1:unavailable-plaintext"}).status_code == 401
    finally:
        service.close_relay()


def test_status_is_read_only_and_reports_external_drift(tmp_path: Path) -> None:
    """Report external profile changes without rewriting Claude files.

    Require: state records an owned profile with a saved host key.
    Guarantee: wrong applied id reports drifted.
    Maintain: status does not mutate state or config files.
    Assert: key hint is present without credential text.
    """
    service = _service(tmp_path)
    service.secrets.set("hub.oxen.ai", "saved-key-7890")
    state = service.state.load()
    state.profile_id = "c2898676-28c3-4ef2-90bb-6a10501a33c2"
    service.state.save(state)
    service.paths.library_dir.mkdir(parents=True)
    service.paths.meta_file.write_text(json.dumps({"appliedId": "c2898676-28c3-4ef2-90bb-6a10501a33c1", "entries": []}), encoding="utf-8")
    before = service.state.path.read_bytes()
    status = service.status()
    assert status["link"] == "drifted"
    assert status["keyHint"] == "7890"
    assert service.state.path.read_bytes() == before
