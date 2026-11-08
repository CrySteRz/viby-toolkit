"""Invoice lines and totals. VAT is computed and rounded per line, then summed."""
from dataclasses import dataclass, field

from . import money, rates


@dataclass
class Line:
    description: str
    unit_price: object  # str or Decimal, parsed on use
    quantity: int = 1
    category: str = "standard"
    discount_pct: int = 0  # whole percent, applied before VAT

    def net(self):
        gross = money.parse(self.unit_price) * self.quantity
        if self.discount_pct:
            gross -= gross * money.parse(self.discount_pct) / 100
        return money.round_cents(gross)

    def vat(self):
        return money.round_cents(self.net() * rates.rate_for(self.category))

    def total(self):
        return self.net() + self.vat()


@dataclass
class Invoice:
    number: str
    customer: str
    currency: str = "EUR"
    lines: list = field(default_factory=list)

    def add(self, description, unit_price, quantity=1, category="standard", discount_pct=0):
        line = Line(description, unit_price, quantity, category, discount_pct)
        self.lines.append(line)
        return line

    def subtotal(self):
        return sum((line.net() for line in self.lines), money.parse("0.00"))

    def vat_total(self):
        return sum((line.vat() for line in self.lines), money.parse("0.00"))

    def vat_by_category(self):
        out = {}
        for line in self.lines:
            out[line.category] = out.get(line.category, money.parse("0.00")) + line.vat()
        return out

    def total(self):
        return self.subtotal() + self.vat_total()

    def installments(self, count):
        """Split the total into `count` payments that add up to the cent."""
        return money.allocate(self.total(), count)
