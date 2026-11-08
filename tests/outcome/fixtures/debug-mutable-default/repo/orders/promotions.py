"""Promo codes. A code applies at most once per order."""
from .models import Adjustment

PROMOS = {
    "SAVE10": ("percent", 10),
    "SAVE20": ("percent", 20),
    "FIVEOFF": ("fixed", 500),
}


class PromoError(ValueError):
    pass


def discount_for(code, subtotal_cents):
    try:
        kind, value = PROMOS[code]
    except KeyError:
        raise PromoError(f"unknown promo code {code!r}") from None
    if kind == "percent":
        return -(subtotal_cents * value // 100)
    return -min(value, subtotal_cents)


def apply_code(order, code, subtotal_cents):
    code = code.strip().upper()
    if code in order.codes():
        raise PromoError(f"{code} is already applied")
    adjustment = Adjustment(code, discount_for(code, subtotal_cents), reason="promo code")
    order.adjustments.append(adjustment)
    return adjustment
