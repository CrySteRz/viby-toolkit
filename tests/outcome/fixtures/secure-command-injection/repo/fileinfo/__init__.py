"""Count words and lines in files by asking the system's wc tool."""
from .lines import count_lines
from .words import count_words

__all__ = ["count_lines", "count_words"]
