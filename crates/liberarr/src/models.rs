//! Curated models `init` proposes. Data only: edit the table when a model is released or retired.
use liberarr_core::ai::{Provider, Step};

pub struct Entry {
    pub provider: Provider,
    pub step: Step,
    pub id: &'static str,
    /// Catalog key of the short description.
    pub label: &'static str,
    pub recommended: bool,
    /// Retirement date (`YYYY-MM-DD`), shown next to the model.
    pub deprecated: Option<&'static str>,
}

const fn entry(
    provider: Provider,
    step: Step,
    id: &'static str,
    label: &'static str,
    recommended: bool,
    deprecated: Option<&'static str>,
) -> Entry {
    Entry {
        provider,
        step,
        id,
        label,
        recommended,
        deprecated,
    }
}

use Provider::{Anthropic, Claude, Mistral, OpenAi};
use Step::{Summarize, Transcribe};

// No OpenAI chat model: none is named anywhere in the code, and they change too fast to guess.
pub const MODELS: &[Entry] = &[
    entry(
        Mistral,
        Transcribe,
        "voxtral-mini-transcribe-3",
        "model.voxtral_mini_transcribe_3",
        true,
        None,
    ),
    entry(
        Mistral,
        Summarize,
        "mistral-small-2603",
        "model.mistral_small_2603",
        true,
        None,
    ),
    entry(
        OpenAi,
        Transcribe,
        "whisper-1",
        "model.whisper_1",
        true,
        Some("2027-02-26"),
    ),
    entry(
        OpenAi,
        Transcribe,
        "gpt-transcribe",
        "model.gpt_transcribe",
        false,
        None,
    ),
    entry(
        Anthropic,
        Summarize,
        "claude-sonnet-5-5",
        "model.claude_sonnet_5_5",
        true,
        None,
    ),
    entry(
        Anthropic,
        Summarize,
        "claude-haiku-5-5",
        "model.claude_haiku_5_5",
        false,
        None,
    ),
    entry(Claude, Summarize, "sonnet", "model.cli_sonnet", true, None),
    entry(Claude, Summarize, "opus", "model.cli_opus", false, None),
    entry(Claude, Summarize, "haiku", "model.cli_haiku", false, None),
];

/// Curated entries for a provider and step. A gateway is proposed the OpenAI models.
pub fn curated(provider: Provider, step: Step) -> impl Iterator<Item = &'static Entry> {
    let provider = if provider == Provider::Compatible {
        Provider::OpenAi
    } else {
        provider
    };
    MODELS
        .iter()
        .filter(move |e| e.provider == provider && e.step == step)
}

/// Whether a model id returned by a provider can do `step`.
pub fn usable(id: &str, step: Step) -> bool {
    let has = |words: &[&str]| words.iter().any(|w| id.contains(w));
    match step {
        // `voxtral` also names text-to-speech and realtime models.
        Step::Transcribe => {
            has(&["transcribe", "whisper", "voxtral"]) && !has(&["tts", "realtime"])
        }
        Step::Summarize => !has(&[
            "embed",
            "moderation",
            "transcribe",
            "whisper",
            "voxtral",
            "tts",
            "image",
            "realtime",
        ]),
    }
}
