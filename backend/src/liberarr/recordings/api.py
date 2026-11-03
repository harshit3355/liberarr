import shutil
import tempfile
from pathlib import Path
from typing import Annotated

from fastapi import APIRouter, Depends, HTTPException, Request, UploadFile
from fastapi.responses import FileResponse, JSONResponse
from pydantic import BaseModel

from liberarr.auth.api import require_auth
from liberarr.container import Services
from liberarr.jobs.application import rerun
from liberarr.jobs.domain import UnknownStep
from liberarr.recordings.domain import NotAudioError
from liberarr.shared.http import services

router = APIRouter(prefix="/api/recordings", dependencies=[Depends(require_auth)])
Svc = Annotated[Services, Depends(services)]


@router.get("")
def list_recordings(svc: Svc) -> list[dict]:
    return svc.recordings.list_items()


@router.post("", status_code=201)
def upload(request: Request, svc: Svc, file: UploadFile, title: str | None = None):
    settings = request.app.state.settings
    name = Path(file.filename or "recording").name
    tmp_dir = settings.data_dir / "tmp"
    tmp_dir.mkdir(parents=True, exist_ok=True)
    with tempfile.NamedTemporaryFile(dir=tmp_dir, suffix=Path(name).suffix.lower(), delete=False) as tmp:
        shutil.copyfileobj(file.file, tmp)
    tmp_path = Path(tmp.name)
    try:
        rid, created = svc.ingest.run(tmp_path, "upload", title or Path(name).stem)
    except NotAudioError:
        raise HTTPException(status_code=400, detail="not an audio file") from None
    finally:
        tmp_path.unlink(missing_ok=True)
    return JSONResponse({"id": rid}, status_code=201 if created else 200)


@router.get("/{rid}")
def get_recording(rid: int, svc: Svc) -> dict:
    body = svc.recordings.item(rid)
    if body is None:
        raise HTTPException(status_code=404, detail="recording not found")
    t = svc.transcripts.get(rid)
    s = svc.summaries.get(rid)
    body["transcript"] = (
        {"engine": t.engine, "language": t.language, "text": t.text, "data": {"segments": t.segments, "words": t.words}}
        if t
        else None
    )
    body["summary"] = {"template": "meeting", **s.model_dump()} if s else None
    body["jobs"] = svc.jobs.list_for_recording(rid)
    return body


@router.get("/{rid}/audio")
def get_audio(rid: int, svc: Svc) -> FileResponse:
    rec = svc.recordings.get(rid)
    if rec is None:
        raise HTTPException(status_code=404, detail="recording not found")
    return FileResponse(rec.audio_path)


class Rerun(BaseModel):
    step: str


@router.post("/{rid}/rerun", status_code=202)
def rerun_step(rid: int, body: Rerun, svc: Svc) -> dict:
    if svc.recordings.get(rid) is None:
        raise HTTPException(status_code=404, detail="recording not found")
    try:
        return {"job_id": rerun(svc.jobs, rid, body.step)}
    except UnknownStep:
        raise HTTPException(status_code=400, detail="unknown step") from None

