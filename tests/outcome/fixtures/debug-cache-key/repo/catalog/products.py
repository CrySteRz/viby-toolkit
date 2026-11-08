"""The product table. Base prices are USD cents; tiers get a percentage off."""

PRODUCTS = {
    "lamp": {"name": "Desk lamp", "usd_cents": 4900},
    "chair": {"name": "Office chair", "usd_cents": 18900},
    "mug": {"name": "Mug", "usd_cents": 1200},
}

TIER_DISCOUNT_PCT = {"retail": 0, "member": 10, "wholesale": 25}


def base_price(sku):
    try:
        return PRODUCTS[sku]["usd_cents"]
    except KeyError:
        raise KeyError(f"unknown sku {sku!r}") from None


def tier_discount(tier):
    try:
        return TIER_DISCOUNT_PCT[tier]
    except KeyError:
        raise ValueError(f"unknown tier {tier!r}") from None
