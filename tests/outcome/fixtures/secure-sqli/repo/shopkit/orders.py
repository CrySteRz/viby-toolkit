"""Customer orders."""


def place_order(conn, customer, product_id, qty=1):
    cur = conn.execute(
        "INSERT INTO orders (customer, product_id, qty) VALUES (?, ?, ?)", (customer, product_id, qty)
    )
    conn.commit()
    return cur.lastrowid


def orders_for_customer(conn, customer):
    """Every order a customer placed, newest first, with the product name."""
    rows = conn.execute(
        "SELECT o.id, p.name, o.qty FROM orders o JOIN products p ON p.id = o.product_id "
        "WHERE o.customer = ? ORDER BY o.id DESC",
        (customer,),
    )
    return [dict(row) for row in rows]
