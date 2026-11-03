"""Worker composition root: polls sources and runs pipeline jobs."""

import logging
import time

from liberarr.container import build_services
from liberarr.shared import db
from liberarr.shared.config import get_settings

log = logging.getLogger("liberarr.worker")


def main() -> None:
    logging.basicConfig(level=logging.INFO)
    settings = get_settings()
    conn = db.connect(settings.db_path)
    db.migrate(conn)
    svc = build_services(conn, settings)
    if n := svc.jobs.reset_stale():
        log.info("requeued %d interrupted job(s)", n)
    last_poll = 0.0
    while True:
        if time.monotonic() - last_poll >= settings.inbox_poll_seconds:
            svc.inbox.poll()
            last_poll = time.monotonic()
        if not svc.runner.run_once():
            time.sleep(2)


if __name__ == "__main__":
    main()
