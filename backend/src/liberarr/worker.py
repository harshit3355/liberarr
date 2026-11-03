import logging
import sqlite3
import time
from collections.abc import Callable

from liberarr import db, jobs
from liberarr.ai import get_client
from liberarr.config import Settings, get_settings
from liberarr.jobs import Handler
from liberarr.pipeline.summarize import summarize_recording
from liberarr.pipeline.transcribe import transcribe_recording
from liberarr.sources.folder import InboxWatcher

log = logging.getLogger("liberarr.worker")


def run_once(conn: sqlite3.Connection, handlers: dict[str, Handler]) -> bool:
    job = jobs.claim_next(conn)
    if job is None:
        return False
    try:
        handlers[job.step](conn, job.recording_id)
    except Exception as exc:  # any failure is recorded on the job; prior outputs stay untouched
        log.warning("job %s (%s, recording %s) failed: %s", job.id, job.step, job.recording_id, type(exc).__name__)
        jobs.fail(conn, job.id, f"{type(exc).__name__}: {exc}")
    else:
        jobs.complete(conn, job.id)
    return True


def build_handlers(settings: Settings) -> dict[str, Handler]:
    client = get_client(settings)
    return {
        "transcribe": lambda conn, rid: transcribe_recording(conn, settings, client, rid),
        "summarize": lambda conn, rid: summarize_recording(conn, settings, client, rid),
    }


def build_pollers(settings: Settings) -> list[Callable[[sqlite3.Connection], object]]:
    return [InboxWatcher(settings).poll]


def main() -> None:
    logging.basicConfig(level=logging.INFO)
    settings = get_settings()
    conn = db.connect(settings.db_path)
    db.migrate(conn)
    if n := jobs.reset_stale(conn):
        log.info("requeued %d interrupted job(s)", n)
    handlers, pollers = build_handlers(settings), build_pollers(settings)
    last_poll = 0.0
    while True:
        if time.monotonic() - last_poll >= settings.inbox_poll_seconds:
            for poll in pollers:
                poll(conn)
            last_poll = time.monotonic()
        if not run_once(conn, handlers):
            time.sleep(2)


if __name__ == "__main__":
    main()
