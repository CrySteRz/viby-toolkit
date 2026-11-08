import unittest
from datetime import datetime, timezone

from reminders import ReminderStore, Rule, upcoming
from reminders import clock
from reminders.display import agenda

UTC = timezone.utc


class PlannerTest(unittest.TestCase):
    def test_first_is_later_today(self):
        rule = Rule("standup", "Europe/Berlin", 9, 30)
        after = datetime(2026, 1, 12, 6, 0, tzinfo=UTC)  # 07:00 in Berlin
        [first] = upcoming(rule, after, 1)
        self.assertEqual(first, datetime(2026, 1, 12, 8, 30, tzinfo=UTC))

    def test_first_rolls_to_tomorrow(self):
        rule = Rule("standup", "Europe/Berlin", 9, 30)
        after = datetime(2026, 1, 12, 9, 0, tzinfo=UTC)  # 10:00 in Berlin
        [first] = upcoming(rule, after, 1)
        self.assertEqual(first, datetime(2026, 1, 13, 8, 30, tzinfo=UTC))

    def test_daily_in_winter(self):
        rule = Rule("pills", "America/New_York", 8)
        firings = upcoming(rule, datetime(2026, 1, 5, tzinfo=UTC), 5)
        self.assertEqual([f.hour for f in firings], [13] * 5)
        self.assertEqual([f.day for f in firings], [5, 6, 7, 8, 9])

    def test_every_other_day(self):
        rule = Rule("plants", "UTC", 18, every_days=2)
        firings = upcoming(rule, datetime(2026, 2, 1, tzinfo=UTC), 3)
        self.assertEqual([f.day for f in firings], [1, 3, 5])

    def test_rejects_naive(self):
        with self.assertRaises(ValueError):
            upcoming(Rule("x", "UTC", 9), datetime(2026, 1, 1), 1)


class StoreTest(unittest.TestCase):
    def test_due_and_reschedule(self):
        store = ReminderStore()
        rule = Rule("standup", "Europe/Berlin", 9)
        store.add("ana", rule, datetime(2026, 1, 12, 0, 0, tzinfo=UTC))
        self.assertEqual(store.due(datetime(2026, 1, 12, 7, 59, tzinfo=UTC)), [])
        self.assertEqual(store.due(datetime(2026, 1, 12, 8, 0, tzinfo=UTC)), [("ana", rule)])
        self.assertEqual(store.next_fire("ana", "standup"), datetime(2026, 1, 13, 8, 0, tzinfo=UTC))

    def test_agenda_shows_local_time(self):
        rule = Rule("standup", "Europe/Berlin", 9)
        text = agenda(rule, upcoming(rule, datetime(2026, 1, 12, tzinfo=UTC), 2))
        self.assertIn("09:00 CET", text)
        self.assertEqual(clock.to_local(datetime(2026, 1, 12, 8, tzinfo=UTC), "Europe/Berlin").hour, 9)


if __name__ == "__main__":
    unittest.main()
