"""How reminder times are shown to a user."""
from . import clock


def show(when_utc, tz_name):
    local = clock.to_local(when_utc, tz_name)
    return local.strftime("%a %d %b %H:%M ") + local.tzname()


def agenda(rule, firings):
    lines = [rule.label()]
    lines += ["  " + show(f, rule.tz) for f in firings]
    return "\n".join(lines)
