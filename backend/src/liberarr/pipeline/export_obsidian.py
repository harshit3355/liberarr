import json
import re
import sqlite3
from datetime import datetime
from pathlib import Path

from liberarr.config import Settings
from liberarr.pipeline.summarize import MeetingSummary, _ts

START = "%% liberarr:start %%"
END = "%% liberarr:end %%"
UNSAFE = re.compile(r'[/\\:*?"<>|]')


def render_block(recording: sqlite3.Row, transcript: dict, summary: MeetingSummary) -> str:
    actions = [
        "- [{}] {}".format("x" if a.done else " ", " — ".join(p for p in (a.action, a.owner, a.due) if p))
        for a in summary.actions
    ]
    lines = [f"[{_ts(s['start'])}] {s['text'].strip()}" for s in transcript["segments"]]
    return "\n".join(
        [
            START,
            "## Résumé",
            summary.summary_md.strip(),
            "",
            "## Carte mentale",
            "```mermaid",
            summary.mindmap.strip(),
            "```",
            "",
            "## Actions",
            *(actions or ["_Aucune action._"]),
            "",
            "## Transcription",
            *lines,
            END,
        ]
    )


def upsert_block(existing: str | None, block: str, frontmatter: str) -> str:
    if existing is None:
        return f"{frontmatter}\n{block}\n"
    start, end = existing.find(START), existing.find(END)
    if start != -1 and end > start:
        return existing[:start] + block + existing[end + len(END) :]
    return existing.rstrip("\n") + "\n\n" + block + "\n"


def _frontmatter(recording: sqlite3.Row) -> str:
    return "\n".join(
        [
            "---",
            f"date: {recording['created_at'][:10]}",
            "type: meeting",
            f"source: {recording['source']}",
            f"liberarr_id: {recording['id']}",
            "tags: [liberarr]",
            "---",
        ]
    )


def export_recording(conn: sqlite3.Connection, settings: Settings, recording_id: int) -> Path:
    rec = conn.execute("SELECT * FROM recordings WHERE id=?", (recording_id,)).fetchone()
    t = conn.execute("SELECT data_json FROM transcripts WHERE recording_id=?", (recording_id,)).fetchone()
    s = conn.execute("SELECT * FROM summaries WHERE recording_id=?", (recording_id,)).fetchone()
    summary = MeetingSummary(
        title=s["title"] or rec["title"] or "",
        summary_md=s["summary_md"],
        mindmap=s["mindmap"],
        actions=json.loads(s["actions_json"]),
    )
    if rec["export_path"]:
        path = Path(rec["export_path"])
    else:
        stamp = datetime.fromisoformat(rec["created_at"]).strftime("%Y-%m-%d %H%M")
        title = UNSAFE.sub("", summary.title).strip() or f"recording {recording_id}"
        path = settings.obsidian_path / f"{stamp} - {title}.md"
        conn.execute("UPDATE recordings SET export_path=? WHERE id=?", (str(path), recording_id))
    path.parent.mkdir(parents=True, exist_ok=True)
    existing = path.read_text(encoding="utf-8") if path.exists() else None
    block = render_block(rec, json.loads(t["data_json"]), summary)
    path.write_text(upsert_block(existing, block, _frontmatter(rec)), encoding="utf-8")
    return path
