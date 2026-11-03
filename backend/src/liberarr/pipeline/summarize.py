import json
import sqlite3

from pydantic import BaseModel, ValidationError, field_validator

from liberarr.config import Settings

SYSTEM_PROMPT = """Tu es l'assistant de compte rendu de réunion. On te donne la transcription horodatée d'un enregistrement \
(français, parfois mêlé d'anglais). Réponds UNIQUEMENT avec un objet JSON de cette forme :
{
  "title": "titre court et explicite",
  "summary_md": "compte rendu en Markdown avec les sections ## Contexte, ## Points clés, ## Décisions",
  "mindmap": "carte mentale Mermaid commençant par la ligne 'mindmap', racine = sujet principal",
  "actions": [{"action": "...", "owner": "personne nommée dans la transcription ou null",
               "due": "échéance seulement si elle est dite, sinon null",
               "start": secondes du segment où l'action est énoncée, ou null}]
}
N'invente rien qui ne soit pas dans la transcription. Rédige en français."""


class Action(BaseModel):
    action: str
    owner: str | None = None
    due: str | None = None
    start: float | None = None
    done: bool = False


class MeetingSummary(BaseModel):
    title: str
    summary_md: str
    mindmap: str
    actions: list[Action] = []

    @field_validator("mindmap")
    @classmethod
    def _is_mindmap(cls, v: str) -> str:
        if not v.strip().startswith("mindmap"):
            raise ValueError("mindmap must be a Mermaid diagram starting with 'mindmap'")
        return v.strip()


def parse_summary(text: str) -> MeetingSummary:
    try:
        return MeetingSummary.model_validate_json(text)
    except ValidationError as exc:
        raise ValueError(str(exc)) from None


def _ts(seconds: float) -> str:
    m, s = divmod(int(seconds), 60)
    return f"{m:02d}:{s:02d}"


def build_messages(transcript: dict) -> list[dict]:
    lines = "\n".join(f"[{_ts(s['start'])}] {s['text'].strip()}" for s in transcript["segments"])
    return [{"role": "system", "content": SYSTEM_PROMPT}, {"role": "user", "content": lines}]


def summarize_recording(conn: sqlite3.Connection, settings: Settings, client, recording_id: int) -> None:
    row = conn.execute("SELECT data_json FROM transcripts WHERE recording_id=?", (recording_id,)).fetchone()
    messages = build_messages(json.loads(row["data_json"]))
    for attempt in range(2):
        resp = client.chat.completions.create(
            model=settings.llm_model, messages=messages, response_format={"type": "json_object"}
        )
        content = resp.choices[0].message.content or ""
        try:
            summary = parse_summary(content)
            break
        except ValueError as exc:
            if attempt == 1:
                raise
            messages = [
                *messages,
                {"role": "assistant", "content": content},
                {"role": "user", "content": f"Réponse invalide : {exc}. Renvoie uniquement le JSON demandé, corrigé."},
            ]
    conn.execute(
        "INSERT INTO summaries (recording_id, template, title, summary_md, mindmap, actions_json) VALUES (?, 'meeting', ?, ?, ?, ?) "
        "ON CONFLICT(recording_id) DO UPDATE SET template=excluded.template, title=excluded.title, "
        "summary_md=excluded.summary_md, mindmap=excluded.mindmap, actions_json=excluded.actions_json, created_at=datetime('now')",
        (
            recording_id,
            summary.title,
            summary.summary_md,
            summary.mindmap,
            json.dumps([a.model_dump() for a in summary.actions], ensure_ascii=False),
        ),
    )
