import unittest

from catalog import PriceService


class HiddenCurrencyIsPartOfTheCacheKey(unittest.TestCase):
    """One long-lived service answers shoppers in different currencies."""

    def test_usd_then_eur(self):
        svc = PriceService()
        self.assertEqual(svc.price("lamp", "USD"), 4900)
        self.assertEqual(svc.price("lamp", "EUR"), 4508)
        self.assertEqual(svc.label("lamp", "EUR"), "45.08 EUR")

    def test_eur_then_gbp_then_eur(self):
        svc = PriceService()
        self.assertEqual(svc.price("chair", "EUR", "member"), 15649)
        self.assertEqual(svc.price("chair", "GBP", "member"), 13438)
        self.assertEqual(svc.price("chair", "EUR", "member"), 15649)

    def test_price_list_per_currency(self):
        svc = PriceService()
        usd = svc.price_list("USD")
        jpy = svc.price_list("JPY")
        self.assertEqual(usd["mug"], 1200)
        self.assertEqual(jpy["mug"], 181200)

    def test_still_caches(self):
        svc = PriceService()
        svc.price("lamp", "EUR")
        svc.price("lamp", "EUR")
        self.assertEqual(svc.cache.hits, 1)


if __name__ == "__main__":
    unittest.main()
