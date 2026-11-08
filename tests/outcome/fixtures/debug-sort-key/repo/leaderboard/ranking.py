"""Rank players by their best submission. Ties go to whoever got there first."""
from .models import Row


def best_per_player(submissions):
    best = {}
    for sub in submissions:
        current = best.get(sub.player)
        if current is None or (sub.score, -sub.submitted_at) > (current.score, -current.submitted_at):
            best[sub.player] = sub
    return list(best.values())


def rank(submissions, limit=None):
    ordered = sorted(
        best_per_player(submissions),
        key=lambda s: (s.score, -s.submitted_at),
        reverse=True,
    )
    if limit is not None:
        ordered = ordered[:limit]
    return [Row(i, s.player, s.score, s.submitted_at) for i, s in enumerate(ordered, start=1)]
