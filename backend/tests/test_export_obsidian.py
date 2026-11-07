import json

import pytest

from liberarr.exports.domain import END, START, upsert_block


@pytest.fixture
def rid(conn):
    rid = conn.execute(
        "INSERT INTO recordings (title, source, source_ref, audio_path, created_at) "
        "VALUES ('Réunion: budget/Q4', 'upload', 'r', '/x.mp3', '2026-11-03 14:05:00') RETURNING id"
    ).fetchone()[0]
    data = {"segments": [{"start": 65, "end": 70, "text": "On valide."}], "words": []}
    conn.execute("INSERT INTO transcripts (recording_id, engine, text, data_json) VALUES (?, 't', 'x', ?)", (rid, json.dumps(data)))
    actions = [{"action": "Envoyer", "owner": "Julie", "due": None, "start": 65, "done": True}]
    conn.execute(
        "INSERT INTO summaries (recording_id, template, title, summary_md, mindmap, actions_json) "
        "VALUES (?, 'meeting', 'Budget Q4', 'Résumé', 'mindmap\n  root((Budget))', ?)",
        (rid, json.dumps(actions)),
    )
    return rid


def test_upsert_preserves_outside_text():
    note = f"# Mes notes\navant\n{START}\nancien\n{END}\naprès ✍️\n"
    out = upsert_block(note, f"{START}\nnouveau\n{END}", "---\nx: 1\n---\n")
    assert out == f"# Mes notes\navant\n{START}\nnouveau\n{END}\naprès ✍️\n"


def test_upsert_appends_when_no_markers():
    out = upsert_block("perso\n", f"{START}\nb\n{END}", "---\n---\n")
    assert out.startswith("perso\n") and out.rstrip().endswith(END)


def test_export_writes_note_with_block(conn, svc, rid):
    path = svc.export.run(rid)
    text = path.read_text()
    assert path.name == "2026-11-03 1405 - Budget Q4.md"
    assert text.startswith("---\n") and f"liberarr_id: {rid}" in text
    assert "```mermaid\nmindmap" in text and "- [x] Envoyer — Julie" in text and "[01:05] On valide." in text


def test_export_path_stable_and_recreated(conn, svc, rid):
    first = svc.export.run(rid)
    conn.execute("UPDATE summaries SET title='Autre titre'")
    assert svc.export.run(rid) == first
    first.unlink()
    assert svc.export.run(rid) == first and first.exists()


def test_note_uses_local_time(conn, svc, rid, monkeypatch):
    import time
    monkeypatch.setenv("TZ", "Europe/Paris")
    time.tzset()
    path = svc.export.run(rid)  # stored 14:05 UTC = 15:05 in Paris (CET)
    assert path.name.startswith("2026-11-03 1505")


def test_failed_write_keeps_existing_note(tmp_path, monkeypatch):
    import os

    from liberarr.exports.infrastructure import FileNoteStore

    note = tmp_path / "n.md"
    note.write_text("mes notes précieuses")
    monkeypatch.setattr(os, "replace", lambda *a: (_ for _ in ()).throw(OSError("disk full")))
    with pytest.raises(OSError):
        FileNoteStore().write(note, "nouveau contenu")
    assert note.read_text() == "mes notes précieuses"


def test_same_title_same_minute_gets_distinct_notes(conn, svc, rid):
    other = conn.execute(
        "INSERT INTO recordings (title, source, source_ref, audio_path, created_at) "
        "SELECT title, source, 'r2', audio_path, created_at FROM recordings WHERE id=? RETURNING id", (rid,)
    ).fetchone()[0]
    conn.execute("INSERT INTO transcripts SELECT ?, engine, language, text, data_json, created_at FROM transcripts WHERE recording_id=?", (other, rid))
    conn.execute("INSERT INTO summaries SELECT ?, template, title, summary_md, mindmap, actions_json, created_at FROM summaries WHERE recording_id=?", (other, rid))
    a, b = svc.export.run(rid), svc.export.run(other)
    assert a != b and a.exists() and b.exists()


def test_upsert_with_missing_end_marker_keeps_owner_text():
    note = f"intro\n{START}\nancien bloc\nnotes perso du propriétaire\n"  # owner deleted END
    once = upsert_block(note, f"{START}\nv1\n{END}", "---\n---\n")
    twice = upsert_block(once, f"{START}\nv2\n{END}", "---\n---\n")
    assert "notes perso du propriétaire" in twice and "v2" in twice and "v1" not in twice
