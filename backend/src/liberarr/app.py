"""HTTP composition root."""

from contextlib import closing

from fastapi import FastAPI
from starlette.exceptions import HTTPException as StarletteHTTPException
from starlette.middleware.sessions import SessionMiddleware
from starlette.staticfiles import StaticFiles

from liberarr.auth import api as auth_api
from liberarr.auth.domain import LoginLimiter
from liberarr.recordings import api as recordings_api
from liberarr.shared import db
from liberarr.shared.config import Settings, get_settings
from liberarr.summaries import api as summaries_api


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
    for module in (auth_api, recordings_api, summaries_api):
        app.include_router(module.router)

    @app.get("/api/health")
    def health() -> dict:
        return {"status": "ok"}

    if settings.static_dir:
        app.mount("/", SpaStaticFiles(directory=settings.static_dir, html=True), name="ui")
    return app


class SpaStaticFiles(StaticFiles):
    """Serve the built UI; unknown non-API paths fall back to index.html for client-side routing."""

    async def get_response(self, path, scope):
        try:
            return await super().get_response(path, scope)
        except StarletteHTTPException as exc:
            if exc.status_code != 404 or path.startswith("api/"):
                raise
            return await super().get_response("index.html", scope)


def build_app() -> FastAPI:
    """uvicorn --factory entrypoint; reads settings from env at startup, not import."""
    return create_app(get_settings())
