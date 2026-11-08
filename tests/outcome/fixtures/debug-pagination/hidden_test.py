import unittest

from search import Index, search
from search.client import count_all, iterate_all


def make_index(n):
    index = Index()
    for i in range(1, n + 1):
        index.add(i, f"Quarterly report {i}")
    return index


class HiddenEveryResultIsReachable(unittest.TestCase):
    def test_partial_last_page(self):
        index = make_index(25)
        self.assertEqual([d["id"] for d in iterate_all(index, "report")], list(range(1, 26)))

    def test_counts_for_many_sizes(self):
        for n in (1, 9, 10, 11, 19, 21, 49, 51):
            with self.subTest(n=n):
                self.assertEqual(count_all(make_index(n), "report", per_page=10), n)

    def test_total_pages_rounds_up(self):
        body = search(make_index(21), "report", page=2, per_page=10)
        self.assertEqual(body["total_pages"], 3)
        self.assertTrue(body["has_next"])
        last = search(make_index(21), "report", page=3, per_page=10)
        self.assertEqual([r["id"] for r in last["results"]], [21])
        self.assertFalse(last["has_next"])

    def test_empty_result(self):
        body = search(make_index(3), "nothing")
        self.assertEqual(body["results"], [])
        self.assertFalse(body["has_next"])


if __name__ == "__main__":
    unittest.main()
