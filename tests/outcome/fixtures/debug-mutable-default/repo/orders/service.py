"""The order service used by the web handlers. One instance lives for the whole worker process."""
import itertools

from . import pricing, promotions
from .models import Order, OrderLine


class OrderService:
    def __init__(self):
        self._orders = {}
        self._ids = itertools.count(1)

    def create(self, customer, items):
        """items: iterable of (sku, unit_cents, quantity)."""
        order = Order(next(self._ids), customer, [OrderLine(*item) for item in items])
        self._orders[order.order_id] = order
        return order

    def get(self, order_id):
        return self._orders[order_id]

    def apply_code(self, order_id, code):
        order = self.get(order_id)
        if order.status != "open":
            raise promotions.PromoError("order is already placed")
        return promotions.apply_code(order, code, pricing.subtotal(order))

    def total(self, order_id):
        return pricing.total(self.get(order_id))

    def place(self, order_id):
        order = self.get(order_id)
        order.status = "placed"
        return pricing.receipt(order)
