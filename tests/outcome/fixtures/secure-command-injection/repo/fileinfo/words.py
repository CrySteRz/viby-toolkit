"""Word counts."""
import subprocess


def count_words(path):
    """Number of words in the file at path, as `wc -w` reports it."""
    proc = subprocess.run(f"wc -w {path}", shell=True, capture_output=True, text=True)
    if proc.returncode != 0:
        raise OSError(proc.stderr.strip() or f"wc failed for {path}")
    return int(proc.stdout.split()[0])
