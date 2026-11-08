import unittest

from orders import Adjustment, Order, OrderLine, OrderService
from orders import pricing, promotions


class PricingTest(unittest.TestCase):
    def test_shipping_below_threshold(self):
        order = Order(1, "ana", [OrderLine("mug", 1200, 2)], adjustments=[])
        self.assertEqual(pricing.total(order), 2400 + 495)

    def test_free_shipping(self):
        order = Order(1, "ana", [OrderLine("lamp", 6000)], adjustments=[])
        self.assertEqual(pricing.total(order), 6000)

    def test_total_never_negative(self):
        order = Order(1, "ana", [OrderLine("pen", 100)], adjustments=[Adjustment("GOODWILL", -900)])
        self.assertEqual(pricing.total(order), 495)


class PromotionTest(unittest.TestCase):
    def test_percent_code(self):
        order = Order(1, "ana", [OrderLine("lamp", 8000)], adjustments=[])
        promotions.apply_code(order, " save10 ", pricing.subtotal(order))
        self.assertEqual(order.codes(), ["SAVE10"])
        self.assertEqual(pricing.total(order), 7200)

    def test_code_only_once(self):
        order = Order(1, "ana", [OrderLine("lamp", 8000)], adjustments=[])
        promotions.apply_code(order, "FIVEOFF", 8000)
        with self.assertRaises(promotions.PromoError):
            promotions.apply_code(order, "fiveoff", 8000)

    def test_unknown_code(self):
        order = Order(1, "ana", [OrderLine("lamp", 8000)], adjustments=[])
        with self.assertRaises(promotions.PromoError):
            promotions.apply_code(order, "FREE", 8000)


class ServiceTest(unittest.TestCase):
    def test_create_and_total(self):
        svc = OrderService()
        order = svc.create("ana", [("mug", 1200, 1), ("lamp", 6000, 1)])
        self.assertEqual(svc.total(order.order_id), 7200)
        self.assertEqual(order.codes(), [])

    def test_place_returns_receipt(self):
        svc = OrderService()
        order = svc.create("ben", [("mug", 1200, 1)])
        receipt = svc.place(order.order_id)
        self.assertIn("total: 16.95", receipt)
        with self.assertRaises(promotions.PromoError):
            svc.apply_code(order.order_id, "SAVE10")


if __name__ == "__main__":
    unittest.main()
