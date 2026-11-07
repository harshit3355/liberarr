from fastapi import APIRouter, Depends, HTTPException, Request, Response
from pydantic import BaseModel

from liberarr.auth.domain import secrets_match


def require_auth(request: Request) -> None:
    settings = request.app.state.settings
    if request.session.get("user"):
        return
    key = request.headers.get("x-api-key")
    if key and secrets_match(key, settings.api_key):
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
    ok_user = secrets_match(creds.username, settings.admin_username)
    ok_pass = secrets_match(creds.password, settings.admin_password)
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
