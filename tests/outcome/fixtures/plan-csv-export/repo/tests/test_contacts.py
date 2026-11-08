import unittest
from datetime import datetime, timezone

from contacts.api import Api
from contacts.errors import NotFound, ValidationError
from contacts.service import ContactService


def fixed_clock():
    return datetime(2026, 4, 2, 9, 30, tzinfo=timezone.utc)


class ServiceTest(unittest.TestCase):
    def setUp(self):
        self.svc = ContactService(clock=fixed_clock)

    def test_create(self):
        c = self.svc.create(" Ana Pop ", "ana@example.com", "Acme", ["lead"])
        self.assertEqual((c.id, c.name, c.created_at), (1, "Ana Pop", fixed_clock()))

    def test_validation(self):
        with self.assertRaises(ValidationError):
            self.svc.create("", "a@example.com")
        with self.assertRaises(ValidationError):
            self.svc.create("Ana", "not-an-email")
        self.svc.create("Ana", "a@example.com")
        with self.assertRaises(ValidationError):
            self.svc.create("Other", "A@example.com")

    def test_archive_hides(self):
        c = self.svc.create("Ana", "a@example.com")
        self.svc.create("Ben", "b@example.com", tags=["vip"])
        self.svc.archive(c.id)
        self.assertEqual([x.name for x in self.svc.list()], ["Ben"])
        self.assertEqual(len(self.svc.list(include_archived=True)), 2)
        self.assertEqual([x.name for x in self.svc.list(tag="vip")], ["Ben"])

    def test_get_unknown(self):
        with self.assertRaises(NotFound):
            self.svc.get(5)


class ApiTest(unittest.TestCase):
    def setUp(self):
        self.api = Api(ContactService(clock=fixed_clock))

    def test_round_trip(self):
        status, headers, body = self.api.handle("POST", "/contacts", {"name": "Ana", "email": "a@example.com"})
        self.assertEqual((status, headers["Content-Type"]), (201, "application/json"))
        self.assertEqual(self.api.handle("GET", "/contacts/1")[2]["name"], "Ana")
        self.assertEqual(len(self.api.handle("GET", "/contacts")[2]["contacts"]), 1)

    def test_archive_route(self):
        self.api.handle("POST", "/contacts", {"name": "Ana", "email": "a@example.com"})
        self.assertTrue(self.api.handle("POST", "/contacts/1/archive")[2]["archived"])
        self.assertEqual(self.api.handle("GET", "/contacts")[2]["contacts"], [])
        self.assertEqual(len(self.api.handle("GET", "/contacts?include_archived=1")[2]["contacts"]), 1)

    def test_errors(self):
        self.assertEqual(self.api.handle("GET", "/contacts/9")[0], 404)
        self.assertEqual(self.api.handle("POST", "/contacts", {"name": "x", "email": "y"})[0], 400)


if __name__ == "__main__":
    unittest.main()
