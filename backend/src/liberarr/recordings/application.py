from collections.abc import Callable
from contextlib import AbstractContextManager
from pathlib import Path

from liberarr.jobs.domain import STEPS, JobQueue
from liberarr.recordings.domain import AudioProbe, AudioStorage, RecordingRepository


class IngestRecording:
    def __init__(
        self,
        unit_of_work: Callable[[], AbstractContextManager],
        recordings: RecordingRepository,
        jobs: JobQueue,
        probe: AudioProbe,
        storage: AudioStorage,
    ) -> None:
        self.uow, self.recordings, self.jobs, self.probe, self.storage = unit_of_work, recordings, jobs, probe, storage

    def run(self, src: Path, source: str, title: str | None = None) -> tuple[int, bool]:
        """Returns (recording_id, created). Same content twice → same recording, the duplicate file is dropped."""
        ref = self.storage.fingerprint(src)
        if (existing := self.recordings.find_by_ref(ref)) is not None:
            self.storage.discard(src)
            return existing, False
        duration = self.probe.duration(src)
        with self.uow():
            rid = self.recordings.add(title or src.stem, source, ref, duration)
            self.recordings.set_audio_path(rid, self.storage.store(src, rid))
            self.jobs.enqueue(rid, STEPS[0])
        return rid, True
