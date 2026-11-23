"""Normalize gateway URLs and check Anthropic-compatible endpoints.

| Stage | Mechanism | Tag | Ref |
| --- | --- | --- | --- |
| NORMALIZE | Convert user host input into a base URL, preserving custom API paths | [mandatory] | R3 |
| CHECK | Validate credential through /v1/models | [mandatory] | R3 |
| CATALOG | Prefer eligible Oxen agent models | [optional] | R3 |
| DISCOVER | Read LiteLLM's catalog and filter known non-chat groups | [mandatory] | L1 |

EARS guards: R3 IF catalog fails, THEN use the compatible model list response.
CLOSED: no live gateway is used by tests; callers inject an httpx client.
Preconditions: API keys are caller-held and base URLs are normalized.
Failure modes: transport errors return display-safe unreachable results.
"""

from __future__ import annotations

from dataclasses import dataclass
from typing import Any, Optional
from urllib.parse import urlsplit

import httpx

from .models import Model

DEFAULT_BASE_URL = "https://hub.oxen.ai/api/ai"
ANTHROPIC_VERSION = "2023-06-01"


@dataclass(frozen=True)
class Check:
    """Gateway check result passed to the UI bridge.

    Require: state is one supported outcome.
    Guarantee: key material is not stored.
    Maintain: ok results carry models only.
    Assert: messages are UI-safe.
    """

    state: str
    models: tuple[Model, ...] = ()
    message: Optional[str] = None


def normalize_base_url(value: str, *, custom: bool = False) -> str:
    """Turn a host or gateway URL into Claude's base gateway URL.

    Require: value is user-supplied text.
    Guarantee: bare local hosts use http and network hosts use https.
    Maintain: a custom nonempty path is preserved.
    Assert: invalid inputs raise a single user-safe ValueError.
    """
    refused = "Enter an endpoint URL like http://127.0.0.1:4000."
    text = value.strip().replace(chr(92), "/")
    if not text or any(char.isspace() for char in text) or "?" in text or "#" in text:
        raise ValueError(refused)
    scheme: Optional[str] = None
    rest = text
    if "://" in text:
        scheme, rest = text.split("://", 1)
        if scheme not in ("http", "https"):
            raise ValueError(refused)
    rest = rest.rstrip("/")
    explicit_version = rest.endswith("/v1")
    if explicit_version:
        rest = rest[:-3].rstrip("/")
    host, separator, path = rest.partition("/")
    local = _is_local(host)
    if not host or "@" in host or (scheme is None and not (local or "." in host or ":" in host)):
        raise ValueError(refused)
    suffix = separator + path if separator else ("" if custom or explicit_version else "/api/ai")
    normalized = f"{scheme or ('http' if local else 'https')}://{host}{suffix}"
    parsed = urlsplit(normalized)
    if not parsed.hostname or parsed.port == 0:
        raise ValueError(refused)
    return normalized


async def discover(base_url: str, api_key: str | None) -> list[Model]:
    """Read model groups from LiteLLM without making inference requests.

    Args:
        base_url: Normalized upstream address.
        api_key: Optional credential scoped to this upstream.
    Returns:
        Unique model groups in the order LiteLLM lists them.
    Raises:
        ValueError: Discovery failed or the result has no usable models.
    """
    headers = {"Authorization": f"Bearer {api_key}"} if api_key else {}
    async with httpx.AsyncClient(timeout=5, trust_env=False, headers=headers) as client:
        try:
            response = await client.get(f"{base_url}/v1/models")
            if response.status_code in (401, 403):
                raise ValueError("LiteLLM requires a valid provider API key.")
            response.raise_for_status()
        except httpx.HTTPError as error:
            raise ValueError(f"Could not read /v1/models at {base_url}. Check the LiteLLM URL and that it is running.") from error
        catalog = _parse_models(response.json())
        # This endpoint is optional and can require admin privileges on LiteLLM.
        metadata: dict[str, set[str | None]] = {}
        try:
            info = await client.get(f"{base_url}/model/info", timeout=2)
            values = info.json().get("data", []) if info.is_success else []
            for item in values:
                name = item.get("model_name")
                mode = (item.get("model_info") or {}).get("mode")
                metadata.setdefault(name, set()).add(mode)
        except (httpx.HTTPError, ValueError, TypeError, AttributeError):
            pass
    non_chat = {"embedding", "rerank", "image_generation", "audio_speech", "audio_transcription", "moderation"}
    usable = {
        item.id: item for item in catalog
        if item.id and not metadata.get(item.id, {None}).issubset(non_chat)
    }
    if not usable:
        raise ValueError("LiteLLM did not list any chat models.")
    return list(usable.values())


async def check(base_url: str, api_key: str, client: Optional[httpx.AsyncClient] = None) -> dict[str, Any]:
    """Check one API key and return suitable gateway models.

    Require: base_url is normalized and api_key is nonblank.
    Guarantee: 401/403 become invalid_key; eligible catalog wins when nonempty.
    Maintain: caller-owned client is never closed.
    Assert: transport failures return unreachable rather than raise.
    """
    owned = client is None
    request_client = client or httpx.AsyncClient(timeout=15)
    try:
        try:
            response = await request_client.get(f"{base_url}/v1/models?limit=1000", headers={"Authorization": f"Bearer {api_key}", "anthropic-version": ANTHROPIC_VERSION})
        except httpx.TimeoutException:
            return _check("unreachable", message=f"{_host(base_url)} took too long to answer.")
        except httpx.HTTPError:
            return _check("unreachable", message=f"Could not reach {_host(base_url)}. Check your internet connection.")
        if response.status_code in (401, 403):
            return _check("invalid_key")
        if response.status_code == 404:
            return _check("failed", message=f"{_host(base_url)} has no compatible model endpoint at this path. Check the URL.")
        if response.is_error:
            return _check("failed", message=f"The gateway answered with an error ({response.status_code}).")
        try:
            fallback = _parse_models(response.json())
        except ValueError:
            return _check("failed", message="The gateway's reply was not a model list. Check the gateway address.")
        preferred = await _catalog(base_url, api_key, request_client)
        return _check("ok", models=preferred or fallback)
    finally:
        if owned:
            await request_client.aclose()


async def test_message(base_url: str, api_key: str, model: str, client: Optional[httpx.AsyncClient] = None) -> str:
    """Send Claude's smallest streaming request and return the answering model.

    Require: base_url is normalized and credentials are caller-held.
    Guarantee: returns model only after nonempty generated text is observed.
    Maintain: caller-owned client remains open.
    Assert: failures expose a gateway detail without API-key material.
    """
    owned = client is None
    request_client = client or httpx.AsyncClient(timeout=15)
    try:
        try:
            response = await request_client.post(f"{base_url}/v1/messages", headers={"Authorization": f"Bearer {api_key}", "anthropic-version": ANTHROPIC_VERSION}, json={"model": model, "max_tokens": 1, "stream": True, "messages": [{"role": "user", "content": "Hi"}]})
        except httpx.HTTPError as error:
            raise ValueError(f"Could not send a test message: {error}") from error
        if response.is_success and _has_generated_text(response.text):
            return model
        if response.is_success:
            raise ValueError(f"A test message to {model} returned no generated text.")
        try:
            detail = response.json().get("error", {}).get("message")
        except ValueError:
            detail = None
        raise ValueError(f"A test message to {model} failed: {detail or f'status {response.status_code}'}")
    finally:
        if owned:
            await request_client.aclose()


async def _catalog(base_url: str, api_key: str, client: httpx.AsyncClient) -> list[Model]:
    """Fetch and filter Oxen's richer model catalog without surfacing failures.

    Require: client can send HTTP requests.
    Guarantee: any request/parse failure returns an empty list.
    Maintain: catalog fallback does not alter the primary check response.
    Assert: only eligible agent models are returned.
    """
    try:
        response = await client.get(f"{base_url}/models", headers={"Authorization": f"Bearer {api_key}"})
        data = response.json().get("data") if response.is_success else None
    except (httpx.HTTPError, ValueError):
        return []
    if not isinstance(data, list):
        return []
    return [model for model in (_agent_model(item) for item in data) if model is not None]


def _check(state: str, models: Optional[list[Model]] = None, message: Optional[str] = None) -> dict[str, Any]:
    """Render the bridge-compatible tagged check result.

    Require: state follows the established UI union.
    Guarantee: result is JSON-compatible and contains no key material.
    Maintain: only ok contains models; non-ok messages remain optional.
    Assert: models are copied into plain dictionaries.
    """
    result: dict[str, Any] = {"state": state}
    if state == "ok":
        result["models"] = [{"id": model.id, "name": model.name, "uuid": model.uuid, "developer": model.developer} for model in models or []]
    if message is not None:
        result["message"] = message
    return result


def _agent_model(item: Any) -> Optional[Model]:
    """Return one Oxen model eligible for Claude agent requests.

    Require: item is a decoded catalog object.
    Guarantee: text, tools, runnable, and summary requirements are enforced.
    Maintain: optional metadata remains optional.
    Assert: ineligible records return None.
    """
    if not isinstance(item, dict) or not isinstance(item.get("id"), str) or not item["id"]:
        return None
    capabilities = item.get("capabilities") if isinstance(item.get("capabilities"), dict) else {}
    if "text" not in capabilities.get("output", []) or capabilities.get("tools") is not True or item.get("can_run_inference") is False or not isinstance(item.get("summary"), str) or not item["summary"].strip():
        return None
    return Model(item["id"], _text(item.get("display_name"), item["id"]), _text(item.get("uuid")), _text((item.get("developer") or {}).get("name") if isinstance(item.get("developer"), dict) else None))


def _parse_models(body: Any) -> list[Model]:
    """Parse Anthropic or OpenAI compatible model-list response data.

    Require: body is decoded JSON.
    Guarantee: invalid shapes raise ValueError and malformed entries are skipped.
    Maintain: input ordering is preserved.
    Assert: no catalog fields are inferred.
    """
    values = body.get("data") if isinstance(body, dict) else body
    if not isinstance(values, list):
        raise ValueError("not a model list")
    return [Model(item["id"], _text(item.get("display_name"), item["id"])) for item in values if isinstance(item, dict) and isinstance(item.get("id"), str)]


def _has_generated_text(body: str) -> bool:
    """Require a nonempty text delta in an Anthropic SSE response.

    Require: body is the buffered response to a streaming message request.
    Guarantee: tool-only, headers-only, and empty streams do not count as replies.
    Maintain: only decoded text_delta content is considered user-visible output.
    Assert: malformed event payloads are ignored without raising.
    """
    for block in body.replace("\r\n", "\n").split("\n\n"):
        data = next((line[5:].strip() for line in block.splitlines() if line.startswith("data:")), None)
        if data is None or data == "[DONE]":
            continue
        try:
            event = httpx.Response(200, text=data).json()
        except ValueError:
            continue
        delta = event.get("delta") if isinstance(event, dict) else None
        if isinstance(delta, dict) and delta.get("type") == "text_delta" and isinstance(delta.get("text"), str) and delta["text"].strip():
            return True
    return False


def _text(value: Any, different_from: Optional[str] = None) -> Optional[str]:
    """Return nonempty text except a redundant display name.

    Require: value may be decoded JSON.
    Guarantee: only meaningful string values survive.
    Maintain: no normalization beyond empty rejection.
    Assert: caller controls semantic validation.
    """
    return value if isinstance(value, str) and value and value != different_from else None


def _is_local(host: str) -> bool:
    """Recognize hosts that default to plain HTTP.

    Require: host excludes URL path.
    Guarantee: localhost, 0.0.0.0, 127/8, and IPv6 loopback are local.
    Maintain: nonlocal hosts are false.
    Assert: no DNS lookup occurs.
    """
    return host.startswith("[::1]") or host.split(":", 1)[0] in ("localhost", "0.0.0.0") or host.split(":", 1)[0].startswith("127.")


def _host(base_url: str) -> str:
    """Extract the host for display-safe gateway messages.

    Require: base_url is a normalized URL.
    Guarantee: URL host is returned when present.
    Maintain: no URL rewrite occurs.
    Assert: malformed values return unchanged.
    """
    return base_url.split("://", 1)[-1].split("/", 1)[0]
