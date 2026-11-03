from liberarr.summaries.domain import Action, ActionNotFound, Summarizer, SummaryRepository
from liberarr.transcription.domain import TranscriptRepository


class SummarizeRecording:
    TEMPLATE = "meeting"

    def __init__(self, transcripts: TranscriptRepository, summarizer: Summarizer, summaries: SummaryRepository) -> None:
        self.transcripts, self.summarizer, self.summaries = transcripts, summarizer, summaries

    def run(self, recording_id: int) -> None:
        transcript = self.transcripts.get(recording_id)
        if transcript is None:
            raise LookupError(f"no transcript for recording {recording_id}")
        self.summaries.save(recording_id, self.summarizer.summarize(transcript.segments), self.TEMPLATE)


class ToggleAction:
    def __init__(self, summaries: SummaryRepository) -> None:
        self.summaries = summaries

    def run(self, recording_id: int, index: int, done: bool) -> list[Action]:
        summary = self.summaries.get(recording_id)
        if summary is None:
            raise ActionNotFound(index)
        updated = summary.with_action_done(index, done)
        self.summaries.save(recording_id, updated, SummarizeRecording.TEMPLATE)
        return updated.actions
