"""Totals. Shipping is free from 50.00; adjustments never take a total below zero."""

FREE_SHIPPING_FROM = 5000
SHIPPING_CENTS = 495


def subtotal(order):
    return sum(line.cents() for line in order.lines)


def shipping(order):
    return 0 if subtotal(order) >= FREE_SHIPPING_FROM else SHIPPING_CENTS


def total(order):
    adjusted = subtotal(order) + sum(a.cents for a in order.adjustments)
    return max(adjusted, 0) + shipping(order)


def receipt(order):
    rows = [f"{line.quantity} x {line.sku}: {line.cents() / 100:.2f}" for line in order.lines]
    rows += [f"{a.code}: {a.cents / 100:.2f}" for a in order.adjustments]
    rows.append(f"shipping: {shipping(order) / 100:.2f}")
    rows.append(f"total: {total(order) / 100:.2f}")
    return "\n".join(rows)
