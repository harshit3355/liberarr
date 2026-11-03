import json
import sqlite3
import subprocess
import tempfile
from pathlib import Path

from liberarr.config import Settings
from liberarr.ingest import probe_duration

# Mono 16 kHz Opus 32 kbps ≈ 14 MB/h, so 20-min chunks stay far below the 25 MB API limit.
FFMPEG_ENCODE = ["-ac", "1", "-ar", "16000", "-c:a", "libopus", "-b:a", "32k"]


def prepare_chunks(src: Path, workdir: Path, chunk_seconds: int = 1200) -> list[tuple[Path, float]]:
    workdir.mkdir(parents=True, exist_ok=True)
    base = ["ffmpeg", "-v", "error", "-y", "-i", str(src), "-vn", *FFMPEG_ENCODE]
    if probe_duration(src) <= chunk_seconds:
        out = workdir / "chunk-000.ogg"
        subprocess.run([*base, str(out)], check=True)
        return [(out, 0.0)]
    subprocess.run(
        [*base, "-f", "segment", "-segment_time", str(chunk_seconds), "-reset_timestamps", "1", str(workdir / "chunk-%03d.ogg")],
        check=True,
    )
    chunks, offset = [], 0.0
    for path in sorted(workdir.glob("chunk-*.ogg")):
        chunks.append((path, offset))
        offset += probe_duration(path)  # real cut points, segment boundaries are not exact
    return chunks


def _shift(items: list[dict], offset: float) -> list[dict]:
    return [{**i, "start": i["start"] + offset, "end": i["end"] + offset} for i in items]


def merge_chunk_results(results: list[tuple[dict, float]]) -> dict:
    return {
        "language": results[0][0].get("language") if results else None,
        "text": " ".join(r.get("text", "").strip() for r, _ in results).strip(),
        "segments": [s for r, o in results for s in _shift(r.get("segments") or [], o)],
        "words": [w for r, o in results for w in _shift(r.get("words") or [], o)],
    }


def transcribe_recording(conn: sqlite3.Connection, settings: Settings, client, recording_id: int) -> None:
    audio_path = Path(conn.execute("SELECT audio_path FROM recordings WHERE id=?", (recording_id,)).fetchone()[0])
    results = []
    with tempfile.TemporaryDirectory(dir=settings.data_dir) as tmp:
        for chunk, offset in prepare_chunks(audio_path, Path(tmp)):
            with chunk.open("rb") as f:
                r = client.audio.transcriptions.create(
                    model=settings.transcribe_model,
                    file=f,
                    response_format="verbose_json",
                    timestamp_granularities=["segment", "word"],
                )
            results.append((r.model_dump(), offset))
    merged = merge_chunk_results(results)
    data = {"segments": merged["segments"], "words": merged["words"]}
    conn.execute(
        "INSERT INTO transcripts (recording_id, engine, language, text, data_json) VALUES (?, ?, ?, ?, ?) "
        "ON CONFLICT(recording_id) DO UPDATE SET engine=excluded.engine, language=excluded.language, "
        "text=excluded.text, data_json=excluded.data_json, created_at=datetime('now')",
        (recording_id, settings.transcribe_model, merged["language"], merged["text"], json.dumps(data, ensure_ascii=False)),
    )
