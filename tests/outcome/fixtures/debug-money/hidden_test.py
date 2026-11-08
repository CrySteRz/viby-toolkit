import unittest
from decimal import ROUND_HALF_UP, Decimal

from billing import Invoice


class HiddenVatRounding(unittest.TestCase):
    """VAT on a half cent must round up, exactly as computed by hand in decimal."""

    def test_standard_rate_half_cent(self):
        inv = Invoice("H-1", "Customer")
        inv.add("Sticker", "2.50")  # 2.50 * 21% = 0.525 -> 0.53
        self.assertEqual(inv.vat_total(), Decimal("0.53"))
        self.assertEqual(inv.total(), Decimal("3.03"))

    def test_reduced_rate_half_cent(self):
        inv = Invoice("H-2", "Customer")
        inv.add("Coffee", "0.50", category="reduced")  # 0.50 * 9% = 0.045 -> 0.05
        self.assertEqual(inv.vat_total(), Decimal("0.05"))

    def test_many_lines_match_hand_calculation(self):
        inv = Invoice("H-3", "Customer")
        prices = ["2.50", "7.50", "12.50", "0.50", "1.50", "4.50"]
        for p in prices:
            inv.add("Item", p)
        expected = sum(
            ((Decimal(p) * Decimal("0.21")).quantize(Decimal("0.01"), rounding=ROUND_HALF_UP) for p in prices),
            Decimal("0"),
        )
        self.assertEqual(inv.vat_total(), expected)


if __name__ == "__main__":
    unittest.main()
