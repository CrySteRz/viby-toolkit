"""The HTTP client seam. Production uses urllib; tests use a scripted fake."""
import urllib.error
import urllib.request
from dataclasses import dataclass, field


@dataclass
class Response:
    status: int
    headers: dict = field(default_factory=dict)
    body: bytes = b""


class UrllibTransport:
    """POST with a timeout. Raises ConnectionError or TimeoutError when there is no HTTP answer."""

    def __init__(self, timeout=10.0):
        self.timeout = timeout

    def post(self, url, body, headers):
        req = urllib.request.Request(url, data=body, headers=headers, method="POST")
        try:
            with urllib.request.urlopen(req, timeout=self.timeout) as resp:
                return Response(resp.status, dict(resp.headers), resp.read())
        except urllib.error.HTTPError as e:
            return Response(e.code, dict(e.headers or {}), e.read() or b"")
        except urllib.error.URLError as e:
            if isinstance(e.reason, TimeoutError):
                raise TimeoutError(str(e.reason)) from e
            raise ConnectionError(str(e.reason)) from e
