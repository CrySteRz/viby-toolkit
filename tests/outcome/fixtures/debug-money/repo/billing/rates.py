"""VAT rates by product category.

The table mirrors the finance team's spreadsheet export, which is why the rates are plain
numbers rather than strings."""
from decimal import Decimal

VAT_RATES = {
    "standard": 0.21,
    "reduced": 0.09,
    "books": 0.06,
    "zero": 0.0,
}


def categories():
    return sorted(VAT_RATES)


def rate_for(category):
    """The VAT rate for a category as a Decimal fraction (0.21 means 21%)."""
    try:
        rate = VAT_RATES[category]
    except KeyError:
        raise ValueError(f"unknown VAT category: {category!r}") from None
    return Decimal(rate)


def percent_label(category):
    """'21%' for display on the invoice."""
    return f"{VAT_RATES[category] * 100:g}%"
