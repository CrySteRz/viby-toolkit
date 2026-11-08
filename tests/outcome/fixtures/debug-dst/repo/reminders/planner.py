"""Work out when a rule fires next. All inputs and outputs are aware UTC datetimes."""
from datetime import timedelta

from . import clock


def first_occurrence(rule, after):
    """The first firing strictly after `after`, at the rule's wall-clock time in its zone."""
    local = clock.to_local(after, rule.tz)
    candidate = clock.local_wall(rule.tz, local.year, local.month, local.day, rule.hour, rule.minute)
    if candidate <= local:
        nxt = local.date() + timedelta(days=1)
        candidate = clock.local_wall(rule.tz, nxt.year, nxt.month, nxt.day, rule.hour, rule.minute)
    return clock.to_utc(candidate)


def following(rule, previous):
    """The firing after `previous`, which is itself a firing of this rule."""
    return clock.to_utc(previous) + timedelta(days=rule.every_days)


def upcoming(rule, after, count):
    """The next `count` firings after `after`, in order."""
    out = []
    current = None
    for _ in range(count):
        current = first_occurrence(rule, after) if current is None else following(rule, current)
        out.append(current)
    return out
