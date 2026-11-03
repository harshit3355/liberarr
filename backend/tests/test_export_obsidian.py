import json

import pytest

from liberarr.pipeline.export_obsidian import END, START, export_recording, upsert_block


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


def test_export_writes_note_with_block(conn, settings, rid):
    path = export_recording(conn, settings, rid)
    text = path.read_text()
    assert path.name == "2026-11-03 1405 - Budget Q4.md"
    assert text.startswith("---\n") and f"liberarr_id: {rid}" in text
    assert "```mermaid\nmindmap" in text and "- [x] Envoyer — Julie" in text and "[01:05] On valide." in text


def test_export_path_stable_and_recreated(conn, settings, rid):
    first = export_recording(conn, settings, rid)
    conn.execute("UPDATE summaries SET title='Autre titre'")
    assert export_recording(conn, settings, rid) == first
    first.unlink()
    assert export_recording(conn, settings, rid) == first and first.exists()
