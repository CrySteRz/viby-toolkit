import unittest

from shortener import codes
from shortener.api import create_app
from shortener.service import InvalidUrl, LinkService


class CodesTest(unittest.TestCase):
    def test_round_trip(self):
        for n in (0, 1, 61, 62, 1000, 123456789):
            self.assertEqual(codes.decode(codes.encode(n)), n)

    def test_short(self):
        self.assertEqual(codes.encode(61), "Z")
        self.assertEqual(codes.encode(62), "10")


class ServiceTest(unittest.TestCase):
    def test_create_and_resolve(self):
        svc = LinkService()
        link = svc.create("ana", "https://example.com/a")
        self.assertEqual(svc.resolve(link.code).url, "https://example.com/a")
        self.assertEqual(link.hits, 1)

    def test_rejects(self):
        svc = LinkService()
        for bad in ("ftp://x.org", "example.com", "", "https://sho.rt/abc"):
            with self.assertRaises(InvalidUrl):
                svc.create("ana", bad)


class ApiTest(unittest.TestCase):
    def test_create_redirect_list(self):
        app = create_app()
        status, _h, body = app.handle("POST", "/links", {"X-User": "ana"}, {"url": "https://example.com/x"})
        self.assertEqual(status, 201)
        self.assertTrue(body["short_url"].startswith("https://sho.rt/"))
        status, headers, _ = app.handle("GET", "/" + body["code"])
        self.assertEqual((status, headers["Location"]), (302, "https://example.com/x"))
        listed = app.handle("GET", "/links", {"X-User": "ana"})[2]["links"]
        self.assertEqual(listed[0]["hits"], 1)

    def test_errors(self):
        app = create_app()
        self.assertEqual(app.handle("POST", "/links", {}, {"url": "https://e.com"})[0], 401)
        self.assertEqual(app.handle("POST", "/links", {"X-User": "a"}, {"url": "nope"})[0], 400)
        self.assertEqual(app.handle("GET", "/zzzz")[0], 404)

    def test_several_links(self):
        app = create_app()
        for i in range(4):
            self.assertEqual(app.handle("POST", "/links", {"X-User": "ben"}, {"url": f"https://e.com/{i}"})[0], 201)


if __name__ == "__main__":
    unittest.main()
