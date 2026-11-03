import json
import sqlite3
from typing import Annotated
import shutil
import tempfile
from pathlib import Path

from fastapi import APIRouter, Depends, HTTPException, Request, UploadFile
from fastapi.responses import FileResponse, JSONResponse
from pydantic import BaseModel

from liberarr import db, jobs
from liberarr.auth import require_auth
from liberarr.ingest import NotAudioError, ingest_file

router = APIRouter(prefix="/api/recordings", dependencies=[Depends(require_auth)])

STATUS_SQL = """
SELECT r.id, r.title, r.source, r.duration_s, r.created_at,
  (SELECT step || ':' || status FROM jobs j WHERE j.recording_id=r.id ORDER BY j.id DESC LIMIT 1) AS last_job
FROM recordings r
"""
STATUS = {"queued": "processing", "running": "processing", "failed": "failed", "done": "done"}


def _summary_item(row) -> dict:
    """Status comes from the latest job: the chain enqueues the next step only on success."""
    d = dict(row)
    step, _, status = (d.pop("last_job") or ":done").partition(":")
    d["status"] = STATUS[status]
    d["failed_step"] = step if status == "failed" else None
    return d


def _conn(request: Request):
    """One connection per request: sqlite transactions must not interleave across threadpool requests."""
    conn = db.connect(request.app.state.settings.db_path)
    try:
        yield conn
    finally:
        conn.close()


Conn = Annotated[sqlite3.Connection, Depends(_conn)]


def _get_or_404(conn, rid: int):
    row = conn.execute("SELECT * FROM recordings WHERE id=?", (rid,)).fetchone()
    if row is None:
        raise HTTPException(status_code=404, detail="recording not found")
    return row


@router.get("")
def list_recordings(conn: Conn) -> list[dict]:
    rows = conn.execute(STATUS_SQL + " ORDER BY r.created_at DESC, r.id DESC").fetchall()
    return [_summary_item(r) for r in rows]


@router.post("", status_code=201)
def upload(request: Request, conn: Conn, file: UploadFile, title: str | None = None):
    settings = request.app.state.settings
    name = Path(file.filename or "recording").name
    tmp_dir = settings.data_dir / "tmp"
    tmp_dir.mkdir(parents=True, exist_ok=True)
    with tempfile.NamedTemporaryFile(dir=tmp_dir, suffix=Path(name).suffix.lower(), delete=False) as tmp:
        shutil.copyfileobj(file.file, tmp)
    tmp_path = Path(tmp.name)
    try:
        rid, created = ingest_file(conn, settings, tmp_path, "upload", title or Path(name).stem)
    except NotAudioError:
        raise HTTPException(status_code=400, detail="not an audio file") from None
    finally:
        tmp_path.unlink(missing_ok=True)
    return JSONResponse({"id": rid}, status_code=201 if created else 200)


def _with_json(row, column: str, key: str) -> dict | None:
    if row is None:
        return None
    d = dict(row)
    d[key] = json.loads(d.pop(column))
    return d


@router.get("/{rid}")
def get_recording(rid: int, conn: Conn) -> dict:
    _get_or_404(conn, rid)
    body = _summary_item(conn.execute(STATUS_SQL + " WHERE r.id=?", (rid,)).fetchone())
    t = conn.execute("SELECT * FROM transcripts WHERE recording_id=?", (rid,)).fetchone()
    s = conn.execute("SELECT * FROM summaries WHERE recording_id=?", (rid,)).fetchone()
    body["transcript"] = _with_json(t, "data_json", "data")
    body["summary"] = _with_json(s, "actions_json", "actions")
    body["jobs"] = [
        dict(j)
        for j in conn.execute(
            "SELECT id, step, status, attempts, error, updated_at FROM jobs WHERE recording_id=? ORDER BY id", (rid,)
        )
    ]
    return body


@router.get("/{rid}/audio")
def get_audio(rid: int, conn: Conn) -> FileResponse:
    row = _get_or_404(conn, rid)
    return FileResponse(row["audio_path"])


class Rerun(BaseModel):
    step: str


@router.post("/{rid}/rerun", status_code=202)
def rerun(rid: int, body: Rerun, conn: Conn) -> dict:
    _get_or_404(conn, rid)
    try:
        job_id = jobs.rerun(conn, rid, body.step)
    except ValueError:
        raise HTTPException(status_code=400, detail="unknown step") from None
    return {"job_id": job_id}
