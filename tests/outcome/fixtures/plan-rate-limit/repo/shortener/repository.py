"""Storage for links. In memory; production has the same interface over SQL."""
import itertools
from dataclasses import dataclass


@dataclass
class Link:
    code: str
    url: str
    owner: str
    hits: int = 0


class LinkRepository:
    def __init__(self):
        self._by_code = {}
        self._seq = itertools.count(1000)

    def next_number(self):
        return next(self._seq)

    def add(self, link):
        self._by_code[link.code] = link
        return link

    def get(self, code):
        return self._by_code.get(code)

    def by_owner(self, owner):
        return [link for link in self._by_code.values() if link.owner == owner]
