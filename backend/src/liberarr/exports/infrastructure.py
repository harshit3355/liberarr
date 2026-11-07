import os
from pathlib import Path


class FileNoteStore:
    def read(self, path: Path) -> str | None:
        return path.read_text(encoding="utf-8") if path.exists() else None

    def write(self, path: Path, text: str) -> None:
        path.parent.mkdir(parents=True, exist_ok=True)
        tmp = path.with_name(path.name + ".tmp")
        try:
            tmp.write_text(text, encoding="utf-8")
            os.replace(tmp, path)  # atomic: a crash never leaves a truncated note (owner text would be lost)
        finally:
            tmp.unlink(missing_ok=True)
