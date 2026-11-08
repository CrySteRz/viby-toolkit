import json
import unittest

from webhooks.api import Api
from webhooks.sender import WebhookSender, sign
from webhooks.subscriptions import SubscriptionStore
from webhooks.transport import Response


class FakeClock:
    def __init__(self):
        self.t = 1_700_000_000.0
        self.slept = []

    def now(self):
        return self.t

    def sleep(self, seconds):
        self.slept.append(seconds)
        self.t += seconds


class Scripted:
    """Answers each post with the next scripted item: a Response, or an exception to raise."""

    def __init__(self, *script):
        self.script = list(script)
        self.calls = []

    def post(self, url, body, headers):
        self.calls.append((url, body, headers))
        item = self.script.pop(0)
        if isinstance(item, Exception):
            raise item
        return item


class SenderTest(unittest.TestCase):
    def setUp(self):
        self.sub = SubscriptionStore().add("https://example.com/hook", ["order.paid"], "s3cret")

    def test_success_is_signed(self):
        transport = Scripted(Response(200))
        result = WebhookSender(transport, clock=FakeClock()).send(self.sub, {"type": "order.paid", "id": 7})
        self.assertTrue(result.ok)
        _url, body, headers = transport.calls[0]
        self.assertEqual(json.loads(body), {"id": 7, "type": "order.paid"})
        self.assertEqual(headers["X-Webhook-Signature"], "sha256=" + sign("s3cret", body))
        self.assertEqual(headers["X-Webhook-Timestamp"], "1700000000")

    def test_client_error_fails(self):
        result = WebhookSender(Scripted(Response(410)), clock=FakeClock()).send(self.sub, {"type": "order.paid"})
        self.assertEqual((result.ok, result.status, result.error), (False, 410, "HTTP 410"))


class ApiTest(unittest.TestCase):
    def test_fan_out(self):
        transport = Scripted(Response(200), Response(204))
        api = Api(WebhookSender(transport, clock=FakeClock()))
        api.handle("POST", "/subscriptions", {"url": "https://a.example/h", "events": ["order.paid"], "secret": "a"})
        api.handle("POST", "/subscriptions", {"url": "https://b.example/h", "events": ["*"], "secret": "b"})
        status, body = api.handle("POST", "/events", {"type": "order.paid"})
        self.assertEqual(status, 202)
        self.assertEqual([d["status"] for d in body["deliveries"]], [200, 204])

    def test_bad_subscription(self):
        api = Api(WebhookSender(Scripted(), clock=FakeClock()))
        self.assertEqual(api.handle("POST", "/subscriptions", {"url": "ftp://x"})[0], 400)
        self.assertEqual(api.handle("DELETE", "/subscriptions/4")[0], 404)


if __name__ == "__main__":
    unittest.main()
