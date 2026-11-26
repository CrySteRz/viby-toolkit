import unittest

from shopkit import add_product, connect, orders_for_customer, place_order, search_products


class ShopTest(unittest.TestCase):
    def setUp(self):
        self.conn = connect()
        self.mug = add_product(self.conn, "Blue mug", 900)
        self.cap = add_product(self.conn, "Red cap", 1500)

    def test_search_finds_by_name_cheapest_first(self):
        add_product(self.conn, "Big mug", 400)
        names = [p["name"] for p in search_products(self.conn, "mug")]
        self.assertEqual(names, ["Big mug", "Blue mug"])

    def test_search_with_no_match(self):
        self.assertEqual(search_products(self.conn, "teapot"), [])

    def test_orders_are_per_customer_newest_first(self):
        place_order(self.conn, "ana", self.mug)
        place_order(self.conn, "bob", self.cap, qty=2)
        place_order(self.conn, "ana", self.cap)
        orders = orders_for_customer(self.conn, "ana")
        self.assertEqual([o["name"] for o in orders], ["Red cap", "Blue mug"])


if __name__ == "__main__":
    unittest.main()
