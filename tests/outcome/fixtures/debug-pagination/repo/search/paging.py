"""Page arithmetic, shared by every list endpoint."""
from dataclasses import dataclass

MAX_PER_PAGE = 50


@dataclass
class Page:
    items: list
    page: int
    per_page: int
    total: int
    total_pages: int

    @property
    def has_next(self):
        return self.page < self.total_pages

    @property
    def has_prev(self):
        return self.page > 1


def clamp_per_page(per_page):
    return max(1, min(int(per_page), MAX_PER_PAGE))


def paginate(items, page=1, per_page=10):
    """Slice `items` into 1-based page `page`."""
    per_page = clamp_per_page(per_page)
    page = max(1, int(page))
    total = len(items)
    total_pages = max(1, total // per_page)
    start = (page - 1) * per_page
    return Page(items[start:start + per_page], page, per_page, total, total_pages)
