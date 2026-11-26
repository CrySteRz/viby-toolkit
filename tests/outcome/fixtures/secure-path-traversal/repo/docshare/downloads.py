"""Public downloads served from one folder."""
import os


def list_downloads(root):
    return sorted(n for n in os.listdir(root) if os.path.isfile(os.path.join(root, n)))


def read_download(root, name):
    """Return the bytes of a file the caller asked for by name."""
    path = os.path.join(root, name)
    if not os.path.isfile(path):
        raise FileNotFoundError(name)
    with open(path, "rb") as f:
        return f.read()
