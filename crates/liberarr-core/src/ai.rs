//! AI providers and what each (provider, model) pair can do. Pure data: no I/O.

/// Transcription APIs cap uploads at 25 MB; stay under.
pub const DEFAULT_UPLOAD_BYTES: usize = 24 * 1024 * 1024;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Provider {
    /// Any OpenAI-compatible base URL (gateway such as LiteLLM).
    Compatible,
    OpenAi,
    Mistral,
    Anthropic,
    /// OpenAI-compatible chat through the user's own provider keys (BYOK), summaries only.
    OpenRouter,
    /// The user's logged-in `claude` CLI (subscription), summaries only.
    Claude,
}

impl Provider {
    pub fn parse(s: &str) -> Option<Provider> {
        Some(match s {
            "openai-compatible" => Provider::Compatible,
            "openai" => Provider::OpenAi,
            "mistral" => Provider::Mistral,
            "anthropic" => Provider::Anthropic,
            "openrouter" => Provider::OpenRouter,
            "claude" => Provider::Claude,
            _ => return None,
        })
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Provider::Compatible => "openai-compatible",
            Provider::OpenAi => "openai",
            Provider::Mistral => "mistral",
            Provider::Anthropic => "anthropic",
            Provider::OpenRouter => "openrouter",
            Provider::Claude => "claude",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Step {
    Transcribe,
    Summarize,
}

/// Whether a transcription model returns timestamped segments.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Timestamps {
    Segments,
    TextOnly,
    Unknown,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Capability {
    /// `None`: the provider cannot transcribe.
    pub transcription: Option<Timestamps>,
}

pub fn capability(p: Provider, model: &str) -> Capability {
    let transcription = match (p, model) {
        (Provider::Anthropic | Provider::OpenRouter | Provider::Claude, _) => None,
        (Provider::OpenAi, "whisper-1") => Some(Timestamps::Segments),
        (
            Provider::OpenAi,
            "gpt-transcribe"
            | "gpt-4o-transcribe"
            | "gpt-4o-mini-transcribe"
            | "gpt-4o-transcribe-diarize",
        ) => Some(Timestamps::TextOnly),
        (Provider::Mistral, m) if m.starts_with("voxtral") => Some(Timestamps::Segments),
        _ => Some(Timestamps::Unknown),
    };
    Capability { transcription }
}

pub fn supports(c: Capability, step: Step) -> bool {
    match step {
        Step::Transcribe => c.transcription.is_some(),
        // Every provider can summarize.
        Step::Summarize => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn whisper_has_segments_gpt4o_does_not() {
        assert_eq!(
            capability(Provider::OpenAi, "whisper-1").transcription,
            Some(Timestamps::Segments)
        );
        assert_eq!(
            capability(Provider::OpenAi, "gpt-4o-transcribe").transcription,
            Some(Timestamps::TextOnly)
        );
    }
}
