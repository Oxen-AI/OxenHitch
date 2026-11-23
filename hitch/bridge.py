"""Expose the Hitch service to the React UI through pywebview.

| Stage | Mechanism | Tag | Ref |
| --- | --- | --- | --- |
| DELEGATE | Map JS methods to service operations | [mandatory] | R1 |
| SERIALIZE | Return JSON-compatible result values | [contract] | R1 |
| PUBLISH | Dispatch status changes into the active page | [mandatory] | R1 |
| CLOSE | Keep a configured alias gateway in the notification area | [mandatory] | L3 |

EARS guards: R1 WHEN a mutating method succeeds, THEN publish its latest status.
R1 IF JS API calls arrive before the window is bound, THEN retain safe operation behavior.
CLOSED: API keys are never event payload fields.
Preconditions: pywebview binds one active window before status notifications.
Failure modes: service errors propagate as bridge rejections for UI display.
"""

from __future__ import annotations

import json
import webbrowser
from threading import RLock
from typing import Any, Optional

from .service import HitchService


class HitchBridge:
    """Translate UI method calls and publish status events.

    Require: service owns all persistence and Windows operations.
    Guarantee: exposed return values are JSON-compatible and secret-free.
    Maintain: only one current window receives change events.
    Assert: mutating operations publish after the service completes.
    """

    def __init__(self, service: HitchService):
        """Bind a service without creating a native window.

        Require: service is configured for a user or sandbox.
        Guarantee: object can be exposed directly as pywebview ``js_api``.
        Maintain: window starts unbound.
        Assert: no method is called during construction.
        """
        self.service = service
        self._window: Optional[Any] = None
        self._lock = RLock()
        self._quitting = False

    def bind_window(self, window: Any) -> None:
        """Store the live pywebview window for status event delivery.

        Require: window supports ``evaluate_js``.
        Guarantee: future status changes target the provided window.
        Maintain: binding does not mutate service state.
        Assert: reference replacement is protected by a lock.
        """
        with self._lock:
            self._window = window

    def status(self) -> dict[str, Any]:
        """Return the current status to the UI.

        Require: service status dependencies are available.
        Guarantee: result matches the TypeScript ``Status`` shape.
        Maintain: status remains read-only.
        Assert: no credential fields are returned.
        """
        return self.service.status()

    def check(self) -> Optional[dict[str, Any]]:
        """Check the saved gateway key and update stale model state.

        Require: service may have a per-host key.
        Guarantee: None means no key is stored.
        Maintain: returned shape matches the TypeScript ``Check`` union.
        Assert: status is published if catalog synchronization wrote a profile.
        """
        result = self.service.check()
        self._publish(self.service.status())
        return result

    def connect(self, api_key: Optional[str] = None, base_url: Optional[str] = None, custom_endpoint: bool = False, model_ids: Optional[list[str]] = None) -> dict[str, Any]:
        """Save the requested endpoint configuration and publish its status.

        Require: custom model ids are the endpoint's manually configured names.
        Guarantee: successful status follows persistence.
        Maintain: bridge errors propagate to the UI.
        Assert: no secret enters the emitted status.
        """
        return self._changed(self.service.connect(api_key, base_url, custom_endpoint, model_ids))

    def set_routing(self, enabled: bool) -> dict[str, Any]:
        """Switch routing between Oxen and Anthropic.

        Require: an owned profile exists.
        Guarantee: returns current status after the mode change.
        Maintain: keys and saved profile remain available.
        Assert: status event follows a successful mutation.
        """
        return self._changed(self.service.set_routing(enabled))

    def set_models(self, selected: list[str]) -> dict[str, Any]:
        """Persist selected model ids in their requested order.

        Require: selected is a list of gateway model ids.
        Guarantee: returns the applied status.
        Maintain: service validates and serializes profile writes.
        Assert: event includes the new model selection only.
        """
        return self._changed(self.service.set_models(selected))

    def remove(self) -> dict[str, Any]:
        """Remove the owned profile and broadcast restored status.

        Require: service can inspect current profile metadata.
        Guarantee: prior profile/mode restoration is reflected in return value.
        Maintain: saved Oxen auth key is retained.
        Assert: removal event carries no credential.
        """
        return self._changed(self.service.remove())

    def restart_claude(self) -> dict[str, Any]:
        """Return the manual Windows restart outcome.

        Require: caller explicitly requested a Claude restart.
        Guarantee: an unverified graceful mechanism is never attempted.
        Maintain: process identity is not terminated.
        Assert: the safe manual instruction is propagated as a UI error.
        """
        return self.service.restart_claude()

    def saved_key(self, host: str) -> Optional[str]:
        """Return a four-character hint for a saved host key.

        Require: host is user-entered text.
        Guarantee: only a redacted hint is exposed.
        Maintain: credential retrieval stays inside the service.
        Assert: no full token crosses the JS bridge.
        """
        return self.service.saved_key(host)

    def claude_icon(self) -> Optional[str]:
        """Return a Claude icon data URL when supported.

        Require: a supported icon asset may exist.
        Guarantee: None is returned when no icon is available.
        Maintain: installation files are read-only.
        Assert: this adapter does not invent an icon.
        """
        return self.service.claude_icon()

    def test_message(self) -> str:
        """Send a test request and return the model that generated text.

        Require: gateway and credentials are configured.
        Guarantee: success follows a valid text response event.
        Maintain: no profile mutation occurs.
        Assert: service rejects empty or failed responses.
        """
        return self.service.test_message()

    def autostart(self) -> bool:
        """Read current-user login startup state.

        Require: current-user registry can be queried.
        Guarantee: returns the effective boolean setting.
        Maintain: no registry write occurs.
        Assert: only OxenHitch's own value is checked.
        """
        return self.service.autostart()

    def set_autostart(self, enabled: bool) -> bool:
        """Set current-user login startup state.

        Require: caller supplies a boolean.
        Guarantee: returns the state read after mutation.
        Maintain: unrelated registry values remain unchanged.
        Assert: permission errors reject the bridge call.
        """
        return self.service.set_autostart(enabled)

    def open_url(self, url: str) -> None:
        """Open a web link in the user's default browser.

        Require: url begins with http or https.
        Guarantee: other schemes are rejected.
        Maintain: native file/command execution is unavailable.
        Assert: URL scheme is validated before launch.
        """
        if not url.startswith(("https://", "http://")):
            raise ValueError("Only web links can be opened.")
        webbrowser.open(url)

    def resize(self, height: float) -> None:
        """Resize the desktop window to the reported content height.

        Require: pywebview window has been bound.
        Guarantee: height is clamped to the supported range.
        Maintain: width stays at the compact app width.
        Assert: UI values cannot create an unbounded window.
        """
        with self._lock:
            window = self._window
        if window is not None:
            window.resize(340, max(120, min(720, int(height))))

    def hide(self) -> None:
        """Hide the window without stopping the tray process.

        Require: window may be bound.
        Guarantee: only the app window visibility changes.
        Maintain: tray menu remains available.
        Assert: no process exit occurs.
        """
        with self._lock:
            window = self._window
        if window is not None:
            window.hide()

    def quit(self) -> None:
        """Stop the window and tray application.

        Require: application binds a close callback on the active window.
        Guarantee: close request is delivered to pywebview.
        Maintain: service persistence has completed before this call.
        Assert: absent window is a no-op.
        """
        with self._lock:
            self._quitting = True
            window = self._window
        if window is not None:
            window.destroy()

    def allow_close(self) -> bool:
        """Hide the window while Claude is using its persistent alias listener.

        Returns:
            Whether the native window may close and stop Hitch.
        """
        if self._quitting or self.service.relay is None:
            return True
        self.hide()
        return False

    def _changed(self, status: dict[str, Any]) -> dict[str, Any]:
        """Publish and return a completed mutation's status.

        Require: status is a JSON-compatible service result.
        Guarantee: same status is returned and emitted.
        Maintain: publication failure does not change saved data.
        Assert: the event payload is compact valid JSON.
        """
        self._publish(status)
        return status

    def _publish(self, status: dict[str, Any]) -> None:
        """Dispatch status to the active page when a window exists.

        Require: status contains no secrets.
        Guarantee: the UI receives a ``hitch-status`` CustomEvent.
        Maintain: no window causes a no-op.
        Assert: JSON is encoded before insertion into the JS expression.
        """
        payload = json.dumps(status, separators=(",", ":"))
        with self._lock:
            window = self._window
        if window is not None:
            window.evaluate_js(f"window.dispatchEvent(new CustomEvent('hitch-status', {{detail:{payload}}}))")
