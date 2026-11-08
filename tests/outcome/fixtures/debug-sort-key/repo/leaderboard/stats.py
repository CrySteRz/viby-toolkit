"""Participation numbers for the weekly e-mail."""
from collections import Counter


def submissions_per_week(submissions):
    return dict(sorted(Counter(s.week for s in submissions).items()))


def players_per_week(submissions):
    seen = {}
    for s in submissions:
        seen.setdefault(s.week, set()).add(s.player)
    return {week: len(players) for week, players in sorted(seen.items())}


def most_active(submissions, top=3):
    """Players with the most attempts; ties broken alphabetically."""
    counts = Counter(s.player for s in submissions)
    return sorted(counts.items(), key=lambda kv: (-kv[1], kv[0]))[:top]
