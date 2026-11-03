from contextlib import closing

from fastapi import FastAPI
from starlette.middleware.sessions import SessionMiddleware

from liberarr import auth, db
from liberarr.api import recordings
from liberarr.auth import LoginLimiter
from liberarr.config import Settings, get_settings


def create_app(settings: Settings) -> FastAPI:
    app = FastAPI(title="Liberarr")
    app.state.settings = settings
    with closing(db.connect(settings.db_path)) as conn:
        db.migrate(conn)
    app.state.login_limiter = LoginLimiter()
    app.add_middleware(
        SessionMiddleware,
        secret_key=settings.secret_key,
        https_only=settings.cookie_secure,
        same_site="lax",
        session_cookie="liberarr_session",
    )
    app.include_router(auth.router)
    app.include_router(recordings.router)

    @app.get("/api/health")
    def health() -> dict:
        return {"status": "ok"}

    return app


def build_app() -> FastAPI:
    """uvicorn --factory entrypoint; reads settings from env at startup, not import."""
    return create_app(get_settings())
