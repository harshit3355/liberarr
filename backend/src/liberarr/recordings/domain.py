from dataclasses import dataclass
from pathlib import Path
from typing import Protocol

AUDIO_SUFFIXES = frozenset({".mp3", ".m4a", ".wav", ".ogg", ".opus", ".webm", ".flac", ".aac"})


class NotAudioError(Exception):
    pass


class RecordingNotFound(LookupError):
    pass


@dataclass(frozen=True)
class Recording:
    id: int
    title: str | None
    source: str
    source_ref: str
    audio_path: Path
    duration_s: float | None
    created_at: str
    export_path: Path | None


STATUS = {"queued": "processing", "running": "processing", "failed": "failed", "done": "done"}


def display_status(last_step: str | None, last_status: str | None) -> tuple[str, str | None]:
    """(status, failed_step) from the latest job: the chain only advances on success."""
    status = STATUS[last_status or "done"]
    return status, last_step if status == "failed" else None


class RecordingRepository(Protocol):
    def get(self, recording_id: int) -> Recording | None: ...
    def find_by_ref(self, source_ref: str) -> int | None: ...
    def add(self, title: str, source: str, source_ref: str, duration_s: float) -> int: ...
    def set_audio_path(self, recording_id: int, path: Path) -> None: ...
    def set_export_path(self, recording_id: int, path: Path) -> None: ...


class AudioProbe(Protocol):
    def duration(self, path: Path) -> float:
        """Raises NotAudioError when the file is not decodable audio."""
        ...


class AudioStorage(Protocol):
    def fingerprint(self, src: Path) -> str: ...
    def store(self, src: Path, recording_id: int) -> Path: ...
    def discard(self, src: Path) -> None: ...
