class NotFound(LookupError):
    """The note does not exist (HTTP 404)."""


class ValidationError(ValueError):
    """The request is malformed (HTTP 400)."""
