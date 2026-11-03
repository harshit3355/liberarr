import json
from types import SimpleNamespace

import pytest

from liberarr.container import build_services
from liberarr.recordings.infrastructure import FfprobeAudioProbe
from liberarr.transcription.domain import merge_chunks
from liberarr.transcription.infrastructure import FfmpegChunker

probe_duration = FfprobeAudioProbe().duration


class FakeTranscriptions:
    def __init__(self, responses):
        self.responses = list(responses)
        self.calls = []

    def create(self, **kwargs):
        self.calls.append(kwargs)
        return SimpleNamespace(model_dump=lambda r=self.responses.pop(0): r)


def fake_client(responses):
    t = FakeTranscriptions(responses)
    return SimpleNamespace(audio=SimpleNamespace(transcriptions=t)), t


def resp(text, start=0.0, words=True):
    r = {"language": "french", "text": text, "segments": [{"start": start, "end": start + 1, "text": text}]}
    if words:
        r["words"] = [{"word": text, "start": start, "end": start + 0.5}]
    return r


def test_prepare_chunks_short_audio_single_chunk(audio_file, tmp_path):
    chunks = FfmpegChunker().prepare(audio_file(10), tmp_path / "w")
    assert len(chunks) == 1
    path, offset = chunks[0]
    assert offset == 0.0 and path.suffix == ".ogg"


def test_prepare_chunks_splits_long_audio(audio_file, tmp_path):
    chunks = FfmpegChunker(chunk_seconds=10).prepare(audio_file(25), tmp_path / "w")
    assert len(chunks) == 3
    assert [o for _, o in chunks] == pytest.approx([0, 10, 20], abs=0.5)
    assert sum(probe_duration(p) for p, _ in chunks) == pytest.approx(25, abs=0.5)


def test_merge_offsets_timestamps():
    merged = merge_chunks([(resp("bonjour"), 0.0), (resp("meeting", start=1.0), 1200.0)], "transcribe")
    assert merged.text == "bonjour meeting"
    assert merged.language == "french"
    assert merged.segments[1]["start"] == 1201.0 and merged.segments[1]["end"] == 1202.0
    assert merged.words[1]["start"] == 1201.0


def test_transcribe_stores_transcript_without_language_param(conn, settings, audio_file):
    client, t = fake_client([resp("salut tout le monde")])
    svc = build_services(conn, settings, ai_client=client)
    rid, _ = svc.ingest.run(audio_file(2), "upload")
    svc.transcribe.run(rid)
    call = t.calls[0]
    assert "language" not in call
    assert call["model"] == settings.transcribe_model and call["response_format"] == "verbose_json"
    row = conn.execute("SELECT * FROM transcripts WHERE recording_id=?", (rid,)).fetchone()
    assert row["text"] == "salut tout le monde" and row["engine"] == settings.transcribe_model
    assert json.loads(row["data_json"])["segments"][0]["text"] == "salut tout le monde"


def test_transcribe_tolerates_missing_words(conn, settings, audio_file):
    client, _ = fake_client([resp("ok", words=False)])
    svc = build_services(conn, settings, ai_client=client)
    rid, _ = svc.ingest.run(audio_file(2), "upload")
    svc.transcribe.run(rid)
    data = json.loads(conn.execute("SELECT data_json FROM transcripts").fetchone()[0])
    assert data["words"] == []


def test_transcribe_rerun_replaces_transcript(conn, settings, audio_file):
    client, _ = fake_client([resp("v1"), resp("v2")])
    svc = build_services(conn, settings, ai_client=client)
    rid, _ = svc.ingest.run(audio_file(2), "upload")
    svc.transcribe.run(rid)
    svc.transcribe.run(rid)
    assert conn.execute("SELECT text FROM transcripts").fetchall()[0][0] == "v2"
