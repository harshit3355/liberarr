//! `<LIBERARR_HOME>/.env` is the one source of secrets and settings (only `LIBERARR_HOME`, which
//! locates it, comes from the environment). Errors name the variable, never its value.
use liberarr_core::ai::{Provider, Step, capability, supports};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

pub const PLACEHOLDER: &str = "change-me";
/// Minimum length of `LIBERARR_SECRET_KEY` and `LIBERARR_API_KEY`.
const MIN_SECRET: usize = 32;

pub struct Profile {
    pub provider: Provider,
    pub base_url: String,
    pub api_key: String,
    pub model: String,
}

pub struct Config {
    pub home: PathBuf,
    pub secret_key: String,
    pub api_key: String,
    pub admin_password: String,
    pub transcribe: Profile,
    pub llm: Profile,
    pub obsidian: PathBuf,
    /// Set the `Secure` cookie flag (serving behind HTTPS).
    pub cookie_secure: bool,
}

/// `LIBERARR_HOME`, default `~/.liberarr`.
pub fn home() -> Result<PathBuf, String> {
    if let Some(h) = std::env::var_os("LIBERARR_HOME").filter(|h| !h.is_empty()) {
        return Ok(h.into());
    }
    std::env::var_os("HOME")
        .map(|h| PathBuf::from(h).join(".liberarr"))
        .ok_or_else(|| "HOME n'est pas défini : définissez LIBERARR_HOME".into())
}

/// `KEY=VALUE` lines; blank lines and `#` comments are skipped, surrounding quotes removed.
pub fn parse_env(text: &str) -> HashMap<String, String> {
    text.lines()
        .filter_map(|l| {
            let l = l.trim();
            if l.starts_with('#') {
                return None;
            }
            let (k, v) = l.split_once('=')?;
            let v = v.trim();
            let v = v
                .strip_prefix('"')
                .and_then(|v| v.strip_suffix('"'))
                .unwrap_or(v);
            Some((k.trim().to_string(), v.to_string()))
        })
        .collect()
}

pub fn is_unset(v: &str) -> bool {
    let v = v.trim();
    v.is_empty() || v == PLACEHOLDER
}

pub fn load(home: &Path) -> Result<Config, String> {
    let file = match std::fs::read_to_string(home.join(".env")) {
        Ok(t) => parse_env(&t),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Err(format!(
                "{} introuvable : lancez d'abord `liberarr init`",
                home.join(".env").display()
            ));
        }
        Err(e) => return Err(format!("impossible de lire le fichier .env : {}", e.kind())),
    };
    load_from(home, |k| file.get(k).cloned())
}

fn load_from(home: &Path, get: impl Fn(&str) -> Option<String>) -> Result<Config, String> {
    let need = |k: &str| match get(k) {
        Some(v) if !is_unset(&v) => Ok(v.trim().to_string()),
        _ => Err(format!(
            "{k} est absent, vide ou encore un exemple dans {}/.env : lancez `liberarr init`",
            home.display()
        )),
    };
    let secret = |k: &str| {
        let v = need(k)?;
        if v.chars().count() < MIN_SECRET {
            return Err(format!(
                "{k} est trop court (au moins {MIN_SECRET} caractères) : supprimez la ligne et relancez `liberarr init`"
            ));
        }
        Ok(v)
    };
    let profile = |p: &str, step: Step| -> Result<Profile, String> {
        let var = format!("LIBERARR_{p}_PROVIDER");
        let provider = match get(&var).filter(|v| !v.trim().is_empty()) {
            None => Provider::Compatible,
            Some(v) => Provider::parse(v.trim()).ok_or_else(|| {
                format!("{var} n'est pas reconnu (openai-compatible, openai, mistral, anthropic, claude)")
            })?,
        };
        let model = need(&format!("LIBERARR_{p}_MODEL"))?;
        if !supports(capability(provider, &model), step) {
            let verb = match step {
                Step::Transcribe => "transcrire",
                Step::Summarize => "résumer",
            };
            return Err(format!("{var} : ce fournisseur ne sait pas {verb}"));
        }
        // The claude CLI uses its own login: no URL, no key.
        let (base_url, api_key) = if provider == Provider::Claude {
            (String::new(), String::new())
        } else {
            (
                need(&format!("LIBERARR_{p}_BASE_URL"))?,
                need(&format!("LIBERARR_{p}_API_KEY"))?,
            )
        };
        Ok(Profile {
            provider,
            base_url,
            api_key,
            model,
        })
    };
    Ok(Config {
        home: home.to_path_buf(),
        secret_key: secret("LIBERARR_SECRET_KEY")?,
        api_key: secret("LIBERARR_API_KEY")?,
        admin_password: need("LIBERARR_ADMIN_PASSWORD")?,
        transcribe: profile("TRANSCRIBE", Step::Transcribe)?,
        llm: profile("LLM", Step::Summarize)?,
        obsidian: get("LIBERARR_OBSIDIAN_DIR")
            .filter(|v| !is_unset(v))
            .map_or_else(|| home.join("obsidian"), PathBuf::from),
        cookie_secure: get("LIBERARR_COOKIE_SECURE").is_some_and(|v| v.trim() == "true"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const VALID: &str = "LIBERARR_SECRET_KEY=0123456789abcdef0123456789abcdef
LIBERARR_API_KEY=fedcba9876543210fedcba9876543210
LIBERARR_ADMIN_PASSWORD=pw
LIBERARR_TRANSCRIBE_BASE_URL=https://api.openai.com/v1
LIBERARR_TRANSCRIBE_API_KEY=sk-a
LIBERARR_TRANSCRIBE_MODEL=whisper-1
LIBERARR_LLM_BASE_URL=https://api.openai.com/v1
LIBERARR_LLM_API_KEY=sk-b
LIBERARR_LLM_MODEL=gpt
";

    fn load_text(text: &str) -> Result<Config, String> {
        let vars = parse_env(text);
        load_from(Path::new("/h"), |k| vars.get(k).cloned())
    }

    fn with(key: &str, value: &str) -> String {
        VALID
            .lines()
            .map(|l| {
                if l.starts_with(&format!("{key}=")) {
                    format!("{key}={value}")
                } else {
                    l.to_string()
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn config_refuses_placeholder_without_echoing() {
        assert!(load_text(VALID).is_ok());
        for (key, value) in [
            ("LIBERARR_SECRET_KEY", "change-me"),
            ("LIBERARR_SECRET_KEY", ""),
            (
                "LIBERARR_SECRET_KEY",
                "short-secret-value-31-chars-xxxx"[..31].as_ref(),
            ),
            ("LIBERARR_API_KEY", "\"change-me\""),
            ("LIBERARR_API_KEY", "only-twenty-chars-ab"),
            ("LIBERARR_LLM_API_KEY", "change-me"),
        ] {
            let msg = load_text(&with(key, value))
                .err()
                .unwrap_or_else(|| panic!("{key}={value} accepted"));
            assert!(msg.contains(key), "{msg}");
            assert!(
                value.is_empty() || !msg.contains(value.trim_matches('"')),
                "{msg}"
            );
        }
        // A missing line is refused like an empty one.
        let missing: String = VALID
            .lines()
            .filter(|l| !l.starts_with("LIBERARR_SECRET_KEY"))
            .collect::<Vec<_>>()
            .join("\n");
        assert!(
            load_text(&missing)
                .err()
                .is_some_and(|m| m.contains("LIBERARR_SECRET_KEY"))
        );
    }

    fn provider_line(key: &str, value: &str) -> String {
        format!("{VALID}{key}={value}\n")
    }

    #[test]
    fn claude_cannot_be_the_transcriber() {
        let msg = load_text(&provider_line("LIBERARR_TRANSCRIBE_PROVIDER", "anthropic"))
            .err()
            .unwrap();
        assert!(msg.contains("LIBERARR_TRANSCRIBE_PROVIDER"), "{msg}");
        assert!(!msg.contains("sk-a"), "{msg}");
    }
}
