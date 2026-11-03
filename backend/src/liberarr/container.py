"""Composition root: wires each bounded context's use cases to their infrastructure adapters."""

import sqlite3
from dataclasses import dataclass
from functools import partial

from liberarr.exports.application import ExportToObsidian
from liberarr.exports.infrastructure import FileNoteStore
from liberarr.jobs.application import JobRunner
from liberarr.jobs.infrastructure import SqliteJobQueue
from liberarr.recordings.application import IngestRecording
from liberarr.recordings.infrastructure import FfprobeAudioProbe, LocalAudioStorage, SqliteRecordingRepository
from liberarr.shared.ai import get_client
from liberarr.shared.config import Settings
from liberarr.shared.db import transaction
from liberarr.sources.inbox import InboxWatcher
from liberarr.summaries.application import SummarizeRecording, ToggleAction
from liberarr.summaries.infrastructure import LlmSummarizer, SqliteSummaryRepository
from liberarr.transcription.application import TranscribeRecording
from liberarr.transcription.infrastructure import FfmpegChunker, OpenAITranscriber, SqliteTranscriptRepository


@dataclass
class Services:
    recordings: SqliteRecordingRepository
    jobs: SqliteJobQueue
    transcripts: SqliteTranscriptRepository
    summaries: SqliteSummaryRepository
    ingest: IngestRecording
    transcribe: TranscribeRecording
    summarize: SummarizeRecording
    toggle_action: ToggleAction
    export: ExportToObsidian
    runner: JobRunner
    inbox: InboxWatcher


def build_services(conn: sqlite3.Connection, settings: Settings, ai_client=None) -> Services:
    client = ai_client or get_client(settings)  # building the client does no network I/O
    recordings = SqliteRecordingRepository(conn)
    jobs = SqliteJobQueue(conn)
    transcripts = SqliteTranscriptRepository(conn)
    summaries = SqliteSummaryRepository(conn)
    ingest = IngestRecording(
        partial(transaction, conn), recordings, jobs, FfprobeAudioProbe(), LocalAudioStorage(settings.audio_dir)
    )
    transcribe = TranscribeRecording(
        recordings,
        FfmpegChunker(),
        OpenAITranscriber(client, settings.transcribe_model),
        transcripts,
        settings.data_dir / "tmp",
    )
    summarize = SummarizeRecording(transcripts, LlmSummarizer(client, settings.llm_model), summaries)
    export = ExportToObsidian(recordings, transcripts, summaries, FileNoteStore(), settings.obsidian_path)
    runner = JobRunner(jobs, {"transcribe": transcribe.run, "summarize": summarize.run, "export": export.run})
    return Services(
        recordings=recordings,
        jobs=jobs,
        transcripts=transcripts,
        summaries=summaries,
        ingest=ingest,
        transcribe=transcribe,
        summarize=summarize,
        toggle_action=ToggleAction(summaries),
        export=export,
        runner=runner,
        inbox=InboxWatcher(settings.inbox_dir, ingest),
    )
