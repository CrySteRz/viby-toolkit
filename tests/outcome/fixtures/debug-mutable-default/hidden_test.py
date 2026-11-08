import unittest

from orders import Order, OrderLine, OrderService


class HiddenAdjustmentsDoNotLeak(unittest.TestCase):
    """A promo code applied to one order must never show up on another."""

    def test_second_customer_gets_no_discount(self):
        svc = OrderService()
        first = svc.create("ana", [("lamp", 8000, 1)])
        svc.apply_code(first.order_id, "SAVE10")
        second = svc.create("ben", [("lamp", 8000, 1)])
        self.assertEqual(second.codes(), [])
        self.assertEqual(svc.total(second.order_id), 8000)
        self.assertEqual(svc.total(first.order_id), 7200)

    def test_two_services_share_nothing(self):
        a, b = OrderService(), OrderService()
        order_a = a.create("ana", [("mug", 1200, 1)])
        a.apply_code(order_a.order_id, "FIVEOFF")
        order_b = b.create("ben", [("mug", 1200, 1)])
        b.apply_code(order_b.order_id, "SAVE20")
        self.assertEqual(order_a.codes(), ["FIVEOFF"])
        self.assertEqual(order_b.codes(), ["SAVE20"])

    def test_direct_construction(self):
        one = Order(1, "ana", [OrderLine("pen", 100)])
        one.adjustments.append("marker")
        two = Order(2, "ben", [OrderLine("pen", 100)])
        self.assertEqual(two.adjustments, [])


if __name__ == "__main__":
    unittest.main()
