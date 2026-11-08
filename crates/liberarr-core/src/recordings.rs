//! Recordings context: the entity, its repository port and the ingest use case.
use crate::Result;
use crate::jobs::{JobQueue, STEPS};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    Device,
    File,
}

impl Source {
    pub fn as_str(self) -> &'static str {
        match self {
            Source::Device => "device",
            Source::File => "file",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Recording {
    pub id: i64,
    pub title: Option<String>,
    pub source: Source,
    pub source_ref: String,
    pub audio_path: PathBuf,
    pub duration_s: Option<f64>,
    pub created_at: jiff::Timestamp,
    pub export_path: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct NewRecording {
    pub title: Option<String>,
    pub source: Source,
    pub source_ref: String,
    pub audio_path: PathBuf,
    pub duration_s: Option<f64>,
    pub created_at: jiff::Timestamp,
}

/// Where a recording stands, derived from its latest job (the chain only advances on success).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecordingStatus {
    Processing,
    Failed,
    Done,
}

/// One row of the recordings list.
#[derive(Debug, Clone, PartialEq)]
pub struct RecordingItem {
    pub id: i64,
    pub title: Option<String>,
    pub source: Source,
    pub duration_s: Option<f64>,
    pub created_at: jiff::Timestamp,
    pub status: RecordingStatus,
    pub failed_step: Option<String>,
    /// Done with an empty transcript: no summary and no note were made.
    pub no_speech: bool,
}

pub trait RecordingRepo {
    async fn get(&self, id: i64) -> Result<Option<Recording>>;
    async fn find_by_ref(&self, source_ref: &str) -> Result<Option<i64>>;
    async fn add(&self, new: NewRecording) -> Result<i64>;
    async fn set_export_path(&self, id: i64, path: &Path) -> Result<()>;
    async fn list_items(&self) -> Result<Vec<RecordingItem>>;
}

pub struct StoredAudio {
    pub path: PathBuf,
    pub duration_s: Option<f64>,
}

/// Hashing and copying audio touch the filesystem, so they sit behind a port.
/// The original is stored untouched.
pub trait AudioStore {
    /// SHA-256 (hex) of the file content.
    fn fingerprint(&self, src: &Path) -> Result<String>;
    fn store(&self, src: &Path, fingerprint: &str) -> Result<StoredAudio>;
}

pub struct Ingest<'a, R, Q, S> {
    pub recordings: &'a R,
    pub jobs: &'a Q,
    pub store: &'a S,
}

impl<R: RecordingRepo, Q: JobQueue, S: AudioStore> Ingest<'_, R, Q, S> {
    /// Returns `(recording id, newly added)`. Identical audio (same SHA-256) is never ingested twice.
    /// `recorded_at` (e.g. a device session start) becomes `created_at`; `None` means now.
    pub async fn run(
        &self,
        src: &Path,
        source: Source,
        title: Option<String>,
        recorded_at: Option<jiff::Timestamp>,
    ) -> Result<(i64, bool)> {
        let hash = self.store.fingerprint(src)?;
        if let Some(id) = self.recordings.find_by_ref(&hash).await? {
            // A crash (or failed enqueue) after `add` strands the recording: re-enqueue on retry.
            if self.jobs.list_for(id).await?.is_empty() {
                self.jobs.enqueue(id, STEPS[0]).await?;
            }
            return Ok((id, false));
        }
        let stored = self.store.store(src, &hash)?;
        let id = self
            .recordings
            .add(NewRecording {
                title,
                source,
                source_ref: hash,
                audio_path: stored.path,
                duration_s: stored.duration_s,
                created_at: recorded_at.unwrap_or_else(jiff::Timestamp::now),
            })
            .await?;
        self.jobs.enqueue(id, STEPS[0]).await?;
        Ok((id, true))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fakes::{FakeStore, MemJobs, MemRecordings, block_on};

    #[test]
    fn ingest_dedups_by_hash() {
        let (recs, jobs, store) = (MemRecordings::default(), MemJobs::default(), FakeStore);
        let ingest = Ingest {
            recordings: &recs,
            jobs: &jobs,
            store: &store,
        };
        let first = block_on(ingest.run(Path::new("/in/a.ogg"), Source::File, None, None)).unwrap();
        // Same content (same "hash") arriving from another path is not ingested again.
        let again =
            block_on(ingest.run(Path::new("/elsewhere/a.ogg"), Source::File, None, None)).unwrap();
        assert_eq!(first, (1, true));
        assert_eq!(again, (1, false));
        assert_eq!(recs.0.borrow().len(), 1);
        let rows = jobs.rows();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].0, "transcribe");
    }
    #[test]
    fn ingest_requeues_recording_without_jobs() {
        let (recs, jobs, store) = (MemRecordings::default(), MemJobs::default(), FakeStore);
        let at: jiff::Timestamp = "2026-03-01T10:00:00Z".parse().unwrap();
        // Stranded: `add` succeeded but the first `enqueue` never happened.
        block_on(recs.add(NewRecording {
            title: None,
            source: Source::Device,
            source_ref: "a.ogg".into(),
            audio_path: "/audio/a.ogg".into(),
            duration_s: None,
            created_at: at,
        }))
        .unwrap();
        let ingest = Ingest {
            recordings: &recs,
            jobs: &jobs,
            store: &store,
        };
        let r =
            block_on(ingest.run(Path::new("/in/a.ogg"), Source::Device, None, Some(at))).unwrap();
        assert_eq!(r, (1, false));
        assert_eq!(jobs.rows().len(), 1);
        // Already has a job: a further retry must not enqueue again.
        block_on(ingest.run(Path::new("/in/a.ogg"), Source::Device, None, Some(at))).unwrap();
        assert_eq!(jobs.rows().len(), 1);
        // `recorded_at` becomes created_at on a fresh ingest.
        let (id, _) =
            block_on(ingest.run(Path::new("/in/b.ogg"), Source::Device, None, Some(at))).unwrap();
        assert_eq!(recs.0.borrow()[id as usize - 1].created_at, at);
    }
}
