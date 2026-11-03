import shutil

import pytest

from liberarr.recordings.domain import NotAudioError


def test_ingest_creates_recording_and_queues_transcribe(conn, svc, settings, audio_file):
    src = audio_file(1)
    rid, created = svc.ingest.run(src, "upload")
    assert created
    row = conn.execute("SELECT * FROM recordings WHERE id=?", (rid,)).fetchone()
    assert row["title"] == src.stem
    assert row["duration_s"] == pytest.approx(1.0, abs=0.2)
    assert (settings.audio_dir / f"{rid}.mp3").exists() and not src.exists()
    assert conn.execute("SELECT step, status FROM jobs").fetchone()[:] == ("transcribe", "queued")


def test_ingest_dedups_by_hash(conn, svc, settings, audio_file, tmp_path):
    src = audio_file(1)
    copy = tmp_path / "copy.mp3"
    shutil.copy(src, copy)
    first, _ = svc.ingest.run(src, "upload")
    second, created = svc.ingest.run(copy, "folder")
    assert (second, created) == (first, False)
    assert not copy.exists()
    assert conn.execute("SELECT count(*) FROM recordings").fetchone()[0] == 1


def test_ingest_rejects_non_audio(conn, svc, settings, tmp_path):
    txt = tmp_path / "notes.mp3"
    txt.write_text("hello")
    with pytest.raises(NotAudioError):
        svc.ingest.run(txt, "upload")
    assert conn.execute("SELECT count(*) FROM recordings").fetchone()[0] == 0


def test_inbox_waits_for_stable_size(conn, svc, settings, audio_file):
    shutil.move(audio_file(1), settings.inbox_dir / "meeting.mp3")
    assert svc.inbox.poll() == []
    assert len(svc.inbox.poll()) == 1
    assert list(settings.inbox_dir.glob("*.mp3")) == []


def test_inbox_ignores_growing_file(conn, svc, settings, audio_file):
    target = settings.inbox_dir / "meeting.mp3"
    shutil.move(audio_file(1), target)
    svc.inbox.poll()
    with target.open("ab") as f:
        f.write(b"\0" * 100)
    assert svc.inbox.poll() == []


def test_inbox_moves_non_audio_to_rejected(conn, svc, settings):
    bad = settings.inbox_dir / "bad.mp3"
    bad.write_text("not audio")
    svc.inbox.poll()
    assert svc.inbox.poll() == []
    assert (settings.inbox_dir / "rejected" / "bad.mp3").exists()
