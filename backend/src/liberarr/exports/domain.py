import re
from datetime import datetime
from pathlib import Path
from typing import Protocol

from liberarr.recordings.domain import Recording
from liberarr.summaries.domain import MeetingSummary, timestamp

# Liberarr owns only the text between these markers; the owner's edits around them are never touched.
START = "%% liberarr:start %%"
END = "%% liberarr:end %%"
_UNSAFE = re.compile(r'[/\\:*?"<>|]')


def note_filename(recording: Recording, title: str) -> str:
    stamp = datetime.fromisoformat(recording.created_at).strftime("%Y-%m-%d %H%M")
    safe = _UNSAFE.sub("", title).strip() or f"recording {recording.id}"
    return f"{stamp} - {safe}.md"


def frontmatter(recording: Recording) -> str:
    return "\n".join(
        [
            "---",
            f"date: {recording.created_at[:10]}",
            "type: meeting",
            f"source: {recording.source}",
            f"liberarr_id: {recording.id}",
            "tags: [liberarr]",
            "---",
        ]
    )


def render_block(segments: list[dict], summary: MeetingSummary) -> str:
    actions = [
        "- [{}] {}".format("x" if a.done else " ", " — ".join(p for p in (a.action, a.owner, a.due) if p))
        for a in summary.actions
    ]
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
            *(f"[{timestamp(s['start'])}] {s['text'].strip()}" for s in segments),
            END,
        ]
    )


def upsert_block(existing: str | None, block: str, front: str) -> str:
    if existing is None:
        return f"{front}\n{block}\n"
    start, end = existing.find(START), existing.find(END)
    if start != -1 and end > start:
        return existing[:start] + block + existing[end + len(END) :]
    return existing.rstrip("\n") + "\n\n" + block + "\n"


class NoteStore(Protocol):
    def read(self, path: Path) -> str | None: ...
    def write(self, path: Path, text: str) -> None: ...
