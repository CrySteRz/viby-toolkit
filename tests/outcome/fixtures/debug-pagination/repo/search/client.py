"""What the mobile app does: fetch page after page until the server says there is no next one."""
from .api import search


def iterate_all(index, text, per_page=10):
    page = 1
    while True:
        body = search(index, text, page=page, per_page=per_page)
        yield from body["results"]
        if not body["has_next"]:
            return
        page += 1


def count_all(index, text, per_page=10):
    return sum(1 for _ in iterate_all(index, text, per_page))
