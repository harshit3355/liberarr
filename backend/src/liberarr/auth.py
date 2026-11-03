import hmac
import time
from collections import defaultdict, deque

from fastapi import APIRouter, Depends, HTTPException, Request, Response
from pydantic import BaseModel

MAX_FAILURES = 5
WINDOW_SECONDS = 15 * 60


class LoginLimiter:
    """In-memory, per client IP. shortcut: resets on restart and is per-process; upgrade if run with several API workers."""

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


def require_auth(request: Request) -> None:
    settings = request.app.state.settings
    if request.session.get("user"):
        return
    key = request.headers.get("x-api-key")
    if key and hmac.compare_digest(key.encode(), settings.api_key.encode()):
        return
    raise HTTPException(status_code=401, detail="not authenticated")


class Credentials(BaseModel):
    username: str
    password: str


router = APIRouter(prefix="/api/auth")


@router.post("/login", status_code=204)
def login(creds: Credentials, request: Request) -> Response:
    settings = request.app.state.settings
    limiter = request.app.state.login_limiter
    ip = request.client.host if request.client else "unknown"
    if limiter.blocked(ip):
        raise HTTPException(status_code=429, detail="too many failed attempts")
    ok_user = hmac.compare_digest(creds.username.encode(), settings.admin_username.encode())
    ok_pass = hmac.compare_digest(creds.password.encode(), settings.admin_password.encode())
    if not (ok_user and ok_pass):
        limiter.fail(ip)
        raise HTTPException(status_code=401, detail="invalid credentials")
    limiter.reset(ip)
    request.session["user"] = settings.admin_username
    return Response(status_code=204)


@router.post("/logout", status_code=204)
def logout(request: Request) -> Response:
    request.session.clear()
    return Response(status_code=204)


@router.get("/me", dependencies=[Depends(require_auth)])
def me(request: Request) -> dict:
    return {"username": request.app.state.settings.admin_username}
