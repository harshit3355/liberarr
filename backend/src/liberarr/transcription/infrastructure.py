import json
import sqlite3
import subprocess
from pathlib import Path

from liberarr.recordings.domain import AudioProbe
from liberarr.transcription.domain import Transcript

# Mono 16 kHz Opus 32 kbps ≈ 14 MB/h, so 20-min chunks stay far below the 25 MB API limit.
FFMPEG_ENCODE = ["-ac", "1", "-ar", "16000", "-c:a", "libopus", "-b:a", "32k"]


class FfmpegChunker:
    def __init__(self, probe: AudioProbe, chunk_seconds: int = 1200) -> None:
        self.probe = probe
        self.chunk_seconds = chunk_seconds

    def prepare(self, src: Path, workdir: Path) -> list[tuple[Path, float]]:
        workdir.mkdir(parents=True, exist_ok=True)
        base = ["ffmpeg", "-v", "error", "-y", "-i", str(src), "-vn", *FFMPEG_ENCODE]
        if self.probe.duration(src) <= self.chunk_seconds:
            out = workdir / "chunk-000.ogg"
            subprocess.run([*base, str(out)], check=True)
            return [(out, 0.0)]
        pattern = workdir / "chunk-%03d.ogg"
        subprocess.run(
            [*base, "-f", "segment", "-segment_time", str(self.chunk_seconds), "-reset_timestamps", "1", str(pattern)],
            check=True,
        )
        chunks, offset = [], 0.0
        for path in sorted(workdir.glob("chunk-*.ogg")):
            chunks.append((path, offset))
            offset += self.probe.duration(path)  # real cut points: segment boundaries are not exact
        return chunks


class OpenAITranscriber:
    """Any OpenAI-compatible /audio/transcriptions endpoint (LiteLLM in practice)."""

    def __init__(self, client, model: str) -> None:
        self.client = client
        self.engine = model

    def transcribe(self, chunk: Path) -> dict:
        with chunk.open("rb") as f:
            r = self.client.audio.transcriptions.create(
                model=self.engine,
                file=f,
                response_format="verbose_json",
                timestamp_granularities=["segment", "word"],
            )
        return r.model_dump()


class SqliteTranscriptRepository:
    def __init__(self, conn: sqlite3.Connection) -> None:
        self.conn = conn

    def save(self, recording_id: int, t: Transcript) -> None:
        data = json.dumps({"segments": t.segments, "words": t.words}, ensure_ascii=False)
        self.conn.execute(
            "INSERT INTO transcripts (recording_id, engine, language, text, data_json) VALUES (?, ?, ?, ?, ?) "
            "ON CONFLICT(recording_id) DO UPDATE SET engine=excluded.engine, language=excluded.language, "
            "text=excluded.text, data_json=excluded.data_json, created_at=datetime('now')",
            (recording_id, t.engine, t.language, t.text, data),
        )

    def get(self, recording_id: int) -> Transcript | None:
        row = self.conn.execute("SELECT * FROM transcripts WHERE recording_id=?", (recording_id,)).fetchone()
        if row is None:
            return None
        data = json.loads(row["data_json"])
        return Transcript(row["engine"], row["language"], row["text"], data["segments"], data["words"])
