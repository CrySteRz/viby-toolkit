"""HTTP-shaped handler: handle(method, path, body) -> (status, headers, body)."""
import re
from urllib.parse import parse_qs, urlsplit

from .errors import NotFound, ValidationError
from .service import ContactService

JSON = {"Content-Type": "application/json"}
CONTACT = re.compile(r"^/contacts/(\d+)$")
ARCHIVE = re.compile(r"^/contacts/(\d+)/archive$")


class Api:
    def __init__(self, service=None):
        self.service = service or ContactService()

    def handle(self, method, path, body=None):
        url = urlsplit(path)
        query = {k: v[-1] for k, v in parse_qs(url.query).items()}
        try:
            return self._route(method, url.path, query, body or {})
        except NotFound as e:
            return 404, JSON, {"error": str(e)}
        except ValidationError as e:
            return 400, JSON, {"error": str(e)}

    def _route(self, method, path, query, body):
        if path == "/contacts" and method == "GET":
            rows = self.service.list(query.get("include_archived") == "1", query.get("tag"))
            return 200, JSON, {"contacts": [c.to_dict() for c in rows]}
        if path == "/contacts" and method == "POST":
            c = self.service.create(body.get("name"), body.get("email"), body.get("company"), body.get("tags", ()))
            return 201, JSON, c.to_dict()
        m = CONTACT.match(path)
        if m and method == "GET":
            return 200, JSON, self.service.get(int(m.group(1))).to_dict()
        m = ARCHIVE.match(path)
        if m and method == "POST":
            return 200, JSON, self.service.archive(int(m.group(1))).to_dict()
        return 404, JSON, {"error": "no such route"}
