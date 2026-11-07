//! Manual live probe of the AI adapters. Reads only process environment variables and prints one
//! line per case: booleans, counts and the adapters' already-sanitized errors. Never text, audio,
//! summaries, keys or URLs.
//!
//! Variables (all optional, a target with missing ones is skipped): FOUNDRY_BASE_URL,
//! FOUNDRY_API_KEY, FOUNDRY_AUDIO_DEPLOYMENTS (comma separated), FOUNDRY_CHAT_DEPLOYMENT,
//! MISTRAL_API_KEY, OPENAI_API_KEY, PROBE_AUDIO (audio file, needed for transcription).
use liberarr_core::Result;
use liberarr_core::summaries::{ChatMessage, Llm, Role};
use liberarr_core::transcription::{ChunkResult, Transcriber};
use liberarr_infra::ai::OpenAiCompat;
use liberarr_infra::azure::AzureTranscriber;
use liberarr_infra::mistral::MistralTranscriber;

const MISTRAL_BASE: &str = "https://api.mistral.ai/v1";
const OPENAI_BASE: &str = "https://api.openai.com/v1";
const MISTRAL_MODELS: [&str; 4] = [
    "voxtral-mini-latest",
    "voxtral-mini-transcribe-3",
    "voxtral-mini-transcribe-3-0",
    "voxtral-mini-2602",
];
const OPENAI_MODELS: [&str; 2] = ["gpt-transcribe", "whisper-1"];

fn var(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|v| !v.trim().is_empty())
}

fn show_transcript(label: &str, r: Result<ChunkResult>) {
    match r {
        Ok(c) => println!(
            "{label}: ok text={} segments={} language={}",
            if c.text.trim().is_empty() {
                "no"
            } else {
                "yes"
            },
            c.segments.len(),
            c.language.as_deref().unwrap_or("none")
        ),
        Err(e) => println!("{label}: error {e}"),
    }
}

/// Asks for a tiny JSON object and reports only whether the reply parses.
async fn show_chat(label: &str, llm: &impl Llm) {
    let ask = [ChatMessage {
        role: Role::User,
        content: r#"Reply with the JSON object {"ok":true} and nothing else."#.into(),
    }];
    match llm.complete_json(&ask).await {
        Ok(text) => {
            let valid = serde_json::from_str::<serde_json::Value>(&text).is_ok();
            println!(
                "{label}: ok json={}",
                if valid { "valid" } else { "invalid" }
            );
        }
        Err(e) => println!("{label}: error {e}"),
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let audio = match var("PROBE_AUDIO") {
        Some(path) => Some(std::fs::read(path)?),
        None => None,
    };
    let filename = "probe.ogg";

    let foundry = var("FOUNDRY_BASE_URL").zip(var("FOUNDRY_API_KEY"));
    match (&foundry, &audio, var("FOUNDRY_AUDIO_DEPLOYMENTS")) {
        (Some((base, key)), Some(audio), Some(deployments)) => {
            for d in deployments
                .split(',')
                .map(str::trim)
                .filter(|d| !d.is_empty())
            {
                let t = AzureTranscriber::new(base.clone(), key.clone(), d.into());
                show_transcript(&format!("foundry/{d}"), t.transcribe(audio, filename).await);
            }
        }
        _ => println!("foundry transcription: skipped"),
    }
    match (&foundry, var("FOUNDRY_CHAT_DEPLOYMENT")) {
        (Some((base, key)), Some(chat)) => {
            let llm = OpenAiCompat::new(base.clone(), key.clone(), chat, true);
            show_chat("foundry/chat", &llm).await;
        }
        _ => println!("foundry chat: skipped"),
    }

    match (var("MISTRAL_API_KEY"), &audio) {
        (Some(key), Some(audio)) => {
            for m in MISTRAL_MODELS {
                let t = MistralTranscriber::new(MISTRAL_BASE.into(), key.clone(), m.into());
                show_transcript(&format!("mistral/{m}"), t.transcribe(audio, filename).await);
            }
        }
        _ => println!("mistral transcription: skipped"),
    }
    match var("MISTRAL_API_KEY") {
        Some(key) => {
            let llm =
                OpenAiCompat::new(MISTRAL_BASE.into(), key, "mistral-small-2603".into(), true);
            show_chat("mistral/chat", &llm).await;
        }
        None => println!("mistral chat: skipped"),
    }

    match (var("OPENAI_API_KEY"), &audio) {
        (Some(key), Some(audio)) => {
            for m in OPENAI_MODELS {
                for (format, segments) in [("verbose_json", true), ("json", false)] {
                    let t = OpenAiCompat::new(OPENAI_BASE.into(), key.clone(), m.into(), segments);
                    show_transcript(
                        &format!("openai/{m}/{format}"),
                        t.transcribe(audio, filename).await,
                    );
                }
            }
        }
        _ => println!("openai transcription: skipped"),
    }
    Ok(())
}
