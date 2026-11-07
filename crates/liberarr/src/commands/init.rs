//! `liberarr init`: creates the home folder and its `.env` (0600). Idempotent: a value already
//! set is never overwritten; only missing, empty or placeholder values are generated or asked.
use super::{Outcome, ask};
use crate::config::{self, is_unset, parse_env};
use std::io::Write;
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::process::ExitCode;

const DEFAULT_BASE_URL: &str = "https://api.openai.com/v1";

/// `(variable, question, default, hidden)`.
const PROMPTS: [(&str, &str, Option<&str>, bool); 6] = [
    (
        "LIBERARR_TRANSCRIBE_BASE_URL",
        "URL du service de transcription",
        Some(DEFAULT_BASE_URL),
        false,
    ),
    (
        "LIBERARR_TRANSCRIBE_API_KEY",
        "Clé API de transcription",
        None,
        true,
    ),
    (
        "LIBERARR_TRANSCRIBE_MODEL",
        "Modèle de transcription",
        Some("whisper-1"),
        false,
    ),
    (
        "LIBERARR_LLM_BASE_URL",
        "URL du service de résumé (LLM)",
        Some(DEFAULT_BASE_URL),
        false,
    ),
    ("LIBERARR_LLM_API_KEY", "Clé API du LLM", None, true),
    ("LIBERARR_LLM_MODEL", "Modèle du LLM", None, false),
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
    for (k, question, default, hidden) in PROMPTS {
        if missing(k) {
            updates.push((k, prompt(question, default, hidden)?));
        }
    }
    if updates.is_empty() {
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
