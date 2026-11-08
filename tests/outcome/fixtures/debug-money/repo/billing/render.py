"""Plain-text rendering of an invoice, as attached to the confirmation e-mail."""
from . import money, rates


def render(invoice):
    cur = invoice.currency
    out = [f"Invoice {invoice.number} for {invoice.customer}", ""]
    for line in invoice.lines:
        label = f"{line.quantity} x {line.description}"
        if line.discount_pct:
            label += f" (-{line.discount_pct}%)"
        out.append(f"{label:<40}{money.fmt(line.net(), cur):>16}")
    out.append("")
    out.append(f"{'Subtotal':<40}{money.fmt(invoice.subtotal(), cur):>16}")
    for category, vat in sorted(invoice.vat_by_category().items()):
        out.append(f"{'VAT ' + rates.percent_label(category):<40}{money.fmt(vat, cur):>16}")
    out.append(f"{'Total':<40}{money.fmt(invoice.total(), cur):>16}")
    return "\n".join(out)
