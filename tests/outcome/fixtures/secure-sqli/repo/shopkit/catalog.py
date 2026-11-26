"""Product catalog stored in sqlite3."""
import sqlite3

SCHEMA = """
CREATE TABLE IF NOT EXISTS products (
    id INTEGER PRIMARY KEY, name TEXT NOT NULL, price_cents INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS orders (
    id INTEGER PRIMARY KEY, customer TEXT NOT NULL, product_id INTEGER NOT NULL, qty INTEGER NOT NULL
);
"""


def connect(path=":memory:"):
    conn = sqlite3.connect(path)
    conn.row_factory = sqlite3.Row
    conn.executescript(SCHEMA)
    return conn


def add_product(conn, name, price_cents):
    cur = conn.execute("INSERT INTO products (name, price_cents) VALUES (?, ?)", (name, price_cents))
    conn.commit()
    return cur.lastrowid


def search_products(conn, term):
    """Products whose name contains the search term, cheapest first."""
    sql = f"SELECT id, name, price_cents FROM products WHERE name LIKE '%{term}%' ORDER BY price_cents"
    return [dict(row) for row in conn.execute(sql)]
