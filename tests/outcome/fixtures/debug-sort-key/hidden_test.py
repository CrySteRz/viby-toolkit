import unittest

from leaderboard import load_csv, rank

CSV = """player,score,submitted_at,week
ana,98,1000,W
ben,104,1100,W
cy,87,900,W
dee,9,800,W
eli,104,1050,W
ben,99,1200,W
"""


class HiddenNumericRanking(unittest.TestCase):
    def test_three_digit_scores_rank_first(self):
        rows = rank(load_csv(CSV))
        self.assertEqual([r.player for r in rows], ["eli", "ben", "ana", "cy", "dee"])

    def test_best_submission_is_numeric(self):
        rows = {r.player: r.score for r in rank(load_csv(CSV))}
        self.assertEqual(rows["ben"], 104)

    def test_single_digit_below_double_digit(self):
        text = "player,score,submitted_at,week\nana,9,1,W\nben,10,2,W\n"
        self.assertEqual([r.player for r in rank(load_csv(text))], ["ben", "ana"])


if __name__ == "__main__":
    unittest.main()
