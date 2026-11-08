"""Text rendering for the board shown in the office channel."""


def board(rows, title="Leaderboard"):
    lines = [title, "=" * len(title)]
    for row in rows:
        medal = {1: " (gold)", 2: " (silver)", 3: " (bronze)"}.get(row.position, "")
        lines.append(f"{row.position:>3}. {row.player:<16}{row.score:>5} pts{medal}")
    return "\n".join(lines)
