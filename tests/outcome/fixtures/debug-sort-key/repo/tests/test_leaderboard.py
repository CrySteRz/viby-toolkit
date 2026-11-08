import unittest

from leaderboard import load_csv, rank
from leaderboard.loader import LoadError
from leaderboard.render import board
from leaderboard import stats

CSV = """player,score,submitted_at,week
ana,42,1000,2026-W10
ben,57,1100,2026-W10
cy,42,900,2026-W10
ana,48,1200,2026-W10
dee,31,1300,2026-W09
"""


class LoaderTest(unittest.TestCase):
    def test_filters_week(self):
        subs = load_csv(CSV, week="2026-W10")
        self.assertEqual(len(subs), 4)
        self.assertEqual({s.player for s in subs}, {"ana", "ben", "cy"})

    def test_rejects_bad_score(self):
        with self.assertRaises(LoadError):
            load_csv("player,score,submitted_at,week\nana,ten,1,W\n")

    def test_rejects_missing_field(self):
        with self.assertRaises(LoadError):
            load_csv("player,score,submitted_at,week\nana,,1,W\n")


class RankingTest(unittest.TestCase):
    def test_order(self):
        rows = rank(load_csv(CSV, week="2026-W10"))
        self.assertEqual([r.player for r in rows], ["ben", "ana", "cy"])
        self.assertEqual([r.position for r in rows], [1, 2, 3])

    def test_tie_goes_to_earlier(self):
        csv_text = "player,score,submitted_at,week\nana,42,1000,W\ncy,42,900,W\n"
        self.assertEqual([r.player for r in rank(load_csv(csv_text))], ["cy", "ana"])

    def test_limit(self):
        self.assertEqual(len(rank(load_csv(CSV), limit=2)), 2)

    def test_render(self):
        text = board(rank(load_csv(CSV, week="2026-W10")))
        self.assertIn("1. ben", text)
        self.assertIn("57 pts (gold)", text)


class StatsTest(unittest.TestCase):
    def test_participation(self):
        subs = load_csv(CSV)
        self.assertEqual(stats.submissions_per_week(subs), {"2026-W09": 1, "2026-W10": 4})
        self.assertEqual(stats.players_per_week(subs), {"2026-W09": 1, "2026-W10": 3})
        self.assertEqual(stats.most_active(subs, top=2), [("ana", 2), ("ben", 1)])


if __name__ == "__main__":
    unittest.main()
