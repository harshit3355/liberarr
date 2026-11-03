import logging

from liberarr.jobs.domain import JobQueue, StepHandler, next_step, validate_step

log = logging.getLogger("liberarr.jobs")


class JobRunner:
    def __init__(self, queue: JobQueue, handlers: dict[str, StepHandler]) -> None:
        self.queue = queue
        self.handlers = handlers

    def run_once(self) -> bool:
        job = self.queue.claim_next()
        if job is None:
            return False
        try:
            self.handlers[job.step](job.recording_id)
        except Exception as exc:  # recorded on the job; prior outputs stay untouched
            log.warning("job %s (%s, recording %s) failed: %s", job.id, job.step, job.recording_id, type(exc).__name__)
            self.queue.fail(job, f"{type(exc).__name__}: {exc}")
        else:
            self.queue.complete(job, then=next_step(job.step))
        return True


def rerun(queue: JobQueue, recording_id: int, step: str) -> int:
    return queue.enqueue(recording_id, validate_step(step))
