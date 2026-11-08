"""PriceService: what a shopper pays for a product, in their currency and tier."""
from . import fx, products
from .cache import TTLCache


def cache_key(sku, tier):
    return f"price:{sku}:{tier}"


class PriceService:
    def __init__(self, cache=None):
        self.cache = cache or TTLCache()

    def _compute(self, sku, currency, tier):
        usd = products.base_price(sku)
        usd -= usd * products.tier_discount(tier) // 100
        return fx.convert_cents(usd, currency)

    def price(self, sku, currency="USD", tier="retail"):
        """Price in integer minor units of `currency` (cents; whole yen for JPY)."""
        return self.cache.get_or_compute(
            cache_key(sku, tier), lambda: self._compute(sku, currency, tier)
        )

    def price_list(self, currency="USD", tier="retail"):
        return {sku: self.price(sku, currency, tier) for sku in sorted(products.PRODUCTS)}

    def label(self, sku, currency="USD", tier="retail"):
        cents = self.price(sku, currency, tier)
        if currency in fx.ZERO_DECIMAL:
            return f"{cents // 100} {currency}"
        return f"{cents / 100:.2f} {currency}"
