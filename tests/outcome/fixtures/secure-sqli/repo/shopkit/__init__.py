"""A tiny shop backend on sqlite3: a product catalog and customer orders."""
from .catalog import add_product, connect, search_products
from .orders import orders_for_customer, place_order

__all__ = ["add_product", "connect", "search_products", "orders_for_customer", "place_order"]
