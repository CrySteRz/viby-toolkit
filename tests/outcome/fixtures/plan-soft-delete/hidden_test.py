import unittest
from datetime import datetime, timedelta, timezone

from notes.api import Api
from notes.errors import Conflict, NotFound
from notes.service import NoteService


class Clock:
    def __init__(self):
        self.now = datetime(2026, 5, 1, 12, 0, tzinfo=timezone.utc)

    def __call__(self):
        return self.now


class HiddenSoftDeleteService(unittest.TestCase):
    def setUp(self):
        self.clock = Clock()
        self.svc = NoteService(clock=self.clock)
        self.a = self.svc.create("Alpha", "first")
        self.b = self.svc.create("Beta", "second")

    def test_delete_keeps_the_note_and_stamps_it(self):
        self.assertIsNone(self.a.deleted_at)
        self.clock.now += timedelta(hours=1)
        self.svc.delete(self.a.id)
        self.assertEqual(self.svc.repo.get(self.a.id).deleted_at, self.clock.now)

    def test_deleted_is_hidden(self):
        self.svc.delete(self.a.id)
        with self.assertRaises(NotFound):
            self.svc.get(self.a.id)
        self.assertEqual([n.title for n in self.svc.list()], ["Beta"])
        self.assertEqual([n.title for n in self.svc.list(include_deleted=True)], ["Alpha", "Beta"])
        self.assertEqual(self.svc.search("first"), [])

    def test_delete_twice_or_unknown(self):
        self.svc.delete(self.a.id)
        with self.assertRaises(NotFound):
            self.svc.delete(self.a.id)
        with self.assertRaises(NotFound):
            self.svc.delete(999)

    def test_restore(self):
        self.svc.delete(self.a.id)
        restored = self.svc.restore(self.a.id)
        self.assertIsNone(restored.deleted_at)
        self.assertEqual(self.svc.get(self.a.id).title, "Alpha")
        with self.assertRaises(Conflict):
            self.svc.restore(self.b.id)
        with self.assertRaises(NotFound):
            self.svc.restore(999)


class HiddenSoftDeleteApi(unittest.TestCase):
    def setUp(self):
        self.clock = Clock()
        self.api = Api(NoteService(clock=self.clock))
        self.id = self.api.handle("POST", "/notes", {"title": "Trip", "body": "tent"})[1]["id"]
        self.api.handle("POST", "/notes", {"title": "Work"})

    def test_delete_then_get(self):
        self.assertEqual(self.api.handle("DELETE", f"/notes/{self.id}"), (204, None))
        self.assertEqual(self.api.handle("GET", f"/notes/{self.id}")[0], 404)
        self.assertEqual(self.api.handle("DELETE", f"/notes/{self.id}")[0], 404)

    def test_list_include_deleted(self):
        self.api.handle("DELETE", f"/notes/{self.id}")
        live = self.api.handle("GET", "/notes")[1]["notes"]
        self.assertEqual([n["title"] for n in live], ["Work"])
        status, body = self.api.handle("GET", "/notes?include_deleted=1")
        self.assertEqual(status, 200)
        by_title = {n["title"]: n for n in body["notes"]}
        self.assertEqual(by_title["Trip"]["deleted_at"], self.clock.now.isoformat())
        self.assertIsNone(by_title["Work"]["deleted_at"])
        self.assertEqual(self.api.handle("GET", "/notes?q=tent")[1]["notes"], [])

    def test_restore_route(self):
        self.assertEqual(self.api.handle("POST", f"/notes/{self.id}/restore")[0], 409)
        self.api.handle("DELETE", f"/notes/{self.id}")
        status, body = self.api.handle("POST", f"/notes/{self.id}/restore")
        self.assertEqual(status, 200)
        self.assertEqual(body["title"], "Trip")
        self.assertIsNone(body["deleted_at"])
        self.assertEqual(self.api.handle("GET", f"/notes/{self.id}")[0], 200)
        self.assertEqual(self.api.handle("POST", "/notes/999/restore")[0], 404)


if __name__ == "__main__":
    unittest.main()
