import sqlite3
from collections.abc import Iterator
from contextlib import contextmanager
from pathlib import Path

MIGRATIONS: list[str] = [
    """
    CREATE TABLE recordings (
      id INTEGER PRIMARY KEY,
      title TEXT,
      source TEXT NOT NULL,
      source_ref TEXT NOT NULL UNIQUE,
      audio_path TEXT NOT NULL,
      duration_s REAL,
      preset_type TEXT,
      export_path TEXT,
      created_at TEXT NOT NULL DEFAULT (datetime('now'))
    );
    CREATE TABLE transcripts (
      recording_id INTEGER PRIMARY KEY REFERENCES recordings(id) ON DELETE CASCADE,
      engine TEXT NOT NULL, language TEXT, text TEXT NOT NULL,
      data_json TEXT NOT NULL,
      created_at TEXT NOT NULL DEFAULT (datetime('now'))
    );
    CREATE TABLE summaries (
      recording_id INTEGER PRIMARY KEY REFERENCES recordings(id) ON DELETE CASCADE,
      template TEXT NOT NULL, title TEXT, summary_md TEXT NOT NULL,
      mindmap TEXT NOT NULL, actions_json TEXT NOT NULL,
      created_at TEXT NOT NULL DEFAULT (datetime('now'))
    );
    CREATE TABLE jobs (
      id INTEGER PRIMARY KEY,
      recording_id INTEGER NOT NULL REFERENCES recordings(id) ON DELETE CASCADE,
      step TEXT NOT NULL,
      status TEXT NOT NULL CHECK (status IN ('queued','running','done','failed')),
      attempts INTEGER NOT NULL DEFAULT 0,
      error TEXT,
      created_at TEXT NOT NULL DEFAULT (datetime('now')),
      updated_at TEXT NOT NULL DEFAULT (datetime('now'))
    );
    CREATE INDEX jobs_status ON jobs(status, id);
    """,
]


def connect(path: Path) -> sqlite3.Connection:
    path.parent.mkdir(parents=True, exist_ok=True)
    conn = sqlite3.connect(path, check_same_thread=False, isolation_level=None)
    conn.row_factory = sqlite3.Row
    conn.execute("PRAGMA journal_mode=WAL")
    conn.execute("PRAGMA foreign_keys=ON")
    conn.execute("PRAGMA busy_timeout=5000")
    return conn


def migrate(conn: sqlite3.Connection) -> None:
    current = conn.execute("PRAGMA user_version").fetchone()[0]
    for version, sql in enumerate(MIGRATIONS[current:], start=current + 1):
        conn.executescript(f"BEGIN;\n{sql}\nPRAGMA user_version = {version};\nCOMMIT;")


@contextmanager
def transaction(conn: sqlite3.Connection) -> Iterator[None]:
    """Unit of work: everything inside commits together or not at all."""
    conn.execute("BEGIN IMMEDIATE")
    try:
        yield
    except BaseException:
        conn.execute("ROLLBACK")
        raise
    conn.execute("COMMIT")
