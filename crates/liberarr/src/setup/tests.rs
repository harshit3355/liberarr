use super::*;
use crate::commands::init::merge;
use crate::config::parse_env;
use std::collections::VecDeque;

const ENV: &str = "# my notes
LIBERARR_SECRET_KEY=aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa
LIBERARR_API_KEY=bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb
LIBERARR_ADMIN_PASSWORD=admin-pw-secret
LIBERARR_TRANSCRIBE_PROVIDER=mistral
LIBERARR_TRANSCRIBE_BASE_URL=https://api.mistral.ai/v1
LIBERARR_TRANSCRIBE_API_KEY=stored-transcribe-key
LIBERARR_TRANSCRIBE_MODEL=voxtral-mini-transcribe-3
LIBERARR_LLM_PROVIDER=anthropic
LIBERARR_LLM_BASE_URL=https://api.anthropic.com
LIBERARR_LLM_API_KEY=stored-llm-key
LIBERARR_LLM_MODEL=claude-sonnet-5-5
";
const SECRETS: [&str; 3] = [
    "LIBERARR_SECRET_KEY",
    "LIBERARR_API_KEY",
    "LIBERARR_ADMIN_PASSWORD",
];
const TYPED_KEY: &str = "typed-new-key-123";

/// Answers the questions with scripted values and records everything it is shown.
#[derive(Default)]
struct Fake {
    choices: VecDeque<Picked>,
    secrets: VecDeque<&'static str>,
    confirms: VecDeque<bool>,
    seen: Vec<String>,
}

impl Prompter for Fake {
    fn is_interactive(&self) -> bool {
        true
    }
    fn choose(&mut self, q: &str, o: &[Choice], _: Option<usize>) -> std::io::Result<Picked> {
        self.seen.push(q.to_string());
        self.seen
            .extend(o.iter().map(|c| format!("{} {}", c.value, c.note)));
        Ok(self.choices.pop_front().expect("unexpected choice"))
    }
    fn text(&mut self, q: &str, d: Option<&str>) -> std::io::Result<String> {
        self.seen.push(format!("{q} {d:?}"));
        Ok(d.expect("unexpected text").to_string())
    }
    fn secret(&mut self, q: &str) -> std::io::Result<String> {
        self.seen.push(q.to_string());
        Ok(self.secrets.pop_front().expect("unexpected secret").into())
    }
    fn confirm(&mut self, q: &str, _: bool) -> std::io::Result<bool> {
        self.seen.push(q.to_string());
        Ok(self.confirms.pop_front().expect("unexpected confirm"))
    }
    fn info(&mut self, line: &str) {
        self.seen.push(line.to_string());
    }
    fn warn(&mut self, line: &str) {
        self.seen.push(line.to_string());
    }
    fn busy(&mut self, _: &str, work: &mut dyn FnMut()) {
        work();
    }
}

fn ok(_: Provider, _: &str, _: &str) -> Result<Vec<String>, String> {
    Ok(vec![])
}

fn written(plan: &Plan) -> HashMap<String, String> {
    parse_env(&merge(ENV, &plan.updates))
}

#[test]
fn set_values_are_never_overwritten_or_re_asked() {
    // A .env missing only the LLM model: nothing else may be asked or changed.
    let partial = ENV.replace("LIBERARR_LLM_MODEL=claude-sonnet-5-5\n", "");
    let env = parse_env(&partial);
    let mut p = Fake::default();
    p.choices.push_back(Picked::Text("my-model".into()));
    let plan = run_setup(&mut p, &env, &ok).unwrap();
    assert_eq!(
        plan.updates,
        [("LIBERARR_LLM_MODEL", "my-model".to_string())]
    );
    assert!(plan.password.is_none());
}

#[test]
fn reconfigure_keeps_the_secrets_and_never_shows_a_secret() {
    let env = parse_env(ENV);
    let mut p = Fake::default();
    // Keep or reconfigure: reconfigure. Transcription: mistral again (index 2 of 4), key kept,
    // first model. Summary: switch to openai (index 0), which forces a new key; a custom model.
    p.choices.extend([
        Picked::Option(1),
        Picked::Option(2),
        Picked::Option(0),
        Picked::Option(0),
        Picked::Text("gpt-custom".into()),
    ]);
    p.confirms.push_back(false);
    p.secrets.push_back(TYPED_KEY);
    let plan = run_setup(&mut p, &env, &ok).unwrap();

    let out = written(&plan);
    for k in SECRETS {
        assert_eq!(out[k], parse_env(ENV)[k], "{k} changed");
        assert!(plan.updates.iter().all(|(u, _)| *u != k), "{k} rewritten");
    }
    assert_eq!(out["LIBERARR_TRANSCRIBE_API_KEY"], "stored-transcribe-key");
    assert_eq!(out["LIBERARR_LLM_PROVIDER"], "openai");
    assert_eq!(out["LIBERARR_LLM_API_KEY"], TYPED_KEY);
    assert_eq!(out["LIBERARR_LLM_MODEL"], "gpt-custom");
    assert!(plan.password.is_none());

    // Nothing the front-end was shown contains a secret, stored or typed.
    let seen = p.seen.join("\n");
    for secret in [
        TYPED_KEY,
        "stored-transcribe-key",
        "stored-llm-key",
        "admin-pw-secret",
        &env["LIBERARR_SECRET_KEY"],
        &env["LIBERARR_API_KEY"],
    ] {
        assert!(!seen.contains(secret), "{secret} reached the front-end");
    }
}
