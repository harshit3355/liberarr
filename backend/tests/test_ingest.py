import shutil

import pytest

from liberarr.ingest import NotAudioError, ingest_file
from liberarr.sources.folder import InboxWatcher


def test_ingest_creates_recording_and_queues_transcribe(conn, settings, audio_file):
    src = audio_file(1)
    rid, created = ingest_file(conn, settings, src, "upload")
    assert created
    row = conn.execute("SELECT * FROM recordings WHERE id=?", (rid,)).fetchone()
    assert row["title"] == src.stem
    assert row["duration_s"] == pytest.approx(1.0, abs=0.2)
    assert (settings.audio_dir / f"{rid}.mp3").exists() and not src.exists()
    assert conn.execute("SELECT step, status FROM jobs").fetchone()[:] == ("transcribe", "queued")


def test_ingest_dedups_by_hash(conn, settings, audio_file, tmp_path):
    src = audio_file(1)
    copy = tmp_path / "copy.mp3"
    shutil.copy(src, copy)
    first, _ = ingest_file(conn, settings, src, "upload")
    second, created = ingest_file(conn, settings, copy, "folder")
    assert (second, created) == (first, False)
    assert not copy.exists()
    assert conn.execute("SELECT count(*) FROM recordings").fetchone()[0] == 1


def test_ingest_rejects_non_audio(conn, settings, tmp_path):
    txt = tmp_path / "notes.mp3"
    txt.write_text("hello")
    with pytest.raises(NotAudioError):
        ingest_file(conn, settings, txt, "upload")
    assert conn.execute("SELECT count(*) FROM recordings").fetchone()[0] == 0


def test_inbox_waits_for_stable_size(conn, settings, audio_file):
    shutil.move(audio_file(1), settings.inbox_dir / "meeting.mp3")
    watcher = InboxWatcher(settings)
    assert watcher.poll(conn) == []
    assert len(watcher.poll(conn)) == 1
    assert list(settings.inbox_dir.glob("*.mp3")) == []


def test_inbox_ignores_growing_file(conn, settings, audio_file):
    target = settings.inbox_dir / "meeting.mp3"
    shutil.move(audio_file(1), target)
    watcher = InboxWatcher(settings)
    watcher.poll(conn)
    with target.open("ab") as f:
        f.write(b"\0" * 100)
    assert watcher.poll(conn) == []


def test_inbox_moves_non_audio_to_rejected(conn, settings):
    bad = settings.inbox_dir / "bad.mp3"
    bad.write_text("not audio")
    watcher = InboxWatcher(settings)
    watcher.poll(conn)
    assert watcher.poll(conn) == []
    assert (settings.inbox_dir / "rejected" / "bad.mp3").exists()
