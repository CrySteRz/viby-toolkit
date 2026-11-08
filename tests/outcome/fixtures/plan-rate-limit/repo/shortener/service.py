"""Rules for creating and resolving links."""
from urllib.parse import urlsplit

from . import codes
from .repository import Link, LinkRepository


class InvalidUrl(ValueError):
    pass


class LinkService:
    def __init__(self, repo=None, base_url="https://sho.rt/"):
        self.repo = repo or LinkRepository()
        self.base_url = base_url

    def create(self, owner, url):
        parts = urlsplit(url or "")
        if parts.scheme not in ("http", "https") or not parts.netloc:
            raise InvalidUrl(f"not an http(s) URL: {url!r}")
        if parts.netloc.endswith(urlsplit(self.base_url).netloc):
            raise InvalidUrl("cannot shorten a short link")
        link = Link(codes.encode(self.repo.next_number()), url, owner)
        return self.repo.add(link)

    def resolve(self, code):
        link = self.repo.get(code)
        if link is not None:
            link.hits += 1
        return link

    def short_url(self, link):
        return self.base_url + link.code

    def links_of(self, owner):
        return sorted(self.repo.by_owner(owner), key=lambda link: codes.decode(link.code))
