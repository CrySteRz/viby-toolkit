"""Exchange rates against USD, refreshed nightly by a cron job. Amounts are integer cents."""

RATES_PER_USD = {
    "USD": 1.0,
    "EUR": 0.92,
    "GBP": 0.79,
    "JPY": 151.0,
}

ZERO_DECIMAL = {"JPY"}


def convert_cents(usd_cents, currency):
    try:
        rate = RATES_PER_USD[currency]
    except KeyError:
        raise ValueError(f"unsupported currency {currency!r}") from None
    amount = usd_cents * rate
    if currency in ZERO_DECIMAL:
        return int(round(amount / 100)) * 100
    return int(round(amount))
