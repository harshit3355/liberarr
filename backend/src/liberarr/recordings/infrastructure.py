import hashlib
import shutil
import sqlite3
import subprocess
from pathlib import Path

from liberarr.recordings.domain import NotAudioError, Recording, display_status


class SqliteRecordingRepository:
    def __init__(self, conn: sqlite3.Connection) -> None:
        self.conn = conn

    def get(self, recording_id: int) -> Recording | None:
        row = self.conn.execute("SELECT * FROM recordings WHERE id=?", (recording_id,)).fetchone()
        if row is None:
            return None
        return Recording(
            id=row["id"],
            title=row["title"],
            source=row["source"],
            source_ref=row["source_ref"],
            audio_path=Path(row["audio_path"]),
            duration_s=row["duration_s"],
            created_at=row["created_at"],
            export_path=Path(row["export_path"]) if row["export_path"] else None,
        )

    def find_by_ref(self, source_ref: str) -> int | None:
        row = self.conn.execute("SELECT id FROM recordings WHERE source_ref=?", (source_ref,)).fetchone()
        return row["id"] if row else None

    def add(self, title: str, source: str, source_ref: str, duration_s: float) -> int:
        return self.conn.execute(
            "INSERT INTO recordings (title, source, source_ref, audio_path, duration_s) VALUES (?, ?, ?, '', ?) RETURNING id",
            (title, source, source_ref, duration_s),
        ).fetchone()[0]

    def set_audio_path(self, recording_id: int, path: Path) -> None:
        self.conn.execute("UPDATE recordings SET audio_path=? WHERE id=?", (str(path), recording_id))

    def set_export_path(self, recording_id: int, path: Path) -> None:
        self.conn.execute("UPDATE recordings SET export_path=? WHERE id=?", (str(path), recording_id))

    # Read model for the UI list/detail: joins the summary title and the latest job.
    _LIST_SQL = """
    SELECT r.id, COALESCE((SELECT s.title FROM summaries s WHERE s.recording_id=r.id), r.title) AS title,
      r.source, r.duration_s, r.created_at,
      (SELECT step FROM jobs j WHERE j.recording_id=r.id ORDER BY j.id DESC LIMIT 1) AS last_step,
      (SELECT status FROM jobs j WHERE j.recording_id=r.id ORDER BY j.id DESC LIMIT 1) AS last_status
    FROM recordings r
    """

    def _item(self, row) -> dict:
        d = dict(row)
        d["status"], d["failed_step"] = display_status(d.pop("last_step"), d.pop("last_status"))
        return d

    def list_items(self) -> list[dict]:
        return [self._item(r) for r in self.conn.execute(self._LIST_SQL + " ORDER BY r.created_at DESC, r.id DESC")]

    def item(self, recording_id: int) -> dict | None:
        row = self.conn.execute(self._LIST_SQL + " WHERE r.id=?", (recording_id,)).fetchone()
        return self._item(row) if row else None


class FfprobeAudioProbe:
    def duration(self, path: Path) -> float:
        r = subprocess.run(
            ["ffprobe", "-v", "error", "-show_entries", "format=duration", "-of", "csv=p=0", str(path)],
            capture_output=True,
            text=True,
        )
        try:
            return float(r.stdout.strip())
        except ValueError:
            raise NotAudioError(path.name) from None


class LocalAudioStorage:
    def __init__(self, audio_dir: Path) -> None:
        self.audio_dir = audio_dir

    def fingerprint(self, src: Path) -> str:
        h = hashlib.sha256()
        with src.open("rb") as f:
            for chunk in iter(lambda: f.read(1 << 20), b""):
                h.update(chunk)
        return h.hexdigest()

    def store(self, src: Path, recording_id: int) -> Path:
        self.audio_dir.mkdir(parents=True, exist_ok=True)
        dest = self.audio_dir / f"{recording_id}{src.suffix.lower() or '.audio'}"
        shutil.move(src, dest)
        return dest

    def discard(self, src: Path) -> None:
        src.unlink(missing_ok=True)
