import unittest
from datetime import datetime, timezone
from zoneinfo import ZoneInfo

from reminders import ReminderStore, Rule, upcoming

UTC = timezone.utc


class HiddenDaylightSaving(unittest.TestCase):
    """A daily 09:00 reminder fires at 09:00 local time on every day, across a clock change."""

    def local_hours(self, rule, firings):
        return [f.astimezone(ZoneInfo(rule.tz)).strftime("%H:%M") for f in firings]

    def test_spring_forward_berlin(self):
        rule = Rule("standup", "Europe/Berlin", 9)
        firings = upcoming(rule, datetime(2026, 3, 27, 0, 0, tzinfo=UTC), 5)  # clocks change 29 March
        self.assertEqual(self.local_hours(rule, firings), ["09:00"] * 5)
        self.assertEqual([f.day for f in firings], [27, 28, 29, 30, 31])

    def test_fall_back_new_york(self):
        rule = Rule("pills", "America/New_York", 8, 15)
        firings = upcoming(rule, datetime(2026, 10, 30, 0, 0, tzinfo=UTC), 4)  # clocks change 1 Nov
        self.assertEqual(self.local_hours(rule, firings), ["08:15"] * 4)

    def test_every_three_days_across_change(self):
        rule = Rule("plants", "Europe/London", 18, every_days=3)
        firings = upcoming(rule, datetime(2026, 3, 26, 0, 0, tzinfo=UTC), 3)
        self.assertEqual(self.local_hours(rule, firings), ["18:00"] * 3)

    def test_store_reschedules_in_local_time(self):
        store = ReminderStore()
        rule = Rule("standup", "Europe/Berlin", 9)
        store.add("ana", rule, datetime(2026, 3, 28, 0, 0, tzinfo=UTC))
        store.due(datetime(2026, 3, 28, 8, 0, tzinfo=UTC))
        self.assertEqual(store.next_fire("ana", "standup"), datetime(2026, 3, 29, 7, 0, tzinfo=UTC))


if __name__ == "__main__":
    unittest.main()
