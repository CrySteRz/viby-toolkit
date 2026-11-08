import unittest
from decimal import Decimal

from billing import Invoice
from billing import money
from billing.render import render


class MoneyTest(unittest.TestCase):
    def test_round_half_up(self):
        self.assertEqual(money.round_cents("0.125"), Decimal("0.13"))
        self.assertEqual(money.round_cents("2.675"), Decimal("2.68"))

    def test_refuses_float(self):
        with self.assertRaises(TypeError):
            money.parse(0.1)

    def test_allocate_adds_up(self):
        parts = money.allocate("100.00", 3)
        self.assertEqual(parts, [Decimal("33.34"), Decimal("33.33"), Decimal("33.33")])
        self.assertEqual(sum(parts), Decimal("100.00"))


class InvoiceTest(unittest.TestCase):
    def test_simple_invoice(self):
        inv = Invoice("2026-001", "ACME")
        inv.add("Desk", "100.00")
        inv.add("Chair", "50.00", quantity=2)
        self.assertEqual(inv.subtotal(), Decimal("200.00"))
        self.assertEqual(inv.vat_total(), Decimal("42.00"))
        self.assertEqual(inv.total(), Decimal("242.00"))

    def test_mixed_categories(self):
        inv = Invoice("2026-002", "ACME")
        inv.add("Novel", "20.00", category="books")
        inv.add("Bread", "10.00", category="reduced")
        self.assertEqual(inv.vat_by_category(), {"books": Decimal("1.20"), "reduced": Decimal("0.90")})
        self.assertEqual(inv.total(), Decimal("32.10"))

    def test_discount_before_vat(self):
        inv = Invoice("2026-003", "ACME")
        inv.add("Lamp", "80.00", discount_pct=25)
        self.assertEqual(inv.subtotal(), Decimal("60.00"))
        self.assertEqual(inv.total(), Decimal("72.60"))

    def test_render_mentions_total(self):
        inv = Invoice("2026-004", "ACME")
        inv.add("Desk", "100.00")
        text = render(inv)
        self.assertIn("EUR 121.00", text)
        self.assertIn("VAT 21%", text)


if __name__ == "__main__":
    unittest.main()
