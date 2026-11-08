"""HTTP-shaped handler: handle(method, path, headers, body) -> (status, headers, body)."""
from .service import InvalidUrl, LinkService


class App:
    def __init__(self, service):
        self.service = service

    def handle(self, method, path, headers=None, body=None):
        headers = headers or {}
        body = body or {}
        if path == "/links":
            user = headers.get("X-User", "").strip()
            if not user:
                return 401, {}, {"error": "X-User header required"}
            if method == "POST":
                try:
                    link = self.service.create(user, body.get("url"))
                except InvalidUrl as e:
                    return 400, {}, {"error": str(e)}
                return 201, {}, {"code": link.code, "short_url": self.service.short_url(link)}
            if method == "GET":
                links = self.service.links_of(user)
                return 200, {}, {"links": [{"code": x.code, "url": x.url, "hits": x.hits} for x in links]}
            return 405, {}, {"error": "method not allowed"}
        if method == "GET" and path.startswith("/") and path.count("/") == 1 and len(path) > 1:
            link = self.service.resolve(path[1:])
            if link is None:
                return 404, {}, {"error": "unknown link"}
            return 302, {"Location": link.url}, None
        return 404, {}, {"error": "no such route"}


def create_app(repo=None):
    return App(LinkService(repo))
