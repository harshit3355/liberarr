from typing import Annotated

from fastapi import APIRouter, Depends, HTTPException
from pydantic import BaseModel

from liberarr.auth.api import require_auth
from liberarr.container import Services
from liberarr.shared.http import services
from liberarr.summaries.domain import ActionNotFound

router = APIRouter(prefix="/api/recordings", dependencies=[Depends(require_auth)])


class ActionPatch(BaseModel):
    done: bool


@router.patch("/{rid}/actions/{index}")
def patch_action(rid: int, index: int, body: ActionPatch, svc: Annotated[Services, Depends(services)]) -> list[dict]:
    try:
        return [a.model_dump() for a in svc.toggle_action.run(rid, index, body.done)]
    except ActionNotFound:
        raise HTTPException(status_code=404, detail="action not found") from None
