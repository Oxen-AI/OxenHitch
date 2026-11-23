"""Verify pywebview bridge delegation and JSON-safe status events.

| Stage | Mechanism | Tag | Ref |
| --- | --- | --- | --- |
| FIXTURE | Inject fake service and page window | [contract] | R1 |
| DELEGATE | Route bridge methods to service | [mandatory] | R1 |
| PUBLISH | Dispatch status as a page CustomEvent | [mandatory] | R1 |

EARS guards: R1 WHEN bridge mutation succeeds, THEN publish its returned status.
CLOSED: tests do not create a native window or access user files.
Preconditions: fake service returns JSON-compatible status dictionaries.
Failure modes: assertion failures identify bridge contract drift.
"""

from __future__ import annotations

import json
from typing import Any

from hitch.bridge import HitchBridge


class FakeWindow:
    """Capture JavaScript expressions emitted by the bridge.

    Require: no native window is required.
    Guarantee: evaluate_js payloads are observable in the test.
    Maintain: calls remain ordered.
    Assert: no JavaScript is executed.
    """

    def __init__(self) -> None:
        """Initialize an empty JavaScript capture list.

        Require: none.
        Guarantee: calls starts empty.
        Maintain: test owns the captured list.
        Assert: no external runtime is created.
        """
        self.calls: list[str] = []

    def evaluate_js(self, expression: str) -> None:
        """Record a bridge event expression.

        Require: expression is text.
        Guarantee: exact expression is appended.
        Maintain: no side effect outside the fixture.
        Assert: method never executes JavaScript.
        """
        self.calls.append(expression)


class FakeService:
    """Implement the service operations used by the adapter test.

    Require: status values are safe serialized dictionaries.
    Guarantee: connect returns an observable changed status.
    Maintain: no persistence or network call occurs.
    Assert: test credentials are not returned.
    """

    def status(self) -> dict[str, Any]:
        """Return a minimal status result.

        Require: none.
        Guarantee: status follows the UI contract's key names.
        Maintain: result includes no key text.
        Assert: link is a valid UI enum.
        """
        return {"link": "not_configured", "keyHint": None}

    def connect(self, api_key: str | None = None, base_url: str | None = None, custom_endpoint: bool = False, model_ids: list[str] | None = None) -> dict[str, Any]:
        """Return a connected status after recording no secret.

        Require: caller values are test-only.
        Guarantee: result contains a redacted hint only.
        Maintain: fake service remains stateless.
        Assert: no credential content is copied.
        """
        return {"link": "connected", "keyHint": "1234"}


def test_bridge_delegates_and_publishes_only_redacted_status() -> None:
    """Check connect result and event contain no supplied credential.

    Require: fake service and fake page window.
    Guarantee: bridge returns its service result and publishes event.
    Maintain: event payload is valid JSON.
    Assert: plaintext token is absent from returned and emitted values.
    """
    bridge = HitchBridge(FakeService())  # type: ignore[arg-type]
    window = FakeWindow()
    bridge.bind_window(window)
    status = bridge.connect("secret-not-for-js", "https://gateway.example/api/ai")
    assert status == {"link": "connected", "keyHint": "1234"}
    assert len(window.calls) == 1
    event = window.calls[0]
    encoded = event.partition("detail:")[2].removesuffix("}))")
    assert json.loads(encoded) == status
    assert "secret-not-for-js" not in event
