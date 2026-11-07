from collections.abc import Callable
from dataclasses import dataclass
from typing import Protocol

# The processing pipeline, in order. A step's success enqueues the next one.
STEPS: tuple[str, ...] = ("transcribe", "summarize", "export")
MAX_ERROR = 2000


class UnknownStep(ValueError):
    pass


@dataclass(frozen=True)
class Job:
    id: int
    recording_id: int
    step: str
    attempts: int


def validate_step(step: str) -> str:
    if step not in STEPS:
        raise UnknownStep(step)
    return step


def next_step(step: str) -> str | None:
    i = STEPS.index(step)
    return STEPS[i + 1] if i + 1 < len(STEPS) else None


StepHandler = Callable[[int], None]  # takes a recording id


class JobQueue(Protocol):
    def enqueue(self, recording_id: int, step: str) -> int: ...
    def claim_next(self) -> Job | None: ...
    def complete(self, job: Job, then: str | None) -> None: ...
    def fail(self, job: Job, error: str) -> None: ...
    def reset_stale(self) -> int: ...
    def list_for_recording(self, recording_id: int) -> list[dict]: ...
