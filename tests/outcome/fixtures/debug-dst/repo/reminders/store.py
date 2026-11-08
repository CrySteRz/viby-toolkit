"""In-memory reminder store: what is due, and when each reminder fires next."""
from . import clock, planner


class ReminderStore:
    def __init__(self):
        self._rules = {}
        self._next = {}

    def add(self, user, rule, now):
        self._rules[(user, rule.name)] = rule
        self._next[(user, rule.name)] = planner.first_occurrence(rule, now)

    def remove(self, user, name):
        self._rules.pop((user, name), None)
        self._next.pop((user, name), None)

    def next_fire(self, user, name):
        return self._next[(user, name)]

    def due(self, now):
        """Pop everything due at `now` and schedule its next firing. Returns (user, rule) pairs."""
        now = clock.to_utc(now)
        fired = []
        for key, when in sorted(self._next.items(), key=lambda kv: kv[1]):
            if when <= now:
                rule = self._rules[key]
                fired.append((key[0], rule))
                nxt = planner.following(rule, when)
                while nxt <= now:  # catch up after downtime without firing twice
                    nxt = planner.following(rule, nxt)
                self._next[key] = nxt
        return fired
