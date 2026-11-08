"""What a user asked for: a time of day in their own timezone, repeating every N days."""
from dataclasses import dataclass


@dataclass(frozen=True)
class Rule:
    name: str
    tz: str  # IANA name, e.g. "Europe/Berlin"
    hour: int
    minute: int = 0
    every_days: int = 1

    def __post_init__(self):
        if not 0 <= self.hour < 24 or not 0 <= self.minute < 60:
            raise ValueError("time of day out of range")
        if self.every_days < 1:
            raise ValueError("every_days must be at least 1")

    def label(self):
        cadence = "every day" if self.every_days == 1 else f"every {self.every_days} days"
        return f"{self.name}: {cadence} at {self.hour:02d}:{self.minute:02d} ({self.tz})"
