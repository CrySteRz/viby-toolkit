"""Attachments stored per ticket under one folder."""
import os


class OutsideRoot(ValueError):
    pass


def open_attachment(root, name):
    """Open an attachment by the name a user typed; refuse anything outside the folder."""
    base = os.path.realpath(root)
    path = os.path.realpath(os.path.join(base, name))
    if os.path.commonpath([base, path]) != base:
        raise OutsideRoot(name)
    return open(path, "rb")
