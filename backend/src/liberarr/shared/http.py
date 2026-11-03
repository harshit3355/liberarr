from collections.abc import Iterator

from fastapi import Request

from liberarr.container import Services, build_services
from liberarr.shared import db


def services(request: Request) -> Iterator[Services]:
    """One sqlite connection per request: transactions must not interleave across threadpool requests."""
    conn = db.connect(request.app.state.settings.db_path)
    try:
        yield build_services(conn, request.app.state.settings)
    finally:
        conn.close()
