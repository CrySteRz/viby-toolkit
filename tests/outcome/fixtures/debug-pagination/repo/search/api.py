"""The /search endpoint, as plain functions returning JSON-ready dicts."""
from . import paging


def summarize(doc, width=60):
    body = doc["body"]
    snippet = body if len(body) <= width else body[: width - 1].rstrip() + "…"
    return {"id": doc["id"], "title": doc["title"], "snippet": snippet}


def search(index, text, page=1, per_page=10):
    result = paging.paginate(index.query(text), page, per_page)
    return {
        "results": [summarize(d) for d in result.items],
        "page": result.page,
        "per_page": result.per_page,
        "total": result.total,
        "total_pages": result.total_pages,
        "has_next": result.has_next,
        "has_prev": result.has_prev,
    }
