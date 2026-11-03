import json
import sqlite3

from liberarr.summaries.domain import InvalidSummary, MeetingSummary, parse_summary, timestamp

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


class LlmSummarizer:
    """Any OpenAI-compatible chat endpoint. One retry with the validation error, then InvalidSummary."""

    def __init__(self, client, model: str) -> None:
        self.client = client
        self.model = model

    def summarize(self, segments: list[dict]) -> MeetingSummary:
        lines = "\n".join(f"[{timestamp(s['start'])}] {s['text'].strip()}" for s in segments)
        messages = [{"role": "system", "content": SYSTEM_PROMPT}, {"role": "user", "content": lines}]
        for attempt in range(2):
            resp = self.client.chat.completions.create(
                model=self.model, messages=messages, response_format={"type": "json_object"}
            )
            content = resp.choices[0].message.content or ""
            try:
                return parse_summary(content)
            except InvalidSummary as exc:
                if attempt == 1:
                    raise
                messages = [
                    *messages,
                    {"role": "assistant", "content": content},
                    {"role": "user", "content": f"Réponse invalide : {exc}. Renvoie uniquement le JSON demandé, corrigé."},
                ]
        raise AssertionError("unreachable")


class SqliteSummaryRepository:
    def __init__(self, conn: sqlite3.Connection) -> None:
        self.conn = conn

    def save(self, recording_id: int, s: MeetingSummary, template: str) -> None:
        actions = json.dumps([a.model_dump() for a in s.actions], ensure_ascii=False)
        self.conn.execute(
            "INSERT INTO summaries (recording_id, template, title, summary_md, mindmap, actions_json) VALUES (?, ?, ?, ?, ?, ?) "
            "ON CONFLICT(recording_id) DO UPDATE SET template=excluded.template, title=excluded.title, "
            "summary_md=excluded.summary_md, mindmap=excluded.mindmap, actions_json=excluded.actions_json",
            (recording_id, template, s.title, s.summary_md, s.mindmap, actions),
        )

    def get(self, recording_id: int) -> MeetingSummary | None:
        row = self.conn.execute("SELECT * FROM summaries WHERE recording_id=?", (recording_id,)).fetchone()
        if row is None:
            return None
        return MeetingSummary(
            title=row["title"] or "",
            summary_md=row["summary_md"],
            mindmap=row["mindmap"],
            actions=json.loads(row["actions_json"]),
        )
