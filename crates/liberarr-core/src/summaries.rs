//! Summaries context: LLM output validation, the summarize use case and action toggling.
use crate::transcription::{Transcript, TranscriptRepo, timestamp};
use crate::{Error, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
#[error("invalid summary: {0}")]
pub struct InvalidSummary(pub String);

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Action {
    pub action: String,
    #[serde(default)]
    pub owner: Option<String>,
    #[serde(default)]
    pub due: Option<String>,
    /// Seconds into the recording where the action is stated.
    #[serde(default)]
    pub start: Option<f64>,
    #[serde(default)]
    pub done: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MeetingSummary {
    pub title: String,
    pub summary_md: String,
    pub mindmap: String,
    #[serde(default)]
    pub actions: Vec<Action>,
}

/// LLM output is untrusted: it must be the exact JSON shape with a Mermaid mindmap.
pub fn parse_summary(text: &str) -> Result<MeetingSummary, InvalidSummary> {
    // serde messages can quote the model's text (summary content), so keep only category and position.
    let mut s: MeetingSummary = serde_json::from_str(text).map_err(|e| {
        InvalidSummary(format!(
            "{:?} error at line {} column {}",
            e.classify(),
            e.line(),
            e.column()
        ))
    })?;
    if !s.mindmap.trim_start().starts_with("mindmap") {
        return Err(InvalidSummary(
            "mindmap must be a Mermaid diagram starting with 'mindmap'".into(),
        ));
    }
    s.mindmap = s.mindmap.trim().to_string();
    Ok(s)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    System,
    User,
    Assistant,
}

impl Role {
    pub fn as_str(self) -> &'static str {
        match self {
            Role::System => "system",
            Role::User => "user",
            Role::Assistant => "assistant",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ChatMessage {
    pub role: Role,
    pub content: String,
}

impl ChatMessage {
    fn new(role: Role, content: impl Into<String>) -> Self {
        Self {
            role,
            content: content.into(),
        }
    }
}

const SYSTEM_PROMPT: &str = r#"Tu es l'assistant de compte rendu de réunion. On te donne la transcription d'un enregistrement (français, parfois mêlé d'anglais), horodatée ligne par ligne quand c'est possible. Réponds UNIQUEMENT avec un objet JSON de cette forme :
{
  "title": "titre court et explicite",
  "summary_md": "compte rendu en Markdown avec les sections ## Contexte, ## Points clés, ## Décisions",
  "mindmap": "carte mentale Mermaid commençant par la ligne 'mindmap', racine = sujet principal",
  "actions": [{"action": "...", "owner": "personne nommée dans la transcription ou null",
               "due": "échéance seulement si elle est dite, sinon null",
               "start": secondes du segment où l'action est énoncée, ou null}]
}
N'invente rien qui ne soit pas dans la transcription. Rédige en français."#;

/// System prompt plus the transcript: `[mm:ss] text` lines, or the plain text without timestamps.
pub fn summary_messages(t: &Transcript) -> Vec<ChatMessage> {
    let body = if t.has_timestamps {
        t.segments
            .iter()
            .map(|s| format!("[{}] {}", timestamp(s.start), s.text.trim()))
            .collect::<Vec<_>>()
            .join("\n")
    } else {
        t.text.clone()
    };
    vec![
        ChatMessage::new(Role::System, SYSTEM_PROMPT),
        ChatMessage::new(Role::User, body),
    ]
}

pub trait Llm {
    /// Returns the raw JSON text of the model's reply.
    async fn complete_json(&self, messages: &[ChatMessage]) -> Result<String>;
}

pub trait SummaryRepo {
    async fn save(&self, recording_id: i64, summary: &MeetingSummary) -> Result<()>;
    async fn get(&self, recording_id: i64) -> Result<Option<MeetingSummary>>;
}

pub struct Summarize<'a, T, L, S> {
    pub transcripts: &'a T,
    pub llm: &'a L,
    pub summaries: &'a S,
}

impl<T: TranscriptRepo, L: Llm, S: SummaryRepo> Summarize<'_, T, L, S> {
    /// One retry with the validation error appended, then `InvalidSummary`. Nothing is saved on failure.
    pub async fn run(&self, recording_id: i64) -> Result<MeetingSummary> {
        let t = self
            .transcripts
            .get(recording_id)
            .await?
            .ok_or_else(|| Error::NotFound("transcript".into()))?;
        let mut messages = summary_messages(&t);
        for attempt in 0..2 {
            let reply = self.llm.complete_json(&messages).await?;
            match parse_summary(&reply) {
                Ok(s) => {
                    self.summaries.save(recording_id, &s).await?;
                    return Ok(s);
                }
                Err(e) if attempt == 1 => return Err(e.into()),
                Err(e) => {
                    messages.push(ChatMessage::new(Role::Assistant, reply));
                    messages.push(ChatMessage::new(
                        Role::User,
                        format!(
                            "Réponse invalide : {}. Renvoie uniquement le JSON demandé, corrigé.",
                            e.0
                        ),
                    ));
                }
            }
        }
        unreachable!("the second attempt always returns")
    }
}

pub struct ToggleAction<'a, S> {
    pub summaries: &'a S,
}

impl<S: SummaryRepo> ToggleAction<'_, S> {
    pub async fn run(&self, recording_id: i64, index: usize, done: bool) -> Result<MeetingSummary> {
        let mut s = self
            .summaries
            .get(recording_id)
            .await?
            .ok_or_else(|| Error::NotFound("summary".into()))?;
        s.actions
            .get_mut(index)
            .ok_or_else(|| Error::NotFound("action".into()))?
            .done = done;
        self.summaries.save(recording_id, &s).await?;
        Ok(s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Error;
    use crate::fakes::{FakeLlm, MemSummaries, MemTranscripts, block_on};

    #[test]
    fn parse_summary_rejects_non_mindmap() {
        let json = r#"{"title":"t","summary_md":"s","mindmap":"graph TD\n A-->B","actions":[]}"#;
        assert!(parse_summary(json).is_err());
        let ok = r#"{"title":"t","summary_md":"s","mindmap":"mindmap\n  root","actions":[{"action":"x"}]}"#;
        assert_eq!(parse_summary(ok).unwrap().actions[0].owner, None);
    }

    #[test]
    fn summarize_retries_once_then_fails() {
        let bad_diagram =
            r#"{"title":"t","summary_md":"s","mindmap":"graph TD\n A-->B","actions":[]}"#;
        let llm = FakeLlm::new(&["not json", bad_diagram]);
        let transcripts = MemTranscripts::default();
        transcripts.map.borrow_mut().insert(
            1,
            Transcript {
                engine: "e".into(),
                language: None,
                text: "salut".into(),
                segments: vec![],
                has_timestamps: false,
            },
        );
        let summaries = MemSummaries::default();
        let uc = Summarize {
            transcripts: &transcripts,
            llm: &llm,
            summaries: &summaries,
        };

        let err = block_on(uc.run(1)).unwrap_err();

        assert!(matches!(err, Error::InvalidSummary(_)));
        assert_eq!(llm.calls.get(), 2);
        assert!(summaries.0.borrow().is_empty());
        assert_eq!(transcripts.saves.get(), 0);
    }
}
