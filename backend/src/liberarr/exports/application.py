from pathlib import Path

from liberarr.exports.domain import NoteStore, frontmatter, note_filename, render_block, upsert_block
from liberarr.recordings.domain import RecordingNotFound, RecordingRepository
from liberarr.summaries.domain import SummaryRepository
from liberarr.transcription.domain import TranscriptRepository


class ExportToObsidian:
    """One-way export. The note path is fixed on first export so renames never create duplicates."""

    def __init__(
        self,
        recordings: RecordingRepository,
        transcripts: TranscriptRepository,
        summaries: SummaryRepository,
        notes: NoteStore,
        export_dir: Path,
    ) -> None:
        self.recordings, self.transcripts, self.summaries, self.notes = recordings, transcripts, summaries, notes
        self.export_dir = export_dir

    def run(self, recording_id: int) -> Path:
        rec = self.recordings.get(recording_id)
        transcript = self.transcripts.get(recording_id)
        summary = self.summaries.get(recording_id)
        if rec is None or transcript is None or summary is None:
            raise RecordingNotFound(recording_id)
        path = rec.export_path
        if path is None:
            path = self.export_dir / note_filename(rec, summary.title or rec.title or "")
            if self.notes.read(path) is not None:  # same minute + same title: never take over another note
                path = path.with_name(f"{path.stem} ({rec.id}){path.suffix}")
            self.recordings.set_export_path(recording_id, path)
        block = render_block(transcript.segments, summary)
        self.notes.write(path, upsert_block(self.notes.read(path), block, frontmatter(rec)))
        return path
