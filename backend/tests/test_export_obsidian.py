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
