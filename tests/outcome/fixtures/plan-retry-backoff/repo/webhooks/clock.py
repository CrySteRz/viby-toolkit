"""Time, behind an object so tests never really sleep."""
import time


class SystemClock:
    def now(self):
        return time.time()

    def sleep(self, seconds):
        time.sleep(seconds)
