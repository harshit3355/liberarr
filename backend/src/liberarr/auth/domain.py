import hmac
import time
from collections import defaultdict, deque

MAX_FAILURES = 5
WINDOW_SECONDS = 15 * 60


def secrets_match(given: str, expected: str) -> bool:
    # bytes: str compare_digest raises TypeError on non-ASCII input
    return hmac.compare_digest(given.encode(), expected.encode())


class LoginLimiter:
    """Per client IP. shortcut: in-memory and per-process; upgrade if the API runs several workers."""

    def __init__(self) -> None:
        self._failures: dict[str, deque[float]] = defaultdict(deque)

    def _recent(self, ip: str) -> deque[float]:
        q = self._failures[ip]
        cutoff = time.monotonic() - WINDOW_SECONDS
        while q and q[0] < cutoff:
            q.popleft()
        return q

    def blocked(self, ip: str) -> bool:
        return len(self._recent(ip)) >= MAX_FAILURES

    def fail(self, ip: str) -> None:
        self._recent(ip).append(time.monotonic())

    def reset(self, ip: str) -> None:
        self._failures.pop(ip, None)
