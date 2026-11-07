//! `liberarr init`: creates the home folder and its `.env` (0600). Idempotent: a value already
//! set is never overwritten; only missing, empty or placeholder values are generated or asked.
//! The questions live in `setup`; this file reads and writes the `.env`.
use super::Outcome;
use crate::config::{self, parse_env};
use crate::i18n;
use crate::setup::{self, PlainPrompter};
use rust_i18n::t;
use std::io::Write;
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt};
use std::process::ExitCode;

/// Replaces the lines of `updates` in place, appends the others, keeps everything else as is.
pub(crate) fn merge(existing: &str, updates: &[(&str, String)]) -> String {
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

pub fn run(non_interactive: bool) -> Outcome {
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
            return Err(
                t!("init.cannot_read", path = path.display(), kind = e.kind())
                    .into_owned()
                    .into(),
            );
        }
    };
    let current = parse_env(&existing);
    let ui = i18n::language(current.get("LIBERARR_LANG").map(String::as_str));
    let plan = if non_interactive {
        setup::from_env(&current, ui, |k| std::env::var(k).ok())?
    } else {
        let rt = tokio::runtime::Runtime::new()?;
        setup::run_setup(&mut PlainPrompter, &current, ui, &|p, url, key| {
            setup::connect(&rt, p, url, key)
        })?
    };
    let setup::Plan { updates, password } = plan;
    if updates.is_empty() {
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
        println!("{}", t!("init.already_complete", path = path.display()));
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

    println!("{}", t!("init.saved", path = path.display()));
    if let Some(p) = password {
        println!("{}", t!("init.login", password = p));
        println!("{}", t!("init.login_note"));
    }
    println!("{}", t!("init.next_steps"));
    Ok(ExitCode::SUCCESS)
}
