import logging
import shutil
import time
from pathlib import Path

from liberarr.recordings.application import IngestRecording
from liberarr.recordings.domain import AUDIO_SUFFIXES, NotAudioError

log = logging.getLogger("liberarr.inbox")


class InboxWatcher:
    """Source adapter: watches a folder and feeds stable audio files to IngestRecording."""

    def __init__(self, inbox_dir: Path, ingest: IngestRecording) -> None:
        self.inbox_dir = inbox_dir
        self.ingest = ingest
        self._sizes: dict[str, int] = {}

    def poll(self) -> list[int]:
        self.inbox_dir.mkdir(parents=True, exist_ok=True)
        pending: dict[str, int] = {}
        created: list[int] = []
        for path in sorted(self.inbox_dir.iterdir()):
            if not path.is_file() or path.suffix.lower() not in AUDIO_SUFFIXES:
                continue
            size = path.stat().st_size
            if self._sizes.get(path.name) != size:
                pending[path.name] = size  # new or still being copied: wait for a stable size
                continue
            try:
                rid, is_new = self.ingest.run(path, "folder")
            except NotAudioError:
                rejected = self.inbox_dir / "rejected"
                rejected.mkdir(exist_ok=True)
                target = rejected / path.name
                if target.exists():  # never overwrite an earlier rejected file
                    target = rejected / f"{path.stem}-{int(time.time())}{path.suffix}"
                shutil.move(path, target)
                log.warning("inbox: rejected a non-audio file")
                continue
            if is_new:
                created.append(rid)
        self._sizes = pending
        return created
