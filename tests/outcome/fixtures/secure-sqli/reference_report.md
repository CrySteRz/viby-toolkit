# Security review

## 1. SQL injection in the product search - HIGH

`shopkit/catalog.py:29`, function `search_products` (CWE-89). The search term is formatted
straight into the SQL text (`f"... LIKE '%{term}%'"`), so a caller can close the quote and append
their own SQL, for example `' UNION SELECT ...`, and read or change anything in the database.
Fix: pass the pattern as a bound parameter (`LIKE ?` with `f"%{term}%"`).

## Checked and fine

`orders_for_customer` in `shopkit/orders.py` binds the customer with a `?` placeholder, so it is
not injectable.
