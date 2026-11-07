//! What `init` proposes and warns about for each provider.
use liberarr_core::ai::{Capability, Provider, Step, Timestamps};
use rust_i18n::t;

const OPENAI_URL: &str = "https://api.openai.com/v1";
const OPENROUTER_URL: &str = "https://openrouter.ai/api/v1";

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
        (Provider::OpenRouter, _) => (Some(OPENROUTER_URL), None),
        (Provider::Claude, Step::Summarize) => (None, Some("sonnet")),
        (Provider::Anthropic | Provider::Claude, _) => (Some("https://api.anthropic.com"), None),
    }
}

pub fn warning(c: Capability, step: Step) -> Option<String> {
    match (step, c.transcription) {
        (Step::Transcribe, Some(Timestamps::TextOnly | Timestamps::Unknown)) => {
            Some(t!("providers.no_timestamps").into_owned())
        }
        _ => None,
    }
}
