import json
from types import SimpleNamespace

import pytest

from liberarr.container import build_services
from liberarr.summaries.domain import InvalidSummary, parse_summary

VALID = {
    "title": "Point budget Q4",
    "summary_md": "## Contexte\nBudget.",
    "mindmap": "mindmap\n  root((Budget))\n    Q4",
    "actions": [{"action": "Envoyer le chiffrage", "owner": "Julie", "due": "vendredi", "start": 12.5}],
}


class FakeChat:
    def __init__(self, contents):
        self.contents = list(contents)
        self.calls = []

    def create(self, **kwargs):
        self.calls.append(kwargs)
        msg = SimpleNamespace(content=self.contents.pop(0))
        return SimpleNamespace(choices=[SimpleNamespace(message=msg)])


def fake_client(contents):
    chat = FakeChat(contents)
    return SimpleNamespace(chat=SimpleNamespace(completions=chat)), chat


@pytest.fixture
def rid(conn):
    rid = conn.execute(
        "INSERT INTO recordings (title, source, source_ref, audio_path) VALUES ('tone-0', 'upload', 'r', '/x.mp3') RETURNING id"
    ).fetchone()[0]
    data = {"segments": [{"start": 12.5, "end": 14.0, "text": "Julie envoie le chiffrage vendredi"}], "words": []}
    conn.execute(
        "INSERT INTO transcripts (recording_id, engine, text, data_json) VALUES (?, 't', 'x', ?)", (rid, json.dumps(data))
    )
    return rid


def test_parse_summary_rejects_non_mindmap():
    with pytest.raises(InvalidSummary):
        parse_summary(json.dumps({**VALID, "mindmap": "graph TD; A-->B"}))


def test_summarize_stores_summary(conn, settings, rid):
    client, chat = fake_client([json.dumps(VALID)])
    build_services(conn, settings, ai_client=client).summarize.run(rid)
    call = chat.calls[0]
    assert call["model"] == settings.llm_model and call["response_format"] == {"type": "json_object"}
    assert "[00:12] Julie envoie le chiffrage vendredi" in call["messages"][-1]["content"]
    row = conn.execute("SELECT * FROM summaries WHERE recording_id=?", (rid,)).fetchone()
    assert row["template"] == "meeting" and row["mindmap"].startswith("mindmap")
    assert json.loads(row["actions_json"])[0]["done"] is False


def test_summarize_retries_once_then_fails(conn, settings, rid):
    client, chat = fake_client(["not json", json.dumps({**VALID, "mindmap": "nope"})])
    before = conn.execute("SELECT * FROM transcripts").fetchone()[:]
    with pytest.raises(InvalidSummary):
        build_services(conn, settings, ai_client=client).summarize.run(rid)
    assert len(chat.calls) == 2
    assert conn.execute("SELECT * FROM transcripts").fetchone()[:] == before
    assert conn.execute("SELECT count(*) FROM summaries").fetchone()[0] == 0
