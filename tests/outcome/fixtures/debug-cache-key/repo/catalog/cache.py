"""A small TTL cache. Price lookups hit three tables, so the storefront caches them for a minute."""
import time


class TTLCache:
    def __init__(self, ttl_seconds=60.0, clock=time.monotonic, max_entries=10_000):
        self.ttl = ttl_seconds
        self.clock = clock
        self.max_entries = max_entries
        self._data = {}
        self.hits = 0
        self.misses = 0

    def get(self, key):
        entry = self._data.get(key)
        if entry is None:
            return None
        value, expires = entry
        if self.clock() >= expires:
            del self._data[key]
            return None
        return value

    def set(self, key, value):
        if len(self._data) >= self.max_entries:
            self._data.pop(next(iter(self._data)))  # drop the oldest insertion
        self._data[key] = (value, self.clock() + self.ttl)

    def get_or_compute(self, key, compute):
        value = self.get(key)
        if value is not None:
            self.hits += 1
            return value
        self.misses += 1
        value = compute()
        self.set(key, value)
        return value

    def clear(self):
        self._data.clear()
