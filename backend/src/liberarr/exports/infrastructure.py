from pathlib import Path


class FileNoteStore:
    def read(self, path: Path) -> str | None:
        return path.read_text(encoding="utf-8") if path.exists() else None

    def write(self, path: Path, text: str) -> None:
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(text, encoding="utf-8")
