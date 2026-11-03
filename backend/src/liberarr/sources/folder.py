import logging
import shutil
import sqlite3

from liberarr.config import Settings
from liberarr.ingest import NotAudioError, ingest_file

log = logging.getLogger("liberarr.inbox")

AUDIO_SUFFIXES = {".mp3", ".m4a", ".wav", ".ogg", ".opus", ".webm", ".flac", ".aac"}


class InboxWatcher:
    def __init__(self, settings: Settings) -> None:
        self.settings = settings
        self._sizes: dict[str, int] = {}

    def poll(self, conn: sqlite3.Connection) -> list[int]:
        inbox = self.settings.inbox_dir
        inbox.mkdir(parents=True, exist_ok=True)
        seen: dict[str, int] = {}
        created: list[int] = []
        for path in sorted(inbox.iterdir()):
            if not path.is_file() or path.suffix.lower() not in AUDIO_SUFFIXES:
                continue
            size = path.stat().st_size
            seen[path.name] = size
            if self._sizes.get(path.name) != size:
                continue  # new or still being copied: wait for a stable size
            seen.pop(path.name)
            try:
                rid, is_new = ingest_file(conn, self.settings, path, "folder")
            except NotAudioError:
                rejected = inbox / "rejected"
                rejected.mkdir(exist_ok=True)
                shutil.move(path, rejected / path.name)
                log.warning("inbox: rejected a non-audio file")
                continue
            if is_new:
                created.append(rid)
        self._sizes = seen
        return created
