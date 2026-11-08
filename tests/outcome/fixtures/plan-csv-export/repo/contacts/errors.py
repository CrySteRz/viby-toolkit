class NotFound(LookupError):
    """HTTP 404."""


class ValidationError(ValueError):
    """HTTP 400."""
