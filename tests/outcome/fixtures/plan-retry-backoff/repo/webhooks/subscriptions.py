"""Who wants which events. In memory; production keeps the same interface over SQL."""
import itertools
from dataclasses import dataclass


@dataclass
class Subscription:
    id: int
    url: str
    event_types: tuple
    secret: str


class SubscriptionStore:
    def __init__(self):
        self._subs = {}
        self._ids = itertools.count(1)

    def add(self, url, event_types, secret):
        if not url.startswith(("http://", "https://")):
            raise ValueError("url must be http(s)")
        sub = Subscription(next(self._ids), url, tuple(event_types), secret)
        self._subs[sub.id] = sub
        return sub

    def remove(self, sub_id):
        return self._subs.pop(sub_id, None) is not None

    def for_event(self, event_type):
        return [s for s in self._subs.values() if event_type in s.event_types or "*" in s.event_types]
