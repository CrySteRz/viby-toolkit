"""Load submissions from the CSV the quiz app uploads every night."""
import csv
import io

from .models import Submission

REQUIRED = ("player", "score", "submitted_at", "week")


class LoadError(ValueError):
    pass


def _clean(row, line_no):
    missing = [k for k in REQUIRED if not (row.get(k) or "").strip()]
    if missing:
        raise LoadError(f"line {line_no}: missing {', '.join(missing)}")
    score = row["score"].strip()
    if not score.isdigit():
        raise LoadError(f"line {line_no}: score must be a whole number, got {score!r}")
    return Submission(
        player=row["player"].strip(),
        score=score,
        submitted_at=int(row["submitted_at"]),
        week=row["week"].strip(),
    )


def load_csv(text, week=None):
    """Parse the upload. Rows for other weeks are skipped when `week` is given."""
    reader = csv.DictReader(io.StringIO(text))
    out = []
    for line_no, row in enumerate(reader, start=2):
        sub = _clean(row, line_no)
        if week is None or sub.week == week:
            out.append(sub)
    return out
