from dataclasses import dataclass, field
from pathlib import Path
from typing import Protocol


@dataclass(frozen=True)
class Transcript:
    engine: str
    language: str | None
    text: str
    segments: list[dict] = field(default_factory=list)  # {"start","end","text"}
    words: list[dict] = field(default_factory=list)  # {"word","start","end"}; empty if the engine gives none


def _shift(items: list[dict], offset: float) -> list[dict]:
    return [{**i, "start": i["start"] + offset, "end": i["end"] + offset} for i in items]


def merge_chunks(results: list[tuple[dict, float]], engine: str) -> Transcript:
    """Join per-chunk engine results; every timestamp is shifted by its chunk's offset in the full recording."""
    return Transcript(
        engine=engine,
        language=results[0][0].get("language") if results else None,
        text=" ".join(r.get("text", "").strip() for r, _ in results).strip(),
        segments=[s for r, o in results for s in _shift(r.get("segments") or [], o)],
        words=[w for r, o in results for w in _shift(r.get("words") or [], o)],
    )


class AudioChunker(Protocol):
    def prepare(self, src: Path, workdir: Path) -> list[tuple[Path, float]]:
        """Encode for the engine and split into (chunk, offset_seconds), in order."""
        ...


class Transcriber(Protocol):
    engine: str

    def transcribe(self, chunk: Path) -> dict:
        """Engine verbose result: {"language","text","segments","words"?}. No language is forced (FR mixed with EN)."""
        ...


class TranscriptRepository(Protocol):
    def save(self, recording_id: int, transcript: Transcript) -> None: ...
    def get(self, recording_id: int) -> Transcript | None: ...
