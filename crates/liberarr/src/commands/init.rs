//! `liberarr init`: creates the home folder and its `.env` (0600). Idempotent: a value already
//! set is never overwritten; only missing, empty or placeholder values are generated or asked.
use super::{Outcome, ask};
use crate::config::{self, is_unset, parse_env};
use crate::providers;
use liberarr_core::ai::{Provider, Step, capability, supports};
use std::io::Write;
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt};
use std::process::ExitCode;

/// Per step: the step, the variables (provider, base URL, API key, model) and the questions
/// (provider, base URL, API key, model).
type StepPrompts = (Step, [&'static str; 4], [&'static str; 4]);

const STEPS: [StepPrompts; 2] = [
    (
        Step::Transcribe,
        [
            "LIBERARR_TRANSCRIBE_PROVIDER",
            "LIBERARR_TRANSCRIBE_BASE_URL",
            "LIBERARR_TRANSCRIBE_API_KEY",
            "LIBERARR_TRANSCRIBE_MODEL",
        ],
        [
            "Fournisseur de transcription",
            "URL du service de transcription",
            "Clé API de transcription",
            "Modèle de transcription",
        ],
    ),
    (
        Step::Summarize,
        [
            "LIBERARR_LLM_PROVIDER",
            "LIBERARR_LLM_BASE_URL",
            "LIBERARR_LLM_API_KEY",
            "LIBERARR_LLM_MODEL",
        ],
        [
            "Fournisseur du résumé (LLM)",
            "URL du service de résumé (LLM)",
            "Clé API du LLM",
            "Modèle du LLM",
        ],
    ),
];

fn random_hex(bytes: usize) -> Result<String, String> {
    let mut buf = vec![0u8; bytes];
    getrandom::fill(&mut buf).map_err(|_| "aléa système indisponible".to_string())?;
    Ok(buf.iter().map(|b| format!("{b:02x}")).collect())
}

/// Asks until a non-empty answer (or the default) is given.
fn prompt(question: &str, default: Option<&str>, hidden: bool) -> std::io::Result<String> {
    let label = match default {
        Some(d) => format!("{question} [{d}] : "),
        None => format!("{question} : "),
    };
    loop {
        let v = ask(&label, hidden)?;
        match (v.is_empty(), default) {
            (false, _) => return Ok(v),
            (true, Some(d)) => return Ok(d.to_string()),
            (true, None) => eprintln!("Une valeur est requise."),
        }
    }
}

/// Asks until the answer is a provider that can do `step`. `openai` is the default: it needs no
/// capability warning with the default model.
fn ask_provider(question: &str, step: Step) -> std::io::Result<Provider> {
    loop {
        let label = format!("{question} (openai, openai-compatible, mistral, anthropic, claude)");
        let v = prompt(&label, Some("openai"), false)?;
        match Provider::parse(v.trim()) {
            Some(p) if supports(capability(p, ""), step) => return Ok(p),
            _ => eprintln!("Fournisseur inconnu, ou qui ne sait pas faire cette étape."),
        }
    }
}

/// True when `claude auth status` exits 0 within a few seconds. Output is discarded: no token is read.
fn claude_connected() -> bool {
    let Ok(mut child) = std::process::Command::new("claude")
        .args(["auth", "status"])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
    else {
        return false;
    };
    for _ in 0..50 {
        match child.try_wait() {
            Ok(Some(s)) => return s.success(),
            Ok(None) => std::thread::sleep(std::time::Duration::from_millis(100)),
            Err(_) => return false,
        }
    }
    let _ = child.kill();
    let _ = child.wait();
    false
}

/// Replaces the lines of `updates` in place, appends the others, keeps everything else as is.
fn merge(existing: &str, updates: &[(&str, String)]) -> String {
    let mut pending: Vec<&(&str, String)> = updates.iter().collect();
    let mut out: Vec<String> = existing
        .lines()
        .map(|line| {
            let key = line.split_once('=').map(|(k, _)| k.trim());
            match pending.iter().position(|(k, _)| Some(*k) == key) {
                Some(i) => {
                    let (k, v) = pending.remove(i);
                    format!("{k}={v}")
                }
                None => line.to_string(),
            }
        })
        .collect();
    out.extend(pending.into_iter().map(|(k, v)| format!("{k}={v}")));
    out.join("\n") + "\n"
}

pub fn run() -> Outcome {
    let home = config::home()?;
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(&home)?;
    let path = home.join(".env");
    let existing = match std::fs::read_to_string(&path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => {
            return Err(format!("impossible de lire {} : {}", path.display(), e.kind()).into());
        }
    };
    let current = parse_env(&existing);
    let missing = |k: &str| current.get(k).is_none_or(|v| is_unset(v));

    let mut updates: Vec<(&str, String)> = Vec::new();
    for k in ["LIBERARR_SECRET_KEY", "LIBERARR_API_KEY"] {
        if missing(k) {
            updates.push((k, random_hex(32)?));
        }
    }
    let password = if missing("LIBERARR_ADMIN_PASSWORD") {
        let p = random_hex(16)?;
        updates.push(("LIBERARR_ADMIN_PASSWORD", p.clone()));
        Some(p)
    } else {
        None
    };
    for (step, [provider_var, url_var, key_var, model_var], [provider_q, url_q, key_q, model_q]) in
        STEPS
    {
        // Asked only when neither the provider nor the base URL is set: never overwrites a choice.
        let provider = if missing(provider_var) && missing(url_var) {
            if step == Step::Summarize && claude_connected() {
                println!(
                    "Claude détecté et connecté : le fournisseur « claude » utilise ton abonnement via la CLI officielle. Usage toléré mais non garanti par Anthropic ; la clé API (« anthropic ») est recommandée. Tes transcriptions sont envoyées à Anthropic selon les réglages de confidentialité de ton compte."
                );
            }
            let p = ask_provider(provider_q, step)?;
            updates.push((provider_var, p.as_str().to_string()));
            p
        } else {
            current
                .get(provider_var)
                .and_then(|v| Provider::parse(v.trim()))
                .unwrap_or(Provider::Compatible)
        };
        let (default_url, default_model) = providers::defaults(provider, step);
        // The claude CLI has its own login: no URL, no key.
        let needs_api = provider != Provider::Claude;
        if needs_api && missing(url_var) {
            updates.push((url_var, prompt(url_q, default_url, false)?));
        }
        if needs_api && missing(key_var) {
            updates.push((key_var, prompt(key_q, None, true)?));
        }
        if missing(model_var) {
            let model = prompt(model_q, default_model, false)?;
            if let Some(w) = providers::warning(capability(provider, &model), step) {
                println!("Attention : {w}.");
            }
            updates.push((model_var, model));
        }
    }
    if updates.is_empty() {
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
        println!("{} est déjà complet : rien à changer.", path.display());
        return Ok(ExitCode::SUCCESS);
    }

    // Temp file created 0600, then renamed: the secrets are never readable by others, even briefly.
    let tmp = home.join(".env.tmp");
    let _ = std::fs::remove_file(&tmp);
    let mut f = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&tmp)?;
    f.write_all(merge(&existing, &updates).as_bytes())?;
    f.sync_all()?;
    std::fs::rename(&tmp, &path)?;

    println!("Configuration enregistrée dans {}.", path.display());
    if let Some(p) = password {
        println!("Connexion à l'interface : identifiant « admin », mot de passe : {p}");
        println!("(affiché une seule fois ; il reste dans .env sous LIBERARR_ADMIN_PASSWORD)");
    }
    println!("Étape suivante : `liberarr link`, puis `liberarr pull`.");
    Ok(ExitCode::SUCCESS)
}
