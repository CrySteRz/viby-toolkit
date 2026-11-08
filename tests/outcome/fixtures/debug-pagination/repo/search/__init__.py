"""Document search with paged results, as served to the mobile app."""
from .api import search
from .index import Index

__all__ = ["Index", "search"]
