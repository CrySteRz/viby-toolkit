"""A small document-sharing backend: public downloads and private attachments."""
from .attachments import open_attachment
from .downloads import read_download

__all__ = ["open_attachment", "read_download"]
