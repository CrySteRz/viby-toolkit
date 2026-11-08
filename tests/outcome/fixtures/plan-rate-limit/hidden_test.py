import unittest

from shortener.api import create_app
from shortener.ratelimit import RateLimiter


class FakeClock:
    def __init__(self, t=1000.0):
        self.t = t

    def __call__(self):
        return self.t


class HiddenRateLimiter(unittest.TestCase):
    def test_limit_then_window_expires(self):
        clock = FakeClock()
        rl = RateLimiter(limit=5, window=60.0, clock=clock)
        self.assertEqual([rl.allow("ana") for _ in range(6)], [True] * 5 + [False])
        self.assertEqual(rl.retry_after("ana"), 60)
        clock.t += 59.9
        self.assertFalse(rl.allow("ana"))
        self.assertEqual(rl.retry_after("ana"), 1)
        clock.t = 1060.0
        self.assertEqual(rl.retry_after("ana"), 0)
        self.assertTrue(rl.allow("ana"))

    def test_window_slides(self):
        clock = FakeClock(0.0)
        rl = RateLimiter(limit=5, window=60.0, clock=clock)
        for t in (0, 10, 20, 30, 40):
            clock.t = t
            self.assertTrue(rl.allow("ana"))
        clock.t = 50
        self.assertFalse(rl.allow("ana"))
        clock.t = 60
        self.assertTrue(rl.allow("ana"))
        clock.t = 61
        self.assertFalse(rl.allow("ana"))
        self.assertEqual(rl.retry_after("ana"), 9)

    def test_denied_requests_do_not_count(self):
        clock = FakeClock(0.0)
        rl = RateLimiter(limit=2, window=10.0, clock=clock)
        rl.allow("ana"), rl.allow("ana")
        clock.t = 5
        for _ in range(20):
            self.assertFalse(rl.allow("ana"))
        clock.t = 10
        self.assertTrue(rl.allow("ana"))

    def test_users_are_independent(self):
        rl = RateLimiter(limit=1, window=60.0, clock=FakeClock())
        self.assertTrue(rl.allow("ana"))
        self.assertFalse(rl.allow("ana"))
        self.assertTrue(rl.allow("ben"))

    def test_defaults(self):
        rl = RateLimiter(clock=FakeClock())
        self.assertEqual(sum(rl.allow("ana") for _ in range(10)), 5)


class HiddenApiRateLimit(unittest.TestCase):
    def post(self, app, user, url="https://example.com/x"):
        return app.handle("POST", "/links", {"X-User": user} if user else {}, {"url": url})

    def test_sixth_link_in_a_minute_is_429(self):
        clock = FakeClock()
        app = create_app(clock=clock)
        codes = [self.post(app, "ana")[2]["code"] for _ in range(5)]
        status, headers, body = self.post(app, "ana")
        self.assertEqual(status, 429)
        self.assertEqual(headers["Retry-After"], "60")
        self.assertIn("error", body)
        self.assertEqual(self.post(app, "ben")[0], 201)
        self.assertEqual(app.handle("GET", "/" + codes[0])[0], 302)
        clock.t += 60
        self.assertEqual(self.post(app, "ana")[0], 201)

    def test_unauthenticated_does_not_count_but_invalid_does(self):
        clock = FakeClock()
        app = create_app(clock=clock)
        for _ in range(10):
            self.assertEqual(self.post(app, None)[0], 401)
        for _ in range(4):
            self.assertEqual(self.post(app, "ana", url="nope")[0], 400)
        self.assertEqual(self.post(app, "ana")[0], 201)
        self.assertEqual(self.post(app, "ana")[0], 429)

    def test_injected_limiter(self):
        clock = FakeClock()
        app = create_app(limiter=RateLimiter(limit=1, window=5.0, clock=clock))
        self.assertEqual(self.post(app, "ana")[0], 201)
        status, headers, _ = self.post(app, "ana")
        self.assertEqual((status, headers["Retry-After"]), (429, "5"))

    def test_listing_is_not_limited(self):
        app = create_app(clock=FakeClock())
        for _ in range(5):
            self.post(app, "ana")
        for _ in range(20):
            self.assertEqual(app.handle("GET", "/links", {"X-User": "ana"})[0], 200)


if __name__ == "__main__":
    unittest.main()
