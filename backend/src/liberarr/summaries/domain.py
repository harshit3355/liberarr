from typing import Protocol

from pydantic import BaseModel, ValidationError, field_validator


class InvalidSummary(ValueError):
    pass


class ActionNotFound(LookupError):
    pass


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

    def with_action_done(self, index: int, done: bool) -> "MeetingSummary":
        if not 0 <= index < len(self.actions):
            raise ActionNotFound(index)
        actions = [a.model_copy(update={"done": done}) if i == index else a for i, a in enumerate(self.actions)]
        return self.model_copy(update={"actions": actions})


def parse_summary(text: str) -> MeetingSummary:
    """LLM output is untrusted: it must be the exact JSON shape with a Mermaid mindmap."""
    try:
        return MeetingSummary.model_validate_json(text)
    except ValidationError as exc:
        raise InvalidSummary(str(exc)) from None


def timestamp(seconds: float) -> str:
    m, s = divmod(int(seconds), 60)
    return f"{m:02d}:{s:02d}"


class Summarizer(Protocol):
    def summarize(self, segments: list[dict]) -> MeetingSummary:
        """Raises InvalidSummary when the model cannot produce a valid summary."""
        ...


class SummaryRepository(Protocol):
    def save(self, recording_id: int, summary: MeetingSummary, template: str) -> None: ...
    def get(self, recording_id: int) -> MeetingSummary | None: ...
