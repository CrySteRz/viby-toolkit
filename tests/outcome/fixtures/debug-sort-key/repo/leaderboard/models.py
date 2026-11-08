"""Submission and ranked-row types."""
from dataclasses import dataclass


@dataclass(frozen=True)
class Submission:
    player: str
    score: int
    submitted_at: int  # unix seconds; earlier wins a tie
    week: str


@dataclass(frozen=True)
class Row:
    position: int
    player: str
    score: int
    submitted_at: int
