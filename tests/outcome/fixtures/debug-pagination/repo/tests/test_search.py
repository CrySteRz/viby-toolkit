import unittest

from search import Index, search
from search import paging
from search.client import count_all, iterate_all


def make_index(n, word="report"):
    index = Index()
    for i in range(1, n + 1):
        index.add(i, f"Quarterly {word} {i}", body="numbers and charts " * 3)
    index.add(1000, "Holiday plan", body="beach")
    return index


class IndexTest(unittest.TestCase):
    def test_query_all_words(self):
        index = Index()
        index.add(1, "Red apple")
        index.add(2, "Green apple")
        self.assertEqual([d["id"] for d in index.query("apple")], [1, 2])
        self.assertEqual([d["id"] for d in index.query("green apple")], [2])

    def test_remove(self):
        index = Index()
        index.add(1, "Red apple")
        index.remove(1)
        self.assertEqual(index.query("apple"), [])


class PagingTest(unittest.TestCase):
    def test_two_full_pages(self):
        body = search(make_index(20), "report", page=1, per_page=10)
        self.assertEqual((body["total"], body["total_pages"], body["has_next"]), (20, 2, True))
        last = search(make_index(20), "report", page=2, per_page=10)
        self.assertFalse(last["has_next"])
        self.assertEqual(len(last["results"]), 10)

    def test_single_short_page(self):
        body = search(make_index(7), "report")
        self.assertEqual(len(body["results"]), 7)
        self.assertFalse(body["has_next"])

    def test_per_page_is_clamped(self):
        self.assertEqual(paging.clamp_per_page(500), 50)
        self.assertEqual(paging.clamp_per_page(0), 1)


class ClientTest(unittest.TestCase):
    def test_walks_every_page(self):
        index = make_index(30)
        self.assertEqual([d["id"] for d in iterate_all(index, "report")], list(range(1, 31)))

    def test_count(self):
        self.assertEqual(count_all(make_index(40), "quarterly", per_page=20), 40)


if __name__ == "__main__":
    unittest.main()
