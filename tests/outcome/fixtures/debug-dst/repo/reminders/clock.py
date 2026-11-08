"""Timezone helpers. Everything persisted is an aware UTC datetime; local time is for people."""
from datetime import datetime, timezone
from zoneinfo import ZoneInfo

UTC = timezone.utc


def zone(name):
    return ZoneInfo(name)


def require_aware(dt):
    if dt.tzinfo is None or dt.utcoffset() is None:
        raise ValueError(f"naive datetime not allowed here: {dt!r}")
    return dt


def to_utc(dt):
    return require_aware(dt).astimezone(UTC)


def to_local(dt, tz_name):
    return require_aware(dt).astimezone(zone(tz_name))


def local_wall(tz_name, year, month, day, hour, minute):
    """The aware datetime for a wall-clock time in a zone (the earlier one if it repeats)."""
    return datetime(year, month, day, hour, minute, tzinfo=zone(tz_name))


def utc_now():
    return datetime.now(UTC)
