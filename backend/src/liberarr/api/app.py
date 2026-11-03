from fastapi import FastAPI

from liberarr import db
from liberarr.config import Settings, get_settings


def create_app(settings: Settings) -> FastAPI:
    app = FastAPI(title="Liberarr")
    app.state.settings = settings
    app.state.conn = db.connect(settings.db_path)
    db.migrate(app.state.conn)

    @app.get("/api/health")
    def health() -> dict:
        return {"status": "ok"}

    return app


def build_app() -> FastAPI:
    """uvicorn --factory entrypoint; reads settings from env at startup, not import."""
    return create_app(get_settings())
