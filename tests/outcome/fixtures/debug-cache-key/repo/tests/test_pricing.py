import unittest

from catalog import PriceService
from catalog.cache import TTLCache
from catalog import fx


class FakeClock:
    def __init__(self):
        self.now = 0.0

    def __call__(self):
        return self.now


class CacheTest(unittest.TestCase):
    def test_expires(self):
        clock = FakeClock()
        cache = TTLCache(ttl_seconds=60, clock=clock)
        cache.set("k", 1)
        clock.now = 59
        self.assertEqual(cache.get("k"), 1)
        clock.now = 60
        self.assertIsNone(cache.get("k"))

    def test_get_or_compute_counts(self):
        cache = TTLCache()
        self.assertEqual(cache.get_or_compute("k", lambda: 5), 5)
        self.assertEqual(cache.get_or_compute("k", lambda: 6), 5)
        self.assertEqual((cache.hits, cache.misses), (1, 1))


class PricingTest(unittest.TestCase):
    def test_usd_retail(self):
        self.assertEqual(PriceService().price("lamp"), 4900)

    def test_member_discount(self):
        self.assertEqual(PriceService().price("chair", tier="member"), 17010)

    def test_euro_price(self):
        self.assertEqual(PriceService().price("lamp", "EUR"), 4508)

    def test_yen_has_no_cents(self):
        self.assertEqual(PriceService().label("mug", "JPY"), "1812 JPY")

    def test_tiers_are_cached_separately(self):
        svc = PriceService()
        self.assertEqual(svc.price("mug", tier="retail"), 1200)
        self.assertEqual(svc.price("mug", tier="wholesale"), 900)

    def test_unsupported_currency(self):
        with self.assertRaises(ValueError):
            fx.convert_cents(100, "XXX")


if __name__ == "__main__":
    unittest.main()
