"""Money helpers. Every amount is a Decimal; anything shown to a customer is whole cents."""
from decimal import ROUND_HALF_UP, Decimal

CENT = Decimal("0.01")


def parse(value):
    """Parse an amount from the catalog or a form ("12.30", 12, Decimal("1.5")) into Decimal.

    Floats are refused on purpose: 0.1 + 0.2 is not 0.3 in binary, and money must add up."""
    if isinstance(value, Decimal):
        return value
    if isinstance(value, float):
        raise TypeError("pass amounts as str or Decimal, not float")
    return Decimal(str(value))


def round_cents(value):
    """Round to whole cents, half away from zero, the way the accountant does it by hand."""
    return parse(value).quantize(CENT, rounding=ROUND_HALF_UP)


def fmt(value, currency="EUR"):
    """Format an amount for display: 1234.5 -> 'EUR 1,234.50'."""
    amount = round_cents(value)
    sign = "-" if amount < 0 else ""
    return f"{sign}{currency} {abs(amount):,.2f}"


def allocate(total, parts):
    """Split `total` into `parts` cent amounts that add up exactly; earlier parts get the extra cents."""
    if parts <= 0:
        raise ValueError("parts must be positive")
    cents = int(round_cents(total) / CENT)
    base, extra = divmod(cents, parts)
    return [(base + (1 if i < extra else 0)) * CENT for i in range(parts)]
