//! Transcription context: transcript model, chunk merging and the transcribe use case.
use crate::recordings::RecordingRepo;
use crate::{Error, Result};
use std::path::Path;

/// Transcription APIs cap uploads at 25 MB; stay under.
pub const MAX_CHUNK_BYTES: usize = 24 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq)]
pub struct Segment {
    pub start: f64,
    pub end: f64,
    pub text: String,
}

/// What a provider returns for one chunk. `segments` is empty when it gives no timestamps.
#[derive(Debug, Clone, PartialEq)]
pub struct ChunkResult {
    pub language: Option<String>,
    pub text: String,
    pub segments: Vec<Segment>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Transcript {
    pub engine: String,
    pub language: Option<String>,
    pub text: String,
    pub segments: Vec<Segment>,
    /// False when the provider gave no segments: the text is shown without player sync.
    pub has_timestamps: bool,
}

/// `mm:ss`, minutes not wrapped at 60.
pub fn timestamp(seconds: f64) -> String {
    let s = seconds.max(0.0) as u64;
    format!("{:02}:{:02}", s / 60, s % 60)
}

/// Joins per-chunk results; every timestamp is shifted by its chunk's offset in the full recording.
pub fn merge_chunks(results: Vec<(ChunkResult, f64)>, engine: &str) -> Transcript {
    let language = results.iter().find_map(|(r, _)| r.language.clone());
    let text = results
        .iter()
        .map(|(r, _)| r.text.trim())
        .filter(|t| !t.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    let segments: Vec<Segment> = results
        .into_iter()
        .flat_map(|(r, offset)| {
            r.segments.into_iter().map(move |s| Segment {
                start: s.start + offset,
                end: s.end + offset,
                text: s.text,
            })
        })
        .collect();
    Transcript {
        engine: engine.into(),
        language,
        text,
        has_timestamps: !segments.is_empty(),
        segments,
    }
}

pub trait Transcriber {
    /// No forced language: recordings mix French and English.
    async fn transcribe(&self, audio: &[u8], filename: &str) -> Result<ChunkResult>;
    fn engine(&self) -> &str;
}

/// Splits audio into `(chunk bytes, offset in seconds)` of at most `max_bytes` each.
/// Refuses what it cannot split under the limit.
pub trait AudioSplitter {
    fn split(&self, path: &Path, max_bytes: usize) -> Result<Vec<(Vec<u8>, f64)>>;
}

pub trait TranscriptRepo {
    async fn save(&self, recording_id: i64, transcript: &Transcript) -> Result<()>;
    async fn get(&self, recording_id: i64) -> Result<Option<Transcript>>;
}

pub struct Transcribe<'a, R, P, T, S> {
    pub recordings: &'a R,
    pub splitter: &'a P,
    pub transcriber: &'a T,
    pub transcripts: &'a S,
}

impl<R: RecordingRepo, P: AudioSplitter, T: Transcriber, S: TranscriptRepo>
    Transcribe<'_, R, P, T, S>
{
    pub async fn run(&self, recording_id: i64) -> Result<()> {
        let rec = self
            .recordings
            .get(recording_id)
            .await?
            .ok_or_else(|| Error::NotFound("recording".into()))?;
        let ext = rec
            .audio_path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("ogg");
        let mut results = Vec::new();
        for (i, (bytes, offset)) in self
            .splitter
            .split(&rec.audio_path, MAX_CHUNK_BYTES)?
            .into_iter()
            .enumerate()
        {
            results.push((
                self.transcriber
                    .transcribe(&bytes, &format!("chunk-{i}.{ext}"))
                    .await?,
                offset,
            ));
        }
        self.transcripts
            .save(
                recording_id,
                &merge_chunks(results, self.transcriber.engine()),
            )
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seg(start: f64, text: &str) -> Segment {
        Segment {
            start,
            end: start + 1.0,
            text: text.into(),
        }
    }

    #[test]
    fn merge_offsets_timestamps() {
        let first = ChunkResult {
            language: Some("fr".into()),
            text: "a".into(),
            segments: vec![seg(1.0, "a")],
        };
        let second = ChunkResult {
            language: None,
            text: "b".into(),
            segments: vec![seg(1.0, "b")],
        };
        let t = merge_chunks(vec![(first, 0.0), (second, 1200.0)], "whisper");
        assert_eq!(t.segments[0].start, 1.0);
        assert_eq!(t.segments[1].start, 1201.0);
        assert_eq!(t.segments[1].end, 1202.0);
        assert_eq!(t.text, "a b");
        assert_eq!(t.language.as_deref(), Some("fr"));
        assert!(t.has_timestamps);
    }

    #[test]
    fn transcript_without_timestamps_is_kept() {
        let c = ChunkResult {
            language: None,
            text: " bonjour tout le monde ".into(),
            segments: vec![],
        };
        let t = merge_chunks(vec![(c, 0.0)], "whisper");
        assert!(!t.has_timestamps);
        assert!(t.segments.is_empty());
        assert_eq!(t.text, "bonjour tout le monde");
    }
}
