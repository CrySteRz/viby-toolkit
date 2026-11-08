"""HTTP-shaped handler: handle(method, path, body) -> (status, json-ready body or None)."""
import re
from urllib.parse import parse_qs, urlsplit

from .errors import NotFound, ValidationError
from .service import NoteService

NOTE_PATH = re.compile(r"^/notes/(\d+)$")


class Api:
    def __init__(self, service=None):
        self.service = service or NoteService()

    def handle(self, method, path, body=None):
        url = urlsplit(path)
        query = {k: v[-1] for k, v in parse_qs(url.query).items()}
        try:
            return self._route(method, url.path, query, body or {})
        except NotFound as e:
            return 404, {"error": str(e)}
        except ValidationError as e:
            return 400, {"error": str(e)}

    def _route(self, method, path, query, body):
        if path == "/notes":
            if method == "GET":
                if "q" in query:
                    notes = self.service.search(query["q"])
                else:
                    notes = self.service.list(tag=query.get("tag"))
                return 200, {"notes": [n.to_dict() for n in notes]}
            if method == "POST":
                note = self.service.create(body.get("title", ""), body.get("body", ""), body.get("tags", ()))
                return 201, note.to_dict()
            return 405, {"error": "method not allowed"}
        m = NOTE_PATH.match(path)
        if m:
            note_id = int(m.group(1))
            if method == "GET":
                return 200, self.service.get(note_id).to_dict()
            if method == "PATCH":
                note = self.service.update(note_id, body.get("title"), body.get("body"), body.get("tags"))
                return 200, note.to_dict()
            if method == "DELETE":
                self.service.delete(note_id)
                return 204, None
            return 405, {"error": "method not allowed"}
        return 404, {"error": "no such route"}
