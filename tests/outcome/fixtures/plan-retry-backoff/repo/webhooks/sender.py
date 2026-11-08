"""Deliver one event to one subscription: sign it, POST it, report what happened."""
import hashlib
import hmac
import json
from dataclasses import dataclass

from .clock import SystemClock


@dataclass
class DeliveryResult:
    subscription_id: int
    ok: bool
    status: object  # int, or None when no HTTP answer came back
    error: str = ""


def sign(secret, body):
    return hmac.new(secret.encode(), body, hashlib.sha256).hexdigest()


class WebhookSender:
    def __init__(self, transport, clock=None):
        self.transport = transport
        self.clock = clock or SystemClock()

    def _request(self, sub, event):
        body = json.dumps(event, sort_keys=True, separators=(",", ":")).encode()
        headers = {
            "Content-Type": "application/json",
            "X-Webhook-Signature": "sha256=" + sign(sub.secret, body),
            "X-Webhook-Timestamp": str(int(self.clock.now())),
        }
        return body, headers

    def send(self, sub, event):
        body, headers = self._request(sub, event)
        try:
            resp = self.transport.post(sub.url, body, headers)
        except (ConnectionError, TimeoutError) as e:
            return DeliveryResult(sub.id, False, None, f"{type(e).__name__}: {e}")
        ok = 200 <= resp.status < 300
        return DeliveryResult(sub.id, ok, resp.status, "" if ok else f"HTTP {resp.status}")
