import tempfile
from pathlib import Path

from liberarr.recordings.domain import RecordingNotFound, RecordingRepository
from liberarr.transcription.domain import AudioChunker, Transcriber, TranscriptRepository, merge_chunks


class TranscribeRecording:
    def __init__(
        self,
        recordings: RecordingRepository,
        chunker: AudioChunker,
        transcriber: Transcriber,
        transcripts: TranscriptRepository,
        scratch_dir: Path,
    ) -> None:
        self.recordings, self.chunker, self.transcriber, self.transcripts = recordings, chunker, transcriber, transcripts
        self.scratch_dir = scratch_dir

    def run(self, recording_id: int) -> None:
        rec = self.recordings.get(recording_id)
        if rec is None:
            raise RecordingNotFound(recording_id)
        self.scratch_dir.mkdir(parents=True, exist_ok=True)
        with tempfile.TemporaryDirectory(dir=self.scratch_dir) as tmp:
            results = [
                (self.transcriber.transcribe(chunk), offset)
                for chunk, offset in self.chunker.prepare(rec.audio_path, Path(tmp))
            ]
        self.transcripts.save(recording_id, merge_chunks(results, self.transcriber.engine))
