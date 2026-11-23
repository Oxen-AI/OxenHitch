"""Fixture-only tests for Oxen credentials and gateway contracts.

| Stage | Mechanism | Tag | Ref |
| --- | --- | --- | --- |
| FIXTURE | Create temporary TOML and mock HTTP transport | [contract] | R2,R3 |
| VERIFY | Exercise credentials and gateway result shapes | [mandatory] | R2,R3 |

EARS guards: R2/R3 tests never call a live host or use a user credential.
CLOSED: no service, bridge, tray, or Claude profile is tested here.
Preconditions: pytest supplies temporary directories. Failure modes: assertions name contract drift.
"""

import asyncio
from pathlib import Path

import httpx
import pytest

from hitch.gateway import check, normalize_base_url, test_message as send_test_message
from hitch.secrets import OxenConfig


def test_secret_update_preserves_comments_and_other_hosts(tmp_path: Path) -> None:
    """Verify one host update preserves TOML comments and peers.

    Require: temporary auth file.
    Guarantee: only target token changes.
    Maintain: comments and default host survive.
    Assert: no duplicate host table is introduced.
    """
    path = tmp_path / "auth_config.toml"
    path.write_text('# keep\ndefault_host = "hub.oxen.ai"\n\n[[host_configs]]\nhost = "hub.oxen.ai"\nauth_token = "old"\n\n[[host_configs]]\nhost = "localhost:3001"\nauth_token = "local"\n', encoding="utf-8")
    store = OxenConfig(path)
    store.set("hub.oxen.ai", "new")
    assert store.get("hub.oxen.ai") == "new"
    assert store.get("localhost:3001") == "local"
    assert path.read_text(encoding="utf-8").count("[[host_configs]]") == 2
    assert "# keep" in path.read_text(encoding="utf-8")


def test_secret_create_and_malformed_refusal(tmp_path: Path) -> None:
    """Verify fresh creation and malformed-file non-overwrite.

    Require: temporary root.
    Guarantee: a fresh file has default host and malformed bytes survive refusal.
    Maintain: get returns None for malformed TOML.
    Assert: mutation never overwrites undecodable input.
    """
    path = tmp_path / "nested" / "auth_config.toml"
    store = OxenConfig(path)
    store.set("dev.hub.oxen.ai", "token")
    assert store.get("dev.hub.oxen.ai") == "token"
    assert 'default_host = "hub.oxen.ai"' in path.read_text(encoding="utf-8")
    path.write_text("not = [toml", encoding="utf-8")
    assert store.get("dev.hub.oxen.ai") is None
    with pytest.raises(ValueError):
        store.set("dev.hub.oxen.ai", "replacement")
    assert path.read_text(encoding="utf-8") == "not = [toml"


def test_normalization_varied_hosts() -> None:
    """Verify local, public, custom-path, and invalid host behavior.

    Require: no network.
    Guarantee: scheme and gateway path follow Rust parity rules.
    Maintain: custom proxy path remains intact.
    Assert: invalid input raises a safe error.
    """
    assert normalize_base_url(" hub.oxen.ai ") == "https://hub.oxen.ai/api/ai"
    assert normalize_base_url("localhost:3001") == "http://localhost:3001/api/ai"
    assert normalize_base_url("https://proxy.example.com/oxen/v1/") == "https://proxy.example.com/oxen"
    with pytest.raises(ValueError):
        normalize_base_url("ftp://nope.example")


def test_check_prefers_filtered_catalog_and_result_shape() -> None:
    """Verify primary check, eligible catalog filtering, and JSON result shape.

    Require: mock transport only.
    Guarantee: eligible Oxen catalog replaces compatible fallback list.
    Maintain: authorization headers remain scoped to calls.
    Assert: non-agent catalog entries are excluded.
    """
    def handler(request: httpx.Request) -> httpx.Response:
        if request.url.path.endswith("/v1/models"):
            return httpx.Response(200, json={"data": [{"id": "fallback"}]})
        return httpx.Response(200, json={"data": [{"id": "gpt", "display_name": "GPT", "uuid": "u", "developer": {"name": "openai"}, "summary": "works", "capabilities": {"output": ["text"], "tools": True}}, {"id": "image", "summary": "no", "capabilities": {"output": ["image"], "tools": True}}]})
    client = httpx.AsyncClient(transport=httpx.MockTransport(handler))
    try:
        result = asyncio.run(check("https://fixture/api/ai", "key", client))
    finally:
        asyncio.run(client.aclose())
    assert result == {"state": "ok", "models": [{"id": "gpt", "name": "GPT", "uuid": "u", "developer": "openai"}]}


def test_check_failures_and_message_probe() -> None:
    """Verify invalid key, wrong endpoint, and generated text validation.

    Require: mock transport only.
    Guarantee: UI receives expected tagged failure shapes and detail.
    Maintain: no request uses an external network.
    Assert: only an SSE text delta counts as a successful probe.
    """
    invalid = httpx.AsyncClient(transport=httpx.MockTransport(lambda _: httpx.Response(401)))
    wrong = httpx.AsyncClient(transport=httpx.MockTransport(lambda _: httpx.Response(404)))
    no_text = httpx.AsyncClient(transport=httpx.MockTransport(lambda _: httpx.Response(200)))
    good = httpx.AsyncClient(transport=httpx.MockTransport(lambda _: httpx.Response(200, text='event: content_block_delta\ndata: {"type":"content_block_delta","delta":{"type":"text_delta","text":"h"}}\n\n')))
    try:
        assert asyncio.run(check("https://fixture/api/ai", "bad", invalid)) == {"state": "invalid_key"}
        assert asyncio.run(check("https://fixture/api/ai", "key", wrong))["state"] == "failed"
        with pytest.raises(ValueError, match="no generated text"):
            asyncio.run(send_test_message("https://fixture/api/ai", "key", "m", no_text))
        assert asyncio.run(send_test_message("https://fixture/api/ai", "key", "m", good)) == "m"
    finally:
        asyncio.run(invalid.aclose()); asyncio.run(wrong.aclose()); asyncio.run(no_text.aclose()); asyncio.run(good.aclose())
