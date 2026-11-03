import sqlite3
from collections.abc import Callable
from dataclasses import dataclass

STEPS: tuple[str, ...] = ("transcribe", "summarize", "export")
MAX_ERROR = 2000

Handler = Callable[[sqlite3.Connection, int], None]


@dataclass
class Job:
    id: int
    recording_id: int
    step: str
    attempts: int


def enqueue(conn: sqlite3.Connection, recording_id: int, step: str) -> int:
    return conn.execute(
        "INSERT INTO jobs (recording_id, step, status) VALUES (?, ?, 'queued') RETURNING id", (recording_id, step)
    ).fetchone()[0]


def claim_next(conn: sqlite3.Connection) -> Job | None:
    conn.execute("BEGIN IMMEDIATE")
    try:
        row = conn.execute(
            "UPDATE jobs SET status='running', attempts=attempts+1, updated_at=datetime('now') "
            "WHERE id = (SELECT id FROM jobs WHERE status='queued' ORDER BY id LIMIT 1) "
            "RETURNING id, recording_id, step, attempts"
        ).fetchone()
        conn.execute("COMMIT")
    except Exception:
        conn.execute("ROLLBACK")
        raise
    return Job(**dict(row)) if row else None


def complete(conn: sqlite3.Connection, job_id: int) -> None:
    row = conn.execute(
        "UPDATE jobs SET status='done', error=NULL, updated_at=datetime('now') WHERE id=? RETURNING recording_id, step",
        (job_id,),
    ).fetchone()
    i = STEPS.index(row["step"])
    if i + 1 < len(STEPS):
        enqueue(conn, row["recording_id"], STEPS[i + 1])


def fail(conn: sqlite3.Connection, job_id: int, error: str) -> None:
    conn.execute(
        "UPDATE jobs SET status='failed', error=?, updated_at=datetime('now') WHERE id=?", (error[:MAX_ERROR], job_id)
    )


def reset_stale(conn: sqlite3.Connection) -> int:
    return conn.execute("UPDATE jobs SET status='queued', updated_at=datetime('now') WHERE status='running'").rowcount


def rerun(conn: sqlite3.Connection, recording_id: int, step: str) -> int:
    if step not in STEPS:
        raise ValueError(f"unknown step: {step}")
    return enqueue(conn, recording_id, step)
