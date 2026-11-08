import unittest

from webhooks.api import Api
from webhooks.retry import RetryPolicy
from webhooks.sender import WebhookSender
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
    def __init__(self, *script):
        self.script = list(script)
        self.calls = []

    def post(self, url, body, headers):
        self.calls.append((url, body, dict(headers)))
        item = self.script.pop(0)
        if isinstance(item, Exception):
            raise item
        return item


def sub():
    return SubscriptionStore().add("https://example.com/hook", ["order.paid"], "s3cret")


EVENT = {"type": "order.paid", "id": 1}


class HiddenRetryPolicy(unittest.TestCase):
    def test_default_delays_double_and_cap(self):
        policy = RetryPolicy()
        self.assertEqual((policy.max_attempts, policy.base_delay, policy.max_delay), (5, 0.5, 30.0))
        self.assertEqual([policy.delay(n) for n in range(1, 9)], [0.5, 1, 2, 4, 8, 16, 30, 30])

    def test_custom(self):
        self.assertEqual(RetryPolicy(max_attempts=3, base_delay=2, max_delay=5).delay(3), 5)


class HiddenSenderRetries(unittest.TestCase):
    def test_transient_then_success(self):
        clock = FakeClock()
        transport = Scripted(ConnectionError("reset"), Response(503), Response(200))
        result = WebhookSender(transport, clock=clock).send(sub(), EVENT)
        self.assertTrue(result.ok)
        self.assertEqual((result.attempts, result.status), (3, 200))
        self.assertEqual(clock.slept, [0.5, 1])

    def test_timeout_is_retried(self):
        clock = FakeClock()
        result = WebhookSender(Scripted(TimeoutError("slow"), Response(201)), clock=clock).send(sub(), EVENT)
        self.assertEqual((result.ok, result.attempts), (True, 2))

    def test_gives_up_after_max_attempts(self):
        clock = FakeClock()
        transport = Scripted(*[Response(500)] * 5)
        result = WebhookSender(transport, clock=clock).send(sub(), EVENT)
        self.assertEqual((result.ok, result.attempts, result.status), (False, 5, 500))
        self.assertEqual(clock.slept, [0.5, 1, 2, 4])
        self.assertEqual(len(transport.calls), 5)

    def test_client_errors_are_not_retried(self):
        for status in (400, 401, 404, 410):
            with self.subTest(status=status):
                clock = FakeClock()
                result = WebhookSender(Scripted(Response(status)), clock=clock).send(sub(), EVENT)
                self.assertEqual((result.ok, result.attempts, result.status), (False, 1, status))
                self.assertEqual(clock.slept, [])

    def test_success_first_time_never_sleeps(self):
        clock = FakeClock()
        result = WebhookSender(Scripted(Response(200)), clock=clock).send(sub(), EVENT)
        self.assertEqual((result.ok, result.attempts), (True, 1))
        self.assertEqual(clock.slept, [])

    def test_retry_after_header(self):
        clock = FakeClock()
        transport = Scripted(Response(429, {"Retry-After": "7"}), Response(429, {"Retry-After": "120"}), Response(200))
        result = WebhookSender(transport, clock=clock).send(sub(), EVENT)
        self.assertEqual(result.attempts, 3)
        self.assertEqual(clock.slept, [7, 30])

    def test_429_without_header_uses_backoff(self):
        clock = FakeClock()
        WebhookSender(Scripted(Response(429), Response(200)), clock=clock).send(sub(), EVENT)
        self.assertEqual(clock.slept, [0.5])

    def test_custom_policy_and_fresh_timestamp(self):
        clock = FakeClock()
        transport = Scripted(Response(502), Response(502))
        sender = WebhookSender(transport, clock=clock, policy=RetryPolicy(max_attempts=2, base_delay=2))
        result = sender.send(sub(), EVENT)
        self.assertEqual((result.ok, result.attempts), (False, 2))
        stamps = [headers["X-Webhook-Timestamp"] for _u, _b, headers in transport.calls]
        self.assertEqual(stamps, ["1700000000", "1700000002"])


class HiddenApiReportsAttempts(unittest.TestCase):
    def test_attempts_in_response(self):
        clock = FakeClock()
        transport = Scripted(Response(503), Response(200), *[Response(500)] * 5)
        api = Api(WebhookSender(transport, clock=clock))
        api.handle("POST", "/subscriptions", {"url": "https://a.example/h", "events": ["order.paid"], "secret": "a"})
        api.handle("POST", "/subscriptions", {"url": "https://b.example/h", "events": ["order.paid"], "secret": "b"})
        status, body = api.handle("POST", "/events", EVENT)
        self.assertEqual(status, 502)
        self.assertEqual([(d["ok"], d["attempts"]) for d in body["deliveries"]], [(True, 2), (False, 5)])


if __name__ == "__main__":
    unittest.main()
