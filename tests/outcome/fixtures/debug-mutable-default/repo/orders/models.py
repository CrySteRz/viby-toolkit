"""Order data. Kept as plain classes so the ORM layer can subclass them."""

NO_ADJUSTMENTS = []


class OrderLine:
    def __init__(self, sku, unit_cents, quantity=1):
        if quantity < 1:
            raise ValueError("quantity must be at least 1")
        self.sku = sku
        self.unit_cents = unit_cents
        self.quantity = quantity

    def cents(self):
        return self.unit_cents * self.quantity

    def __repr__(self):
        return f"OrderLine({self.sku!r}, {self.unit_cents}, {self.quantity})"


class Adjustment:
    """A change to the order total: a promo code, a goodwill credit, a shipping fee."""

    def __init__(self, code, cents, reason=""):
        self.code = code
        self.cents = cents  # negative for discounts
        self.reason = reason

    def __eq__(self, other):
        return isinstance(other, Adjustment) and (self.code, self.cents) == (other.code, other.cents)

    def __repr__(self):
        return f"Adjustment({self.code!r}, {self.cents})"


class Order:
    def __init__(self, order_id, customer, lines, adjustments=NO_ADJUSTMENTS):
        self.order_id = order_id
        self.customer = customer
        self.lines = list(lines)
        self.adjustments = adjustments
        self.status = "open"

    def codes(self):
        return [a.code for a in self.adjustments]
