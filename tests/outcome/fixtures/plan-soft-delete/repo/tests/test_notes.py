import unittest
from datetime import datetime, timedelta, timezone

from notes.api import Api
from notes.errors import NotFound, ValidationError
from notes.service import NoteService


class Clock:
    def __init__(self):
        self.now = datetime(2026, 5, 1, 12, 0, tzinfo=timezone.utc)

    def __call__(self):
        return self.now


class ServiceTest(unittest.TestCase):
    def setUp(self):
        self.clock = Clock()
        self.svc = NoteService(clock=self.clock)

    def test_create_and_get(self):
        note = self.svc.create(" Groceries ", "milk", ["home"])
        self.assertEqual(self.svc.get(note.id).title, "Groceries")
        self.assertEqual(note.created_at, self.clock.now)

    def test_update_moves_updated_at(self):
        note = self.svc.create("Groceries")
        self.clock.now += timedelta(minutes=5)
        self.svc.update(note.id, body="eggs")
        self.assertEqual(note.updated_at - note.created_at, timedelta(minutes=5))

    def test_validation(self):
        with self.assertRaises(ValidationError):
            self.svc.create("  ")
        with self.assertRaises(ValidationError):
            self.svc.create("x", tags=[""])

    def test_delete_unknown(self):
        with self.assertRaises(NotFound):
            self.svc.delete(99)

    def test_list_by_tag(self):
        self.svc.create("a", tags=["work"])
        self.svc.create("b", tags=["home"])
        self.assertEqual([n.title for n in self.svc.list(tag="work")], ["a"])


class ApiTest(unittest.TestCase):
    def setUp(self):
        self.api = Api(NoteService(clock=Clock()))

    def test_create_list_get(self):
        status, body = self.api.handle("POST", "/notes", {"title": "Trip", "body": "pack"})
        self.assertEqual(status, 201)
        self.assertEqual(self.api.handle("GET", f"/notes/{body['id']}")[1]["title"], "Trip")
        self.assertEqual(len(self.api.handle("GET", "/notes")[1]["notes"]), 1)

    def test_search(self):
        self.api.handle("POST", "/notes", {"title": "Trip", "body": "pack the tent"})
        self.api.handle("POST", "/notes", {"title": "Work"})
        status, body = self.api.handle("GET", "/notes?q=tent")
        self.assertEqual([n["title"] for n in body["notes"]], ["Trip"])

    def test_delete(self):
        _, body = self.api.handle("POST", "/notes", {"title": "Trip"})
        self.assertEqual(self.api.handle("DELETE", f"/notes/{body['id']}"), (204, None))
        self.assertEqual(self.api.handle("GET", f"/notes/{body['id']}")[0], 404)

    def test_bad_request(self):
        self.assertEqual(self.api.handle("POST", "/notes", {"title": ""})[0], 400)
        self.assertEqual(self.api.handle("PUT", "/notes")[0], 405)


if __name__ == "__main__":
    unittest.main()
