import csv
import io
import unittest
from datetime import datetime, timezone

from contacts.api import Api
from contacts.service import ContactService


def fixed_clock():
    return datetime(2026, 4, 2, 9, 30, tzinfo=timezone.utc)


def seeded():
    svc = ContactService(clock=fixed_clock)
    svc.create("Ana Pop", "ana@example.com", "Acme", ["lead", "vip"])
    svc.create("Doe, Jane", "jane@example.com", 'The "Best" Shop')
    svc.create("Line Break", "lb@example.com", "Two\nLines")
    svc.create("=HYPERLINK(\"http://x\")", "evil@example.com", "+cmd", ["@risk"])
    svc.create("Zoë Ölund", "zoe@example.com", "")
    svc.archive(svc.create("Old Contact", "old@example.com").id)
    return svc


class HiddenCsvExport(unittest.TestCase):
    def test_exact_text(self):
        text = seeded().export_csv()
        expected = (
            "id,name,email,company,tags,created_at\r\n"
            "1,Ana Pop,ana@example.com,Acme,lead;vip,2026-04-02\r\n"
            '2,"Doe, Jane",jane@example.com,"The ""Best"" Shop",,2026-04-02\r\n'
            '3,Line Break,lb@example.com,"Two\nLines",,2026-04-02\r\n'
            "4,\"'=HYPERLINK(\"\"http://x\"\")\",evil@example.com,'+cmd,'@risk,2026-04-02\r\n"
            "5,Zoë Ölund,zoe@example.com,,,2026-04-02\r\n"
        )
        self.assertEqual(text, expected)

    def test_round_trips_through_a_csv_reader(self):
        rows = list(csv.reader(io.StringIO(seeded().export_csv(), newline="")))
        self.assertEqual(rows[0], ["id", "name", "email", "company", "tags", "created_at"])
        self.assertEqual(rows[2][1], "Doe, Jane")
        self.assertEqual(rows[2][3], 'The "Best" Shop')
        self.assertEqual(rows[3][3], "Two\nLines")
        self.assertEqual(len(rows), 6)

    def test_include_archived(self):
        rows = list(csv.reader(io.StringIO(seeded().export_csv(include_archived=True), newline="")))
        self.assertEqual(rows[-1][1], "Old Contact")
        self.assertEqual(len(rows), 7)

    def test_empty_export_has_header(self):
        self.assertEqual(ContactService().export_csv(), "id,name,email,company,tags,created_at\r\n")

    def test_api_route(self):
        api = Api(seeded())
        status, headers, body = api.handle("GET", "/contacts/export.csv")
        self.assertEqual(status, 200)
        self.assertEqual(headers["Content-Type"], "text/csv; charset=utf-8")
        self.assertEqual(headers["Content-Disposition"], 'attachment; filename="contacts.csv"')
        self.assertEqual(body, seeded().export_csv())
        _, _, all_rows = api.handle("GET", "/contacts/export.csv?include_archived=1")
        self.assertIn("Old Contact", all_rows)
        self.assertNotIn("Old Contact", body)

    def test_existing_routes_unchanged(self):
        api = Api(seeded())
        self.assertEqual(api.handle("GET", "/contacts/2")[2]["name"], "Doe, Jane")


if __name__ == "__main__":
    unittest.main()
