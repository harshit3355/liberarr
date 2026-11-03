import sqlite3

from liberarr.jobs.domain import MAX_ERROR, Job
from liberarr.shared.db import transaction


class SqliteJobQueue:
    def __init__(self, conn: sqlite3.Connection) -> None:
        self.conn = conn

    def enqueue(self, recording_id: int, step: str) -> int:
        return self.conn.execute(
            "INSERT INTO jobs (recording_id, step, status) VALUES (?, ?, 'queued') RETURNING id", (recording_id, step)
        ).fetchone()[0]

    def claim_next(self) -> Job | None:
        with transaction(self.conn):
            row = self.conn.execute(
                "UPDATE jobs SET status='running', attempts=attempts+1, updated_at=datetime('now') "
                "WHERE id = (SELECT id FROM jobs WHERE status='queued' ORDER BY id LIMIT 1) "
                "RETURNING id, recording_id, step, attempts"
            ).fetchone()
        return Job(**dict(row)) if row else None

    def complete(self, job: Job, then: str | None) -> None:
        with transaction(self.conn):
            self.conn.execute("UPDATE jobs SET status='done', error=NULL, updated_at=datetime('now') WHERE id=?", (job.id,))
            if then:
                self.enqueue(job.recording_id, then)

    def fail(self, job: Job, error: str) -> None:
        self.conn.execute(
            "UPDATE jobs SET status='failed', error=?, updated_at=datetime('now') WHERE id=?", (error[:MAX_ERROR], job.id)
        )

    def reset_stale(self) -> int:
        return self.conn.execute(
            "UPDATE jobs SET status='queued', updated_at=datetime('now') WHERE status='running'"
        ).rowcount

    def list_for_recording(self, recording_id: int) -> list[dict]:
        rows = self.conn.execute(
            "SELECT id, step, status, attempts, error, updated_at FROM jobs WHERE recording_id=? ORDER BY id", (recording_id,)
        )
        return [dict(r) for r in rows]
