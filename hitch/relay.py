"""Route Claude model aliases to LiteLLM on behalf of the Windows app.

| Stage | Mechanism | Tag | Ref |
| --- | --- | --- | --- |
| BIND | Listen on a saved or ephemeral IPv4 loopback port | [mandatory] | L2 |
| VERIFY | Authenticate the profile token and resolve a known model alias | [mandatory] | L3 |
| LIST | Publish alias IDs together with the upstream display names | [mandatory] | L2 |
| FORWARD | Change only the model in the request and stream LiteLLM's reply | [mandatory] | L3 |

L3 IF a route or alias is unknown, THEN reject before making an upstream call.
CLOSED: LiteLLM owns protocol conversion and upstream deployments.
Preconditions: Hitch supplies a trusted upstream URL and locally generated token.
Failure modes: port binding errors raise; upstream connection failures return 502.
"""

from __future__ import annotations

import json
from dataclasses import dataclass
from hmac import compare_digest
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from threading import Thread
from typing import Any
from urllib.parse import urlsplit

import httpx

from .models import Model


@dataclass(frozen=True)
class RelayTarget:
    """Hold a complete routing snapshot for one request.

    Require: the service owns the upstream address, key, and alias map.
    Guarantee: each request uses one configuration even during refresh.
    """

    base_url: str
    provider_key: str | None
    token: str
    aliases: dict[str, Model]


class ModelRelay(ThreadingHTTPServer):
    """Run Claude's authenticated alias gateway for the lifetime of Hitch.

    Require: caller binds one listener per active profile.
    Maintain: the listener accepts only local connections.
    """

    target: RelayTarget | None = None
    allow_reuse_address = False

    def __init__(self, port: int = 0) -> None:
        """Bind loopback and start serving on a background thread.

        Args:
            port: Saved profile port or zero for an available port.
        Raises:
            OSError: The requested port cannot be opened.
        """
        super().__init__(("127.0.0.1", port), RelayRequest)
        self.thread = Thread(target=self.serve_forever, kwargs={"poll_interval": 0.1}, daemon=True)
        self.thread.start()

    def close(self) -> None:
        """Release the listener without waiting for in-flight chats.

        Require: caller is outside the server thread.
        """
        self.shutdown()
        self.server_close()
        self.thread.join()


class RelayRequest(BaseHTTPRequestHandler):
    """Accept Claude's model listing, Messages, and token count routes.

    Maintain: the request cannot determine either the destination host or credential.
    """

    server: ModelRelay
    timeout = 30

    def log_message(self, format: str, *args: Any) -> None:
        """Keep gateway requests out of the windowed application's stderr."""

    def do_GET(self) -> None:
        """Return the configured alias catalog to the authenticated client."""
        target = self._target()
        if target is None:
            return
        if urlsplit(self.path).path != "/v1/models":
            self._error(404, "Unknown gateway route.")
            return
        self._json(200, {"data": [
            {"id": alias, "type": "model", "display_name": model.display_name, "created_at": "1970-01-01T00:00:00Z"}
            for alias, model in target.aliases.items()
        ], "has_more": False})

    def do_POST(self) -> None:
        """Replace an allowed alias before forwarding an Anthropic request."""
        target = self._target()
        if target is None:
            return
        parsed = urlsplit(self.path)
        if parsed.path not in ("/v1/messages", "/v1/messages/count_tokens"):
            self._error(404, "Unknown gateway route.")
            return
        size = self.headers.get("Content-Length", "")
        if not size.isascii() or not size.isdigit() or len(size) > 8 or int(size) > 32 * 1024 * 1024:
            self._error(413, "A request body up to 32 MiB is required.")
            return
        try:
            body = json.loads(self.rfile.read(int(size)))
        except (ValueError, OSError):
            self._error(400, "Expected a JSON request body.")
            return
        alias = body.get("model") if isinstance(body, dict) else None
        if not isinstance(alias, str) or alias not in target.aliases:
            self._error(400, "Unknown Hitch model alias. Refresh the models in Hitch.")
            return
        body["model"] = target.aliases[alias].id
        headers = {
            name: self.headers[name]
            for name in ("anthropic-version", "anthropic-beta", "Accept", "User-Agent")
            if name in self.headers
        }
        headers["Accept-Encoding"] = "identity"
        if target.provider_key:
            headers["Authorization"] = f"Bearer {target.provider_key}"
        query = "?" + parsed.query if parsed.query else ""
        self._forward(target.base_url + parsed.path + query, body, headers)

    def _target(self) -> RelayTarget | None:
        """Validate the loopback host and the random Claude profile token."""
        target = self.server.target
        expected_host = f"127.0.0.1:{self.server.server_port}"
        token = self.headers.get("x-api-key") or self.headers.get("Authorization", "").removeprefix("Bearer ")
        if target is None or self.headers.get("Host") != expected_host or not compare_digest(token.encode(), target.token.encode()):
            self._error(401, "Hitch gateway authorization failed.")
            return None
        return target

    def _forward(self, url: str, body: dict[str, Any], headers: dict[str, str]) -> None:
        """Stream LiteLLM's status and response body without buffering tokens."""
        started = False
        try:
            with httpx.Client(timeout=httpx.Timeout(300, connect=5), trust_env=False) as client:
                with client.stream("POST", url, json=body, headers=headers) as response:
                    started = True
                    self.send_response(response.status_code)
                    for name in ("Content-Type", "Content-Encoding", "Request-Id", "Retry-After"):
                        if name in response.headers:
                            self.send_header(name, response.headers[name])
                    self.end_headers()
                    for chunk in response.iter_raw():
                        self.wfile.write(chunk)
        except httpx.HTTPError:
            if not started:
                self._error(502, "Hitch could not reach LiteLLM.")
        except OSError:
            # The client may cancel an in-flight response.
            return

    def _json(self, status: int, body: dict[str, Any]) -> None:
        """Return one completed JSON response."""
        encoded = json.dumps(body).encode()
        self.send_response(status)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(encoded)))
        self.end_headers()
        self.wfile.write(encoded)

    def _error(self, status: int, message: str) -> None:
        """Return an Anthropic-shaped error without echoing credentials or prompts."""
        self._json(status, {"type": "error", "error": {"type": "api_error", "message": message}})
