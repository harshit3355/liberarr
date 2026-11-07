//! What `init` proposes and warns about for each provider.
use liberarr_core::ai::{Capability, Provider, Step, Timestamps};

const OPENAI_URL: &str = "https://api.openai.com/v1";

/// `(default base URL, default model)`. Summary models have no default: they change too fast.
pub fn defaults(p: Provider, step: Step) -> (Option<&'static str>, Option<&'static str>) {
    match (p, step) {
        (Provider::OpenAi | Provider::Compatible, Step::Transcribe) => {
            (Some(OPENAI_URL), Some("whisper-1"))
        }
        (Provider::OpenAi | Provider::Compatible, Step::Summarize) => (Some(OPENAI_URL), None),
        (Provider::Mistral, Step::Transcribe) => (
            Some("https://api.mistral.ai/v1"),
            Some("voxtral-mini-transcribe-3"),
        ),
        (Provider::Mistral, Step::Summarize) => (Some("https://api.mistral.ai/v1"), None),
        (Provider::Anthropic, _) => (Some("https://api.anthropic.com"), None),
        // The base URL is the resource's own: it is asked in full.
        (Provider::Azure, _) => (None, None),
    }
}

pub fn warning(c: Capability, step: Step) -> Option<&'static str> {
    match (step, c.transcription) {
        (Step::Transcribe, Some(Timestamps::TextOnly | Timestamps::Unknown)) => {
            Some("pas d'horodatage garanti : la lecture ne sera pas synchronisée avec le texte")
        }
        _ => None,
    }
}
