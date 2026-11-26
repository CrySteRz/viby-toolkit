"""Line counts."""
import subprocess


def count_lines(path):
    """Number of lines in the file at path, as `wc -l` reports it."""
    proc = subprocess.run(["wc", "-l", path], capture_output=True, text=True)
    if proc.returncode != 0:
        raise OSError(proc.stderr.strip() or f"wc failed for {path}")
    return int(proc.stdout.split()[0])
