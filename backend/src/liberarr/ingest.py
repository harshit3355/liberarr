import hashlib
import shutil
import sqlite3
import subprocess
from pathlib import Path

from liberarr import jobs
from liberarr.config import Settings


class NotAudioError(Exception):
    pass


def probe_duration(path: Path) -> float:
    r = subprocess.run(
        ["ffprobe", "-v", "error", "-show_entries", "format=duration", "-of", "csv=p=0", str(path)],
        capture_output=True,
        text=True,
    )
    try:
        return float(r.stdout.strip())
    except ValueError:
        raise NotAudioError(path.name) from None


def _sha256(path: Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def ingest_file(
    conn: sqlite3.Connection, settings: Settings, src: Path, source: str, title: str | None = None
) -> tuple[int, bool]:
    ref = _sha256(src)
    existing = conn.execute("SELECT id FROM recordings WHERE source_ref=?", (ref,)).fetchone()
    if existing:
        src.unlink()
        return existing["id"], False
    duration = probe_duration(src)
    suffix = src.suffix.lower() or ".audio"
    settings.audio_dir.mkdir(parents=True, exist_ok=True)
    conn.execute("BEGIN IMMEDIATE")
    try:
        rid = conn.execute(
            "INSERT INTO recordings (title, source, source_ref, audio_path, duration_s) VALUES (?, ?, ?, '', ?) RETURNING id",
            (title or src.stem, source, ref, duration),
        ).fetchone()[0]
        dest = settings.audio_dir / f"{rid}{suffix}"
        conn.execute("UPDATE recordings SET audio_path=? WHERE id=?", (str(dest), rid))
        shutil.move(src, dest)
        jobs.enqueue(conn, rid, "transcribe")
        conn.execute("COMMIT")
    except BaseException:
        conn.execute("ROLLBACK")
        raise
    return rid, True
